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
    store().lock().unwrap().drawings.get(id).cloned().ok_or_else(|| "The drawing is no longer in memory; import it again.".into())
}

/// Borra del dibujo en memoria todas las líneas, curvas y textos de una capa.
pub fn delete_layer(id: &str, layer: &str) -> Result<usize, String> {
    let d = drawing(id)?;
    let Some(li) = d.layers.iter().position(|l| l.name == layer) else { return Err(format!("Layer “{layer}” does not exist.")) };
    let mut n = (*d).clone();
    let li = li as u32;
    let before = n.paths.len() + n.curves.len() + n.texts.len();
    n.paths.retain(|p| p.layer != li);
    n.curves.retain(|p| p.layer != li);
    n.texts.retain(|t| t.layer != li);
    n.layers[li as usize].paths = 0;
    n.finalize();
    let removed = before - (n.paths.len() + n.curves.len() + n.texts.len());
    store().lock().unwrap().drawings.insert(id.to_string(), Arc::new(n));
    Ok(removed)
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
    let run = move || {
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
    };
    spawn_job(run);
    id
}

/// Escritorio: un hilo propio por trabajo. Navegador con hilos (WebAssembly + atomics): el trabajo
/// corre en el grupo de hilos de rayon (Web Workers) y la interfaz sigue sondeando igual.
/// Navegador sin hilos: corre en el acto y queda terminado al volver.
fn spawn_job(run: impl FnOnce() + Send + 'static) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::thread::spawn(run);
    }
    #[cfg(all(target_arch = "wasm32", target_feature = "atomics"))]
    {
        rayon::spawn(run);
    }
    #[cfg(all(target_arch = "wasm32", not(target_feature = "atomics")))]
    {
        run();
    }
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
        if !s.locked {
            s.role_id.clear();
            s.client_id.clear();
        }
        s.flags.retain(|f| !f.contains("team") && !f.contains("desk type") && !f.contains("reference"));
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

/// Recalcula indicadores y avisos de una distribución editada a mano (sin mover nada).
pub fn evaluate(problem: &Problem, layout: &Layout) -> Result<Layout, String> {
    use crate::engine::{decode, search};
    let mut p = problem.clone();
    let was_pinned: HashMap<String, bool> = layout.modules.iter().map(|m| (m.id.clone(), m.pinned)).collect();

    // todos los módulos actuales pasan como fijados, con sus personas
    p.config.pinned = layout
        .modules
        .iter()
        .map(|m| {
            let mut m = m.clone();
            m.people = layout
                .seats
                .iter()
                .filter(|s| s.module_id == m.id)
                .map(|s| SeatPerson { pos: s.pos, role_id: s.role_id.clone(), client_id: s.client_id.clone() })
                .collect();
            m
        })
        .collect();
    let mut ctx = crate::engine::ctx::Ctx::build(&p)?;
    let requested_units = ctx.demand.clone();
    ctx.demand = vec![0; ctx.catalog.len()];
    let mut rng = decode::Rng(7);
    let g = decode::random_genome(&ctx, &mut rng);
    let mut l = search::to_layout(&ctx, &g, &problem.config.weights);
    // la demanda real (no la de la evaluación) para "not placed"
    let mut req_seats = 0u32;
    let mut placed = vec![0u32; ctx.catalog.len()];
    for m in &l.modules {
        if let Some(i) = ctx.catalog.iter().position(|t| t.id == m.type_id) {
            let t = &ctx.catalog[i];
            placed[i] += if t.flexible { t.seats_per_module(m.per_side) } else { 1 };
        }
    }
    l.kpis.unplaced_by_type.clear();
    for d in &problem.config.demand {
        if let Some(i) = ctx.catalog.iter().position(|t| t.id == d.type_id) {
            let t = &ctx.catalog[i];
            let per = if t.flexible { 1 } else { t.seats_per_module(t.per_side) };
            req_seats += d.quantity * per;
            let miss = d.quantity.saturating_sub(placed[i]);
            if miss > 0 {
                l.kpis.unplaced_by_type.push((t.id.clone(), miss));
            }
        }
    }
    let _ = requested_units;
    l.kpis.seats_requested = req_seats;
    l.kpis.seats_placed = l.seats.len() as u32;
    l.kpis.grouping_score = layout.kpis.grouping_score;
    l.kpis.misplaced_people = layout.kpis.misplaced_people;
    for m in l.modules.iter_mut() {
        m.pinned = *was_pinned.get(&m.id).unwrap_or(&false);
        if !m.pinned {
            m.people.clear();
        }
    }
    // conservar numeración, bloqueo y persona de cada puesto (se reconoce por posición)
    let mut next = layout.seats.iter().map(|s| s.number).max().unwrap_or(0);
    for s in l.seats.iter_mut() {
        let old = layout.seats.iter().filter(|o| o.pos.dist(s.pos) < 300.0).min_by(|a, b| a.pos.dist(s.pos).partial_cmp(&b.pos.dist(s.pos)).unwrap());
        match old {
            Some(o) => {
                s.number = o.number;
                s.id = o.id.clone();
                s.locked = o.locked;
                s.role_id = o.role_id.clone();
                s.client_id = o.client_id.clone();
            }
            None => {
                next += 1;
                s.number = next;
                s.id = format!("s{next}");
                s.locked = false;
            }
        }
    }
    l.seats.sort_by_key(|s| s.number);
    Ok(l)
}

/// Codifica bytes en base64 (para devolver PDF por JSON).
pub fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut o = String::with_capacity(data.len() * 4 / 3 + 4);
    for ch in data.chunks(3) {
        let b = [ch[0], *ch.get(1).unwrap_or(&0), *ch.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        o.push(T[(n >> 18) as usize & 63] as char);
        o.push(T[(n >> 12) as usize & 63] as char);
        o.push(if ch.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        o.push(if ch.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    o
}

/// Decodifica base64 estándar.
pub fn unb64(s: &str) -> Result<Vec<u8>, String> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let clean: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    for ch in clean.chunks(4) {
        let mut n = 0u32;
        let mut cnt: usize = 0;
        for &c in ch {
            if c == b'=' {
                break;
            }
            n = (n << 6) | val(c).ok_or("invalid base64")?;
            cnt += 1;
        }
        n <<= 6 * (4 - cnt);
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..cnt.saturating_sub(1).min(3)]);
    }
    Ok(out)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfReq {
    #[serde(default)]
    pub problem: Option<Problem>,
    #[serde(default)]
    pub layout: Option<Layout>,
    #[serde(default)]
    pub drawing_id: String,
    #[serde(default = "one")]
    pub mm_per_unit: f64,
    pub opts: crate::pdf_out::PdfOpts,
}

/// PDF del layout (o del plano limpio si `opts.clean`), en base64.
pub fn export_pdf(r: &PdfReq) -> Result<String, String> {
    let d = if r.drawing_id.is_empty() { None } else { drawing(&r.drawing_id).ok() };
    let bg = d.as_deref().map(|d| (d, r.mm_per_unit));
    let bytes = crate::pdf_out::render_pdf(r.problem.as_ref().map(|p| &p.plan), r.layout.as_ref(), r.problem.as_ref().map(|p| &p.config), bg, &r.opts)?;
    Ok(b64(&bytes))
}

/// Capas de mobiliario según los roles asignados.
pub fn furniture_layers(id: &str, roles: &HashMap<String, LayerRole>) -> Result<Vec<String>, String> {
    let d = drawing(id)?;
    Ok(d.layers.iter().filter(|l| roles.get(&l.name) == Some(&LayerRole::Furniture)).map(|l| l.name.clone()).collect())
}

/// DXF del plano original sin las capas excluidas.
pub fn export_clean_dxf(id: &str, mm_per_unit: f64, exclude: &[String]) -> Result<String, String> {
    let d = drawing(id)?;
    // PDF → milímetros; DXF/DWG → unidades originales
    let out_mm = if d.source == SourceKind::Pdf { 1.0 } else { mm_per_unit };
    Ok(crate::pdf_out::clean_dxf(&d, mm_per_unit, out_mm, exclude))
}

/// Plano de líneas simples (muros a eje, columnas, puertas, oficinas) en DXF (texto, mm) o PDF (base64).
pub fn export_simple(id: &str, opts: &AnalyzeOptions, plan: &Plan, format: &str, paper: &str, title: &str) -> Result<String, String> {
    let d = drawing(id)?;
    let sp = crate::simple::simple_plan(&d, opts, plan, &crate::simple::SimpleOpts::default());
    if format == "dxf" {
        return Ok(crate::simple::simple_dxf(&sp, 1.0));
    }
    let dd = crate::simple::simple_drawing(&sp, title);
    let o = crate::pdf_out::PdfOpts { paper: paper.into(), scale: None, title: title.into(), color_by: "type".into(), background: true, hidden_layers: vec![], clean: true, numbers: false };
    crate::pdf_out::render_pdf(None, None, None, Some((&dd, 1.0)), &o).map(|b| b64(&b))
}

/// Despacho genérico por nombre de comando con argumentos JSON (servidor de desarrollo y pruebas).
pub fn dispatch(cmd: &str, args: Value) -> Result<Value, String> {
    let get = |k: &str| args.get(k).cloned().unwrap_or(Value::Null);
    let parse = |k: &str| -> Result<Value, String> { Ok(get(k)) };
    let de = |v: Value| -> Result<Problem, String> { serde_json::from_value(v).map_err(|e| format!("Invalid data: {e}")) };
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
        "circulation" => {
            let p: Problem = de(get("problem"))?;
            crate::engine::circulation::compute(&p).map(|v| serde_json::to_value(v).unwrap())
        }
        "evaluate" => {
            let p = de(get("problem"))?;
            let l: Layout = serde_json::from_value(get("layout")).map_err(|e| e.to_string())?;
            evaluate(&p, &l).map(|v| serde_json::to_value(v).unwrap())
        }
        "export_pdf" => {
            let r: PdfReq = serde_json::from_value(args).map_err(|e| format!("Invalid data: {e}"))?;
            export_pdf(&r).map(Value::String)
        }
        "export_plan_dxf" => {
            let plan: Plan = serde_json::from_value(get("plan")).map_err(|e| format!("Invalid data: {e}"))?;
            let out_mm = get("outMm").as_f64().unwrap_or(1.0);
            Ok(Value::String(crate::export::plan_dxf(&plan, out_mm)))
        }
        "export_clean_dxf" => {
            let id = get("drawingId").as_str().unwrap_or("").to_string();
            let mm = get("mmPerUnit").as_f64().unwrap_or(1.0);
            let ex: Vec<String> = serde_json::from_value(get("exclude")).unwrap_or_default();
            export_clean_dxf(&id, mm, &ex).map(Value::String)
        }
        "delete_layer" => {
            let id = get("drawingId").as_str().unwrap_or("").to_string();
            let layer = get("layer").as_str().unwrap_or("").to_string();
            delete_layer(&id, &layer).map(|n| Value::from(n as u64))
        }
        "export_simple" => {
            let id = get("drawingId").as_str().unwrap_or("").to_string();
            let opts: AnalyzeOptions = serde_json::from_value(get("options")).map_err(|e| e.to_string())?;
            let plan: Plan = serde_json::from_value(get("plan")).map_err(|e| format!("Invalid data: {e}"))?;
            let fmt = get("format").as_str().unwrap_or("dxf").to_string();
            let paper = get("paper").as_str().unwrap_or("A3").to_string();
            let title = get("title").as_str().unwrap_or("Plan").to_string();
            export_simple(&id, &opts, &plan, &fmt, &paper, &title).map(Value::String)
        }
        "export_dxf" | "export_csv" => {
            let r: ExportReq = serde_json::from_value(args).map_err(|e| e.to_string())?;
            Ok(Value::String(if cmd == "export_dxf" { export_dxf(&r) } else { export_csv(&r) }))
        }
        _ => Err(format!("Comando desconocido: {cmd}")),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_roundtrip() {
        for n in 0..20 {
            let v: Vec<u8> = (0..n).map(|i| (i * 37 + 11) as u8).collect();
            assert_eq!(super::unb64(&super::b64(&v)).unwrap(), v);
        }
    }
}
