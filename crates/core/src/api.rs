//! Fachada de alto nivel para la app (Tauri) y el servidor de desarrollo.
//! Mantiene los dibujos importados en memoria y ejecuta trabajos largos en hilos
//! con progreso y cancelación.

use crate::analyze::{self, AnalyzeOptions, AnalyzeResult, LayerRole, ScaleGuess};
use crate::engine::search::{self, Progress};
use crate::import::{self, Drawing, SourceKind};
use crate::model::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Default)]
struct Store {
    drawings: HashMap<String, Arc<Drawing>>,
    jobs: HashMap<String, Arc<Job>>,
}

struct Job {
    cancel: AtomicBool,
    state: Mutex<JobState>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobState {
    pub done: bool,
    pub progress: Progress,
    pub result: Option<Value>,
    pub error: Option<String>,
    /// Mejor resultado parcial (solo optimización), para ir dibujando.
    pub partial: Option<Value>,
}

static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
static COUNTER: AtomicU64 = AtomicU64::new(1);

fn store() -> &'static Mutex<Store> {
    STORE.get_or_init(|| Mutex::new(Store::default()))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Defaults {
    pub catalog: Vec<DeskType>,
    pub rules: Vec<Rule>,
    pub weights: Weights,
    pub settings: Settings,
}

pub fn defaults() -> Defaults {
    let c = crate::defaults::default_config();
    Defaults { catalog: c.catalog, rules: c.rules, weights: c.weights, settings: c.settings }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerOut {
    pub name: String,
    pub color: String,
    pub paths: usize,
    pub role: LayerRole,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOut {
    pub drawing_id: String,
    pub name: String,
    pub source: SourceKind,
    pub layers: Vec<LayerOut>,
    pub scale: ScaleGuess,
    pub bounds: [f64; 4],
    pub unit_note: String,
}

pub fn import_bytes(name: &str, bytes: &[u8]) -> Result<ImportOut, String> {
    let d = import::import_bytes(name, bytes)?;
    Ok(register(d))
}

pub fn import_dwg(path: &std::path::Path, converter: Option<&std::path::Path>) -> Result<ImportOut, String> {
    let dxf = import::dwg::convert_dwg(path, converter)?;
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or("plano.dwg".into());
    let mut d = import::dxf_in::read_dxf(&name, &dxf)?;
    d.source = SourceKind::Dwg;
    Ok(register(d))
}

fn register(d: Drawing) -> ImportOut {
    let roles = analyze::suggest_roles(&d);
    let scale = analyze::guess_scale(&d, &roles);
    let id = format!("dwg{}", COUNTER.fetch_add(1, Ordering::Relaxed));
    let out = ImportOut {
        drawing_id: id.clone(),
        name: d.name.clone(),
        source: d.source,
        layers: d.layers.iter().filter(|l| l.paths > 0).map(|l| LayerOut { name: l.name.clone(), color: l.color.clone(), paths: l.paths, role: roles[&l.name] }).collect(),
        scale,
        bounds: [d.bounds.min.x, d.bounds.min.y, d.bounds.max.x, d.bounds.max.y],
        unit_note: d.unit_note.clone(),
    };
    store().lock().unwrap().drawings.insert(id, Arc::new(d));
    out
}

fn drawing(id: &str) -> Result<Arc<Drawing>, String> {
    store().lock().unwrap().drawings.get(id).cloned().ok_or_else(|| "El dibujo ya no está en memoria; vuelve a importarlo.".into())
}

/// Geometría de fondo por capa: listas planas [x0,y0,x1,y1,…] en mm, redondeadas a 1 mm.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BgLayer {
    pub name: String,
    pub color: String,
    pub paths: Vec<Vec<f32>>,
}

pub fn background(id: &str, mm_per_unit: f64) -> Result<Vec<BgLayer>, String> {
    let d = drawing(id)?;
    let mut layers: Vec<BgLayer> = d.layers.iter().map(|l| BgLayer { name: l.name.clone(), color: l.color.clone(), paths: vec![] }).collect();
    for p in &d.paths {
        let mut v = Vec::with_capacity(p.pts.len() * 2 + 2);
        for q in &p.pts {
            v.push((q.x * mm_per_unit).round() as f32);
            v.push((q.y * mm_per_unit).round() as f32);
        }
        if p.closed && !p.pts.is_empty() {
            v.push((p.pts[0].x * mm_per_unit).round() as f32);
            v.push((p.pts[0].y * mm_per_unit).round() as f32);
        }
        layers[p.layer as usize].paths.push(v);
    }
    Ok(layers.into_iter().filter(|l| !l.paths.is_empty()).collect())
}

pub fn analyze(id: &str, opts: &AnalyzeOptions) -> Result<AnalyzeResult, String> {
    let d = drawing(id)?;
    analyze::analyze(&d, opts)
}

// ------------------------------------------------------------------ trabajos

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum JobKind {
    Optimize,
    Explore,
    Capacity,
    Suggest,
}

pub fn start_job(kind: JobKind, problem: Problem, explore_n: Option<usize>) -> String {
    let id = format!("job{}", COUNTER.fetch_add(1, Ordering::Relaxed));
    let job = Arc::new(Job { cancel: AtomicBool::new(false), state: Mutex::new(JobState::default()) });
    store().lock().unwrap().jobs.insert(id.clone(), job.clone());
    std::thread::spawn(move || {
        let j2 = job.clone();
        let progress = move |p: Progress| {
            j2.state.lock().unwrap().progress = p;
        };
        let res: Result<Value, String> = match kind {
            JobKind::Optimize => search::optimize(&problem, &job.cancel, &progress).map(|l| serde_json::to_value(l).unwrap()),
            JobKind::Explore => search::explore(&problem, explore_n.unwrap_or(10), &job.cancel, &progress).map(|v| serde_json::to_value(v).unwrap()),
            JobKind::Capacity => search::capacity(&problem, &job.cancel, &progress).map(|v| serde_json::to_value(v).unwrap()),
            JobKind::Suggest => search::suggest(&problem, &job.cancel, &progress).map(|v| serde_json::to_value(v).unwrap()),
        };
        let mut st = job.state.lock().unwrap();
        st.done = true;
        match res {
            Ok(v) => st.result = Some(v),
            Err(e) => st.error = Some(e),
        }
    });
    id
}

pub fn job_status(id: &str) -> Result<JobState, String> {
    let job = store().lock().unwrap().jobs.get(id).cloned().ok_or("Trabajo desconocido")?;
    let st = job.state.lock().unwrap().clone();
    if st.done {
        // liberar memoria una vez entregado
        store().lock().unwrap().jobs.remove(id);
    }
    Ok(st)
}

pub fn cancel_job(id: &str) {
    if let Some(j) = store().lock().unwrap().jobs.get(id) {
        j.cancel.store(true, Ordering::Relaxed);
    }
}

/// Re-asigna roles y clientes sobre una distribución existente (sin mover escritorios).
pub fn reassign(problem: &Problem, layout: &Layout) -> Result<Layout, String> {
    let ctx = crate::engine::ctx::Ctx::build(problem)?;
    let mut l = layout.clone();
    for s in l.seats.iter_mut() {
        s.role_id.clear();
        s.client_id.clear();
        s.flags.retain(|f| !f.contains("equipo") && !f.contains("tipo de escritorio") && !f.contains("referencia"));
    }
    if crate::assign::has_people(&problem.config.org) {
        crate::assign::assign(&ctx, &problem.config, &mut l);
    }
    Ok(l)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReq {
    pub problem: Problem,
    pub layout: Layout,
    #[serde(default = "one")]
    pub mm_per_unit: f64,
    #[serde(default)]
    pub include_plan: bool,
}
fn one() -> f64 {
    1.0
}

pub fn export_dxf(r: &ExportReq) -> String {
    crate::export::to_dxf(&r.problem.plan, &r.layout, &r.problem.config, r.mm_per_unit, r.include_plan)
}

pub fn export_csv(r: &ExportReq) -> String {
    crate::export::to_csv(&r.layout, &r.problem.config)
}

/// Despacho genérico por nombre de comando con argumentos JSON (servidor de desarrollo y pruebas).
pub fn dispatch(cmd: &str, args: Value) -> Result<Value, String> {
    let get = |k: &str| args.get(k).cloned().unwrap_or(Value::Null);
    let parse = |k: &str| -> Result<Value, String> { Ok(get(k)) };
    let de = |v: Value| -> Result<Problem, String> { serde_json::from_value(v).map_err(|e| format!("Datos inválidos: {e}")) };
    match cmd {
        "defaults" => Ok(serde_json::to_value(defaults()).unwrap()),
        "background" => {
            let id = get("drawingId").as_str().unwrap_or("").to_string();
            let s = get("mmPerUnit").as_f64().unwrap_or(1.0);
            background(&id, s).map(|v| serde_json::to_value(v).unwrap())
        }
        "analyze" => {
            let id = get("drawingId").as_str().unwrap_or("").to_string();
            let opts: AnalyzeOptions = serde_json::from_value(parse("options")?).map_err(|e| e.to_string())?;
            analyze(&id, &opts).map(|v| serde_json::to_value(v).unwrap())
        }
        "start_job" => {
            let kind: JobKind = serde_json::from_value(get("kind")).map_err(|e| e.to_string())?;
            let p = de(get("problem"))?;
            let n = get("n").as_u64().map(|x| x as usize);
            Ok(Value::String(start_job(kind, p, n)))
        }
        "job_status" => job_status(get("jobId").as_str().unwrap_or("")).map(|v| serde_json::to_value(v).unwrap()),
        "cancel_job" => {
            cancel_job(get("jobId").as_str().unwrap_or(""));
            Ok(Value::Null)
        }
        "reassign" => {
            let p = de(get("problem"))?;
            let l: Layout = serde_json::from_value(get("layout")).map_err(|e| e.to_string())?;
            reassign(&p, &l).map(|v| serde_json::to_value(v).unwrap())
        }
        "export_dxf" | "export_csv" => {
            let r: ExportReq = serde_json::from_value(args).map_err(|e| e.to_string())?;
            Ok(Value::String(if cmd == "export_dxf" { export_dxf(&r) } else { export_csv(&r) }))
        }
        _ => Err(format!("Comando desconocido: {cmd}")),
    }
}
