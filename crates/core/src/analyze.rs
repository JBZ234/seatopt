//! Del dibujo importado al plano normalizado:
//!  - sugerencia de rol para cada capa,
//!  - estimación de escala (radios de puertas ≈ 900 mm),
//!  - recinto: polígonos explícitos o inundación desde un punto con cierre de vanos,
//!  - columnas / oficinas (huecos), puertas (arcos de giro) y ventanas.

use crate::geom::*;
use crate::import::Drawing;
use crate::model::*;
use crate::raster::{self, Frame};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum LayerRole {
    /// Contorno cerrado del recinto (si existe tal cual).
    Outline,
    /// Muros, fachadas, vidrios, escaleras: bloquean.
    Wall,
    Column,
    /// Polígonos cerrados de oficinas/salas fijas.
    Office,
    Restricted,
    Reserved,
    /// Puertas genéricas (arcos de giro).
    Door,
    MainDoor,
    ExitDoor,
    OfficeDoor,
    Window,
    /// Mobiliario existente: se muestra pero no bloquea.
    Furniture,
    /// Recintos cerrados de referencia (oficinas, baños) fuera del área amoblable.
    Room,
    /// Contorno exterior del edificio (referencia).
    Building,
    Ignore,
}

impl LayerRole {
    pub fn blocks(self) -> bool {
        matches!(self, LayerRole::Outline | LayerRole::Wall | LayerRole::Column | LayerRole::Office | LayerRole::Restricted | LayerRole::Window | LayerRole::Room | LayerRole::Building)
    }
    pub fn is_door(self) -> bool {
        matches!(self, LayerRole::Door | LayerRole::MainDoor | LayerRole::ExitDoor | LayerRole::OfficeDoor)
    }
}

/// Sugerencia de rol según el nombre de la capa (español/inglés, convenciones AIA y comunes).
pub fn suggest_role(name: &str) -> LayerRole {
    let n = name.to_uppercase();
    let has = |k: &[&str]| k.iter().any(|w| n.contains(w));
    if has(&["PUERTA-PPAL", "PUERTA_PPAL", "PUERTA PRINCIPAL", "MAIN DOOR", "MAIN-DOOR", "ENTRADA"]) {
        return LayerRole::MainDoor;
    }
    if has(&["SALIDA", "EXIT", "EMERG"]) {
        return LayerRole::ExitDoor;
    }
    if has(&["PUERTA-OFICINA", "PUERTA_OFICINA"]) {
        return LayerRole::OfficeDoor;
    }
    // vano resaltado de las puertas exportadas por la app: solo dibujo
    if has(&["PUERTA-VANO", "DOOR-OPENING"]) {
        return LayerRole::Ignore;
    }
    if has(&["A-RECINTO", "RECINTOS"]) {
        return LayerRole::Room;
    }
    if has(&["A-EDIFICIO", "EDIFICIO"]) {
        return LayerRole::Building;
    }
    if has(&["MURO-EXT", "MURO_EXT", "CONTORNO", "OUTLINE", "BOUNDARY", "PERIMETRO", "PERÍMETRO"]) {
        return LayerRole::Outline;
    }
    if has(&["COLUM", "PILAR", "COLUMN", "S-COLS", "S-COL", "COL"]) && !has(&["COLOR"]) {
        return LayerRole::Column;
    }
    if has(&["OFICINA-FIJA", "OFICINA_FIJA", "ROOM", "NUCLEO", "NÚCLEO", "CORE"]) {
        return LayerRole::Office;
    }
    if has(&["RESTRING", "TABLERO", "ELEC", "GABINETE", "EXTINTOR", "FIRE"]) {
        return LayerRole::Restricted;
    }
    if has(&["RESERV"]) {
        return LayerRole::Reserved;
    }
    if has(&["DOOR", "PUERTA", "DJAMB", "-DOOR"]) {
        return LayerRole::Door;
    }
    if has(&["WINDOW", "VENTAN", "WIN", "GLAZ", "FACHADA", "CURTAIN"]) {
        return LayerRole::Window;
    }
    if has(&["MOBIL", "MUEBLE", "FURN", "NOITHAT", "SILLA", "CHAIR", "DESK", "ESCRITORIO", "EQUIP", "I-FURN"]) {
        return LayerRole::Furniture;
    }
    if has(&["TEXT", "TEXTO", "COTA", "DIM", "ANNO", "MARQUILLA", "HATCH", "ACHUR", "PISO", "PROY", "SOTANO", "DEFPOINTS", "TITLE", "ROTULO", "BORDE", "LNBORDES", "VIEWPORT", "TUBO", "PDF_GEOMETRY"]) {
        return LayerRole::Ignore;
    }
    if has(&["WALL", "MURO", "VIDRIO", "GLASS", "STAIR", "ESCALER", "ASCENS", "ELEV", "FIXT", "SANIT", "PARTIC", "DIVIS"]) {
        return LayerRole::Wall;
    }
    LayerRole::Ignore
}

pub fn suggest_roles(d: &Drawing) -> HashMap<String, LayerRole> {
    let mut roles: HashMap<String, LayerRole> = d.layers.iter().map(|l| (l.name.clone(), suggest_role(&l.name))).collect();
    // Dibujos sin capas útiles (p. ej. PDF impresos sin capas): lo que no sea texto ni mobiliario bloquea.
    let has_walls = d.layers.iter().any(|l| l.paths > 0 && matches!(roles[&l.name], LayerRole::Wall | LayerRole::Outline));
    if !has_walls {
        for l in &d.layers {
            if l.paths > 0 && roles[&l.name] == LayerRole::Ignore {
                let n = l.name.to_uppercase();
                let texty = ["TEXT", "COTA", "DIM", "ANNO", "MARQUILLA", "HATCH", "ACHUR", "TITLE", "ROTULO"].iter().any(|k| n.contains(k));
                if !texty {
                    roles.insert(l.name.clone(), LayerRole::Wall);
                }
            }
        }
    }
    roles
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScaleGuess {
    /// mm por unidad del dibujo.
    pub mm_per_unit: f64,
    /// Escala de papel 1:N si aplica (PDF).
    pub paper_scale: Option<f64>,
    pub reason: String,
}

/// Arco de puerta detectado en unidades del dibujo.
#[derive(Clone, Debug)]
struct ArcFit {
    c: Pt,
    r: f64,
    e0: Pt,
    e1: Pt,
    sweep: f64,
}

fn fit_arc(pts: &[Pt]) -> Option<ArcFit> {
    if pts.len() < 3 {
        return None;
    }
    let a = pts[0];
    let b = pts[pts.len() / 2];
    let c = *pts.last().unwrap();
    // circunferencia por tres puntos
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-12 {
        return None;
    }
    let a2 = a.dot(a);
    let b2 = b.dot(b);
    let c2 = c.dot(c);
    let ux = (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d;
    let uy = (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d;
    let ctr = pt(ux, uy);
    let r = ctr.dist(a);
    if r <= 0.0 || !r.is_finite() {
        return None;
    }
    for p in pts {
        if (p.dist(ctr) - r).abs() > r * 0.04 {
            return None;
        }
    }
    let v0 = (a - ctr).norm();
    let v1 = (c - ctr).norm();
    let sweep = v0.dot(v1).clamp(-1.0, 1.0).acos().to_degrees();
    Some(ArcFit { c: ctr, r, e0: a, e1: c, sweep })
}

fn door_arcs(d: &Drawing, roles: &HashMap<String, LayerRole>, strict: bool) -> Vec<(ArcFit, LayerRole)> {
    let any_door_layer = strict && d.layers.iter().any(|l| roles.get(&l.name).map(|r| r.is_door()).unwrap_or(false));
    // 1) ajustar cada tramo curvo por separado
    let mut pieces: Vec<(ArcFit, LayerRole, u32)> = Vec::new();
    // arcos como curvas, o aplanados en polilíneas abiertas (PDF impresos, exportaciones)
    let flat = d.curves.is_empty();
    let poly_arcs = d.paths.iter().filter(move |p| flat && !p.closed && p.pts.len() >= 6 && p.pts.len() <= 64);
    for c in d.curves.iter().chain(poly_arcs) {
        let lname = &d.layers[c.layer as usize].name;
        let role = roles.get(lname).copied().unwrap_or(LayerRole::Ignore);
        if any_door_layer && !role.is_door() {
            continue;
        }
        if !any_door_layer && strict && matches!(role, LayerRole::Furniture | LayerRole::Ignore | LayerRole::Column) {
            continue;
        }
        if !strict && role == LayerRole::Column {
            continue;
        }
        if let Some(a) = fit_arc(&c.pts) {
            if (15.0..=125.0).contains(&a.sweep) {
                pieces.push((a, role, c.layer));
            }
        }
    }
    // 2) unir tramos contiguos del mismo círculo (los PDF de AutoCAD parten el arco en 45°)
    let mut used = vec![false; pieces.len()];
    let mut out = Vec::new();
    for i in 0..pieces.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let (mut a, role, layer) = (pieces[i].0.clone(), pieces[i].1, pieces[i].2);
        let mut grew = true;
        while grew {
            grew = false;
            for j in 0..pieces.len() {
                if used[j] || pieces[j].2 != layer {
                    continue;
                }
                let b = &pieces[j].0;
                let tol = a.r * 0.04;
                if a.c.dist(b.c) > tol * 2.0 || (a.r - b.r).abs() > tol {
                    continue;
                }
                let joined = if a.e1.dist(b.e0) < tol {
                    Some((a.e0, b.e1))
                } else if a.e1.dist(b.e1) < tol {
                    Some((a.e0, b.e0))
                } else if a.e0.dist(b.e1) < tol {
                    Some((b.e0, a.e1))
                } else if a.e0.dist(b.e0) < tol {
                    Some((b.e1, a.e1))
                } else {
                    None
                };
                if let Some((e0, e1)) = joined {
                    a.sweep += b.sweep;
                    a.e0 = e0;
                    a.e1 = e1;
                    used[j] = true;
                    grew = true;
                }
            }
        }
        if (65.0..=125.0).contains(&a.sweep) {
            out.push((a, role));
        }
    }
    out
}

pub fn guess_scale(d: &Drawing, roles: &HashMap<String, LayerRole>) -> ScaleGuess {
    let arcs = door_arcs(d, roles, true);
    let mut radii: Vec<f64> = arcs.iter().map(|(a, _)| a.r).collect();
    radii.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let med = if radii.is_empty() { None } else { Some(radii[radii.len() / 2]) };
    match d.source {
        crate::import::SourceKind::Pdf if d.declared_unit_mm.is_some() => {
            let mm = d.declared_unit_mm.unwrap();
            ScaleGuess { mm_per_unit: mm, paper_scale: Some((mm / (25.4 / 72.0)).round()), reason: d.unit_note.clone() }
        }
        crate::import::SourceKind::Pdf => {
            let pt_mm = 25.4 / 72.0;
            let scales = [20.0, 25.0, 50.0, 75.0, 100.0, 125.0, 150.0, 200.0, 250.0, 300.0, 400.0, 500.0];
            if let Some(r) = med {
                let raw = 820.0 / r; // mm por punto (puerta típica 0,8–0,9 m)
                let n_raw = raw / pt_mm;
                let best = scales.iter().copied().min_by(|a, b| (a / n_raw).ln().abs().partial_cmp(&(b / n_raw).ln().abs()).unwrap()).unwrap();
                let (mm, ps, why) = if (best / n_raw).ln().abs() < 0.2 {
                    (best * pt_mm, Some(best), format!("Escala estimada 1:{best} a partir de {} puertas (puertas típicas de 0,8–0,9 m).", radii.len()))
                } else {
                    (raw, Some(n_raw.round()), format!("Escala aproximada 1:{:.0} (no estándar) a partir de {} puertas.", n_raw, radii.len()))
                };
                ScaleGuess { mm_per_unit: mm, paper_scale: ps, reason: why + " Verifica con la herramienta de calibración." }
            } else {
                ScaleGuess { mm_per_unit: 100.0 * pt_mm, paper_scale: Some(100.0), reason: "No se encontraron puertas: se asume 1:100. Calibra con una medida conocida.".into() }
            }
        }
        _ => {
            if let Some(u) = d.declared_unit_mm {
                return ScaleGuess { mm_per_unit: u, paper_scale: None, reason: d.unit_note.clone() };
            }
            let cands = [1.0, 10.0, 1000.0, 25.4, 304.8];
            if let Some(r) = med {
                let best = cands.iter().copied().min_by(|a, b| ((a * r) / 900.0).ln().abs().partial_cmp(&((b * r) / 900.0).ln().abs()).unwrap()).unwrap();
                ScaleGuess { mm_per_unit: best, paper_scale: None, reason: format!("Unidades estimadas por el tamaño de {} puertas.", radii.len()) }
            } else {
                // por tamaño del dibujo: una oficina mide 5–200 m
                let span = d.bounds.w().max(d.bounds.h());
                let best = cands.iter().copied().min_by(|a, b| ((a * span) / 30000.0).ln().abs().partial_cmp(&((b * span) / 30000.0).ln().abs()).unwrap()).unwrap();
                ScaleGuess { mm_per_unit: best, paper_scale: None, reason: "Unidades estimadas por el tamaño total del dibujo.".into() }
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeOptions {
    pub mm_per_unit: f64,
    pub roles: HashMap<String, LayerRole>,
    /// Punto (mm) dentro del área a amoblar. None = automático (el recinto interior más grande).
    #[serde(default)]
    pub seed: Option<Pt>,
    /// Vanos de hasta este ancho se cierran al detectar el recinto (mm).
    #[serde(default = "default_gap")]
    pub gap_close: f64,
}

fn default_gap() -> f64 {
    1200.0
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeResult {
    pub plan: Plan,
    pub notes: Vec<String>,
    pub area_m2: f64,
}

pub(crate) struct Geo {
    pub(crate) paths: Vec<(LayerRole, Vec<Pt>, bool)>,
}

pub(crate) fn to_mm(d: &Drawing, opts: &AnalyzeOptions) -> Geo {
    let s = opts.mm_per_unit;
    let mut paths = Vec::new();
    for p in &d.paths {
        let role = opts.roles.get(&d.layers[p.layer as usize].name).copied().unwrap_or(LayerRole::Ignore);
        if role == LayerRole::Ignore || role == LayerRole::Furniture {
            continue;
        }
        paths.push((role, p.pts.iter().map(|q| *q * s).collect(), p.closed));
    }
    remove_sheet_frame(&mut paths);
    Geo { paths }
}

/// Quita el marco de la hoja (rectángulos o líneas sobre el borde del dibujo) que traen
/// los PDF impresos: si no, el «interior del marco» parece un recinto cerrado.
fn remove_sheet_frame(paths: &mut Vec<(LayerRole, Vec<Pt>, bool)>) {
    for _ in 0..3 {
        let bb = BBox::of(paths.iter().filter(|(r, _, _)| r.blocks() || r.is_door()).flat_map(|(_, p, _)| p.iter().copied()));
        if !bb.is_valid() || bb.w() <= 0.0 || bb.h() <= 0.0 {
            return;
        }
        let tol = bb.w().max(bb.h()) * 0.01;
        let on_edge = |a: Pt, b: Pt| -> bool {
            let horiz = (a.y - b.y).abs() < tol && ((a.y - bb.min.y).abs() < tol || (a.y - bb.max.y).abs() < tol) && (a.x - b.x).abs() > bb.w() * 0.5;
            let vert = (a.x - b.x).abs() < tol && ((a.x - bb.min.x).abs() < tol || (a.x - bb.max.x).abs() < tol) && (a.y - b.y).abs() > bb.h() * 0.5;
            horiz || vert
        };
        let before = paths.len();
        let is_frame = |r: &LayerRole, p: &Vec<Pt>| -> bool {
            if *r == LayerRole::Outline {
                return false;
            }
            let pb = BBox::of(p.iter().copied());
            let spans = pb.w() > bb.w() * 0.9 && pb.h() > bb.h() * 0.9;
            let rect_like = spans && p.len() <= 6 && p.windows(2).all(|w| (w[0].x - w[1].x).abs() < tol || (w[0].y - w[1].y).abs() < tol);
            rect_like || (p.len() == 2 && on_edge(p[0], p[1]))
        };
        // solo es marco si casi nada lo toca (a un muro siempre llegan puertas, tabiques o ventanas)
        let span = bb.w().max(bb.h());
        let touch_tol = span * 0.003;
        let cand: Vec<usize> = (0..paths.len()).filter(|&i| is_frame(&paths[i].0, &paths[i].1)).collect();
        if cand.is_empty() {
            return;
        }
        let mut drop = vec![false; paths.len()];
        for &i in &cand {
            let fp = &paths[i].1;
            let segs: Vec<(Pt, Pt)> = fp.windows(2).map(|w| (w[0], w[1])).chain(std::iter::once((*fp.last().unwrap(), fp[0]))).collect();
            let mut touches = 0usize;
            'outer: for (j, (_, p, _)) in paths.iter().enumerate() {
                if j == i || cand.contains(&j) {
                    continue;
                }
                for &q in p {
                    if segs.iter().any(|&(a, b)| dist_point_segment(q, a, b) < touch_tol) {
                        touches += 1;
                        if touches > 3 {
                            break 'outer;
                        }
                    }
                }
            }
            if touches <= 3 {
                drop[i] = true;
            }
        }
        let mut k = 0;
        paths.retain(|_| {
            k += 1;
            !drop[k - 1]
        });

        if paths.len() == before {
            return;
        }
    }
}

fn closed_like(pts: &[Pt], closed: bool) -> bool {
    closed || (pts.len() >= 4 && pts[0].dist(*pts.last().unwrap()) < 1.0)
}

pub fn analyze(d: &Drawing, opts: &AnalyzeOptions) -> Result<AnalyzeResult, String> {
    let g = to_mm(d, opts);
    let mut notes = Vec::new();
    if g.paths.is_empty() {
        return Err("Ninguna capa está marcada como muro o contorno. En «Capas del dibujo» asigna «Muro / bloquea» a las capas de muros, o marca el área a mano con «Dibujar contorno».".into());
    }
    let mut plan = Plan::default();
    let mut oid = 0;
    let mut next_id = |p: &str| {
        oid += 1;
        format!("{p}{oid}")
    };

    // ---------------------------------------------------------------- recinto
    let outlines: Vec<Vec<Pt>> = g
        .paths
        .iter()
        .filter(|(r, p, c)| *r == LayerRole::Outline && closed_like(p, *c) && p.len() >= 3)
        .map(|(_, p, _)| p.clone())
        .collect();
    // quitar contornos repetidos (p. ej. relleno + trazo en un PDF): con la regla par-impar se anularían
    let mut outlines_u: Vec<Vec<Pt>> = Vec::new();
    for o in outlines {
        let (a, c0) = (polygon_area(&o).abs(), centroid(&o));
        if !outlines_u.iter().any(|u| (polygon_area(u).abs() - a).abs() < a * 0.01 + 1.0 && centroid(u).dist(c0) < 50.0) {
            outlines_u.push(o);
        }
    }
    let outlines = outlines_u;

    let mut bb = BBox::empty();
    for (r, p, _) in &g.paths {
        if r.blocks() || r.is_door() {
            for &q in p {
                bb.add(q);
            }
        }
    }
    if !bb.is_valid() {
        return Err("Las capas marcadas como muro no tienen trazos. Revisa los roles de capa o usa «Dibujar contorno».".into());
    }
    let area_bb = bb.w() * bb.h();
    let res = if area_bb > 9.0e9 { 100.0 } else if area_bb > 2.5e9 { 75.0 } else { 50.0 };
    let f = Frame::covering(bb, res, 2000.0);

    // máscara de bloqueos crudos
    let mut blocked = vec![false; f.len()];
    for (r, p, c) in &g.paths {
        if !r.blocks() {
            continue;
        }
        let n = p.len();
        let segs = if closed_like(p, *c) { n } else { n - 1 };
        for i in 0..segs {
            raster::stroke_segment(&f, &mut blocked, p[i], p[(i + 1) % n], res * 0.75, true);
        }
        if *r == LayerRole::Column && closed_like(p, *c) {
            raster::fill_polygon(&f, &mut blocked, p, true);
        }
    }

    let region_mask: Vec<bool>;
    if !outlines.is_empty() {
        let mut m = vec![false; f.len()];
        raster::fill_rings(&f, &mut m, &outlines, true);
        region_mask = m;
        let biggest = outlines.iter().max_by(|a, b| polygon_area(a).abs().partial_cmp(&polygon_area(b).abs()).unwrap()).unwrap();
        plan.region = vec![biggest.clone()];
        plan.building = biggest.clone();
        notes.push("Recinto tomado de la capa de contorno.".into());
        // recintos y edificio de referencia, si vienen en sus capas
        for (r, p, c) in &g.paths {
            if !closed_like(p, *c) || p.len() < 3 {
                continue;
            }
            match r {
                LayerRole::Room => plan.rooms.push(Room { poly: p.clone(), label: String::new() }),
                LayerRole::Building if polygon_area(p).abs() > polygon_area(&plan.building).abs() => plan.building = p.clone(),
                _ => {}
            }
        }
    } else {
        // inundación con cierre de vanos
        let r = opts.gap_close * 0.5;
        let closed_blk = raster::dilate(&f, &blocked, r);
        let free_closed: Vec<bool> = closed_blk.iter().map(|b| !b).collect();
        let (lab, sizes) = raster::components(&f, &free_closed);
        let mut touches_border = vec![false; sizes.len()];
        for x in 0..f.w {
            touches_border[lab[f.idx(x, 0)] as usize] = true;
            touches_border[lab[f.idx(x, f.h - 1)] as usize] = true;
        }
        for y in 0..f.h {
            touches_border[lab[f.idx(0, y)] as usize] = true;
            touches_border[lab[f.idx(f.w - 1, y)] as usize] = true;
        }
        let seed_label = match opts.seed.and_then(|s| f.cell_idx(s)) {
            Some(i) if lab[i] != 0 => lab[i],
            Some(_) => {
                return Err("El punto elegido cae sobre un muro o en un espacio menor al ancho de cierre de vanos. Haz clic más adentro del área.".into());
            }
            None => {
                let best = (1..sizes.len()).filter(|&i| !touches_border[i]).max_by_key(|&i| sizes[i]);
                match best {
                    Some(b) => b as u32,
                    None => return Err("No se encontró un recinto cerrado. Revisa los roles de capa o dibuja el contorno.".into()),
                }
            }
        };
        let core: Vec<bool> = lab.iter().map(|&l| l == seed_label).collect();
        // recuperar el borde perdido por la dilatación, sin cruzar muros
        let grown = raster::dilate(&f, &core, r + res);
        let cand: Vec<bool> = grown.iter().zip(&blocked).map(|(g, b)| *g && !*b).collect();
        let seed_idx = core.iter().position(|&b| b).unwrap();
        let mut m = raster::flood(&f, &cand, &[seed_idx]);
        // limpieza: quitar islas delgadas (< 300 mm)
        let er = raster::erode(&f, &m, 150.0);
        let op = raster::dilate(&f, &er, 150.0);
        for i in 0..m.len() {
            m[i] = m[i] && op[i];
        }
        region_mask = m;
        // ---- recintos cerrados (oficinas, baños, núcleos) y contorno del edificio, como referencia
        {
            let steps = ((r + res) / res).ceil() as usize;
            let mut room_lab = vec![0u32; f.len()];
            let mut q = std::collections::VecDeque::new();
            let min_cells = (0.5e6 / (res * res)) as usize;
            for i in 0..f.len() {
                let l = lab[i];
                if l != 0 && l != seed_label && !touches_border[l as usize] && sizes[l as usize] >= min_cells {
                    room_lab[i] = l;
                    q.push_back((i, 0usize));
                }
            }
            // crecer cada recinto hasta los muros (sin invadir el área amoblable)
            while let Some((i, d)) = q.pop_front() {
                if d >= steps {
                    continue;
                }
                let (x, y) = ((i % f.w) as i64, (i / f.w) as i64);
                for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= f.w as i64 || ny >= f.h as i64 {
                        continue;
                    }
                    let j = ny as usize * f.w + nx as usize;
                    if room_lab[j] == 0 && !blocked[j] && !region_mask[j] {
                        room_lab[j] = room_lab[i];
                        q.push_back((j, d + 1));
                    }
                }
            }
            let mut ids: Vec<u32> = room_lab.iter().copied().filter(|&l| l != 0).collect();
            ids.sort();
            ids.dedup();
            for id in ids {
                let m: Vec<bool> = room_lab.iter().map(|&l| l == id).collect();
                let area = m.iter().filter(|&&b| b).count() as f64 * res * res / 1e6;
                if area < 1.5 {
                    continue;
                }
                if let Some(ring) = raster::contours(&f, &m).into_iter().max_by(|a, b| polygon_area(a).abs().partial_cmp(&polygon_area(b).abs()).unwrap()) {
                    let ring = simplify_ring(&ring, res * 1.2);
                    if ring.len() >= 3 {
                        plan.rooms.push(Room { poly: ring, label: String::new() });
                    }
                }
            }
            // edificio = todo lo que no es exterior
            let exterior: Vec<bool> = lab.iter().map(|&l| l != 0 && touches_border[l as usize]).collect();
            let inside_b: Vec<bool> = exterior.iter().map(|e| !e).collect();
            let inside_b = raster::erode(&f, &inside_b, r * 0.9);
            if let Some(ring) = raster::contours(&f, &inside_b).into_iter().max_by(|a, b| polygon_area(a).abs().partial_cmp(&polygon_area(b).abs()).unwrap()) {
                plan.building = simplify_ring(&ring, res * 1.5);
            }
        }
        let rings = raster::contours(&f, &region_mask);
        // anillo exterior = el de mayor área absoluta
        let mut rings: Vec<Vec<Pt>> = rings.into_iter().map(|r| simplify_ring(&r, res * 1.2)).filter(|r| r.len() >= 3).collect();
        rings.sort_by(|a, b| polygon_area(b).abs().partial_cmp(&polygon_area(a).abs()).unwrap());
        if rings.is_empty() {
            return Err("No se pudo trazar el contorno del recinto.".into());
        }
        let outer = rings.remove(0);
        plan.region = vec![outer];
        // huecos → columnas u oficinas fijas
        for h in rings {
            let a = polygon_area(&h).abs();
            if a < 0.2e6 {
                continue;
            }
            let kind = if a < 3.0e6 { ObstacleKind::Column } else { ObstacleKind::FixedOffice };
            let id = next_id(if kind == ObstacleKind::Column { "col" } else { "of" });
            plan.obstacles.push(Obstacle { id, kind, poly: h, label: String::new() });
        }
        notes.push(format!("Recinto detectado por inundación (vanos ≤ {:.0} mm cerrados).", opts.gap_close));
    }

    // ---------------------------------------------------------------- obstáculos explícitos por capa
    let inside = |p: Pt| -> bool { f.cell_idx(p).map(|i| region_mask[i]).unwrap_or(false) };
    for (r, p, c) in &g.paths {
        let kind = match r {
            LayerRole::Column => ObstacleKind::Column,
            LayerRole::Office => ObstacleKind::FixedOffice,
            LayerRole::Restricted => ObstacleKind::Restricted,
            LayerRole::Reserved => ObstacleKind::Reserved,
            _ => continue,
        };
        if !closed_like(p, *c) || p.len() < 3 {
            continue;
        }
        let a = polygon_area(p).abs();
        if a < 0.01e6 {
            continue;
        }
        let ctr = centroid(p);
        // ¿toca el recinto?
        let touches = p.iter().any(|&q| inside(q)) || inside(ctr);
        if !touches {
            continue;
        }
        // evitar duplicar huecos ya detectados
        if plan.obstacles.iter().any(|o| point_in_polygon(ctr, &o.poly) && (polygon_area(&o.poly).abs() - a).abs() < a * 0.5) {
            continue;
        }
        let id = next_id(match kind {
            ObstacleKind::Column => "col",
            ObstacleKind::FixedOffice => "of",
            ObstacleKind::Restricted => "rz",
            ObstacleKind::Reserved => "res",
        });
        plan.obstacles.push(Obstacle { id, kind, poly: p.clone(), label: String::new() });
    }

    for t in &d.texts {
        let p = t.pos * opts.mm_per_unit;
        for rm in plan.rooms.iter_mut() {
            if rm.label.is_empty() && point_in_polygon(p, &rm.poly) && !t.text.trim().is_empty() {
                rm.label = t.text.trim().chars().take(40).collect();
            }
        }
    }
    // rótulos de textos dentro de obstáculos
    for t in &d.texts {
        let p = t.pos * opts.mm_per_unit;
        for o in plan.obstacles.iter_mut() {
            if o.label.is_empty() && point_in_polygon(p, &o.poly) && !t.text.trim().is_empty() {
                o.label = t.text.trim().chars().take(40).collect();
            }
        }
    }

    // ---------------------------------------------------------------- puertas
    let region_dist = raster::edt(&f, &region_mask);
    let in_or_near = |p: Pt, tol: f64| -> bool {
        match f.cell_idx(p) {
            Some(i) => region_mask[i] || (region_dist[i] as f64) <= tol,
            None => false,
        }
    };
    let arcs = door_arcs(d, &opts.roles, false);
    // hojas dibujadas (segmentos rectos en capas de puerta): marcan qué extremo del arco NO es el vano
    let leaf_segs: Vec<(Pt, Pt)> = g.paths.iter().filter(|(r, p, _)| r.is_door() && p.len() == 2).map(|(_, p, _)| (p[0], p[1])).collect();
    let mut hinges: Vec<Pt> = Vec::new();
    let mut doors: Vec<(Pt, Pt, LayerRole)> = Vec::new();
    for (a, role) in arcs {
        let c = a.c * opts.mm_per_unit;
        let e0 = a.e0 * opts.mm_per_unit;
        let e1 = a.e1 * opts.mm_per_unit;
        let r = a.r * opts.mm_per_unit;
        let from_door_layer = role.is_door();
        let r_ok = if from_door_layer { (450.0..=1700.0).contains(&r) } else { (650.0..=1300.0).contains(&r) };
        if !r_ok {
            continue;
        }
        // el extremo cerrado de la hoja queda contra el muro/jamba: probar extensión
        let score = |e: Pt| -> usize {
            let dir = (e - c).norm();
            (1..=6).filter(|k| {
                let q = e + dir * (*k as f64 * 60.0);
                f.cell_idx(q).map(|i| blocked[i]).unwrap_or(false)
            }).count()
        };
        // el vano cruza el borde del recinto (área amoblable de un solo lado); la hoja no
        let crossing = |e: Pt| -> usize {
            let nrm = (e - c).norm().perp();
            [0.3, 0.5, 0.7].iter().filter(|&&t| {
                let m = c.lerp(e, t);
                let off = (res * 2.0).max(250.0);
                inside(m + nrm * off) != inside(m - nrm * off)
            }).count()
        };
        let (b0, b1) = (crossing(e0), crossing(e1));
        let tol = r * 0.06 + 20.0;
        let is_leaf = |e: Pt| leaf_segs.iter().any(|&(p, q)| (p.dist(c) < tol && q.dist(e) < tol) || (q.dist(c) < tol && p.dist(e) < tol));
        let (l0, l1) = (is_leaf(e0), is_leaf(e1));
        let far = if l0 != l1 {
            if l0 { e1 } else { e0 }
        } else if b0 != b1 {
            if b0 > b1 { e0 } else { e1 }
        } else {
            let (s0, s1) = (score(e0), score(e1));
            if s0 >= s1 { e0 } else { e1 }
        };
        let (pa, pb) = (c, far);
        let mid = pa.lerp(pb, 0.5);
        if !in_or_near(mid, 700.0) {
            continue;
        }
        hinges.push(pa);
        doors.push((pa, pb, if role.is_door() { role } else { LayerRole::Door }));
    }
    // fusionar puertas dobles (vanos colineales que se tocan)
    let mut merged: Vec<(Pt, Pt, LayerRole)> = Vec::new();
    'outer: for dr in doors {
        for m in merged.iter_mut() {
            let ends = [(m.0, m.1), (m.1, m.0)];
            for (ma, mb) in ends {
                for (da, db) in [(dr.0, dr.1), (dr.1, dr.0)] {
                    let dir_m = (mb - ma).norm();
                    let dir_d = (db - da).norm();
                    if mb.dist(da) < 250.0 && dir_m.dot(dir_d) > 0.95 {
                        *m = (ma, db, m.2);
                        continue 'outer;
                    }
                    // misma puerta detectada dos veces
                    if ma.dist(da) < 150.0 && mb.dist(db) < 150.0 {
                        continue 'outer;
                    }
                }
            }
        }
        merged.push(dr);
    }
    // segmentos explícitos en capas de puerta principal/salida sin arco
    for (r, p, _) in &g.paths {
        if !matches!(r, LayerRole::MainDoor | LayerRole::ExitDoor | LayerRole::OfficeDoor) || p.len() != 2 {
            continue;
        }
        // hoja de una puerta ya detectada por su arco (sale de la bisagra)
        if hinges.iter().any(|h| h.dist(p[0]) < 60.0 || h.dist(p[1]) < 60.0) {
            continue;
        }
        let (a, b) = (p[0], p[1]);
        let w = a.dist(b);
        if !(500.0..=3500.0).contains(&w) {
            continue;
        }
        // la hoja dibujada sale perpendicular desde un extremo: un vano de verdad está sobre el borde del recinto
        let mid = a.lerp(b, 0.5);
        let on_edge = f.cell_idx(mid).map(|i| (region_dist[i] as f64) <= res * 3.0 && !region_mask[i] || {
            // vano: el punto medio está en el borde
            let dir = (b - a).norm().perp();
            let p1 = mid + dir * 300.0;
            let p2 = mid - dir * 300.0;
            inside(p1) != inside(p2)
        }).unwrap_or(false);
        if !on_edge {
            continue;
        }
        if merged.iter().any(|m| m.0.lerp(m.1, 0.5).dist(mid) < w * 0.6) {
            continue;
        }
        merged.push((a, b, *r));
    }
    let mut widest: Option<usize> = None;
    for (i, (a, b, role)) in merged.iter().enumerate() {
        let kind = match role {
            LayerRole::MainDoor => DoorKind::Main,
            LayerRole::ExitDoor => DoorKind::Exit,
            _ => DoorKind::Office,
        };
        if widest.map(|w: usize| merged[w].0.dist(merged[w].1) < a.dist(*b)).unwrap_or(true) {
            widest = Some(i);
        }
        plan.doors.push(Door { id: format!("d{}", i + 1), kind, a: *a, b: *b, label: String::new() });
    }
    if !plan.doors.iter().any(|d| d.kind == DoorKind::Main) {
        if let Some(w) = widest {
            plan.doors[w].kind = DoorKind::Main;
            notes.push("Se marcó como puerta principal la más ancha. Revísalo en la pestaña Plano.".into());
        } else {
            notes.push("No se detectaron puertas: marca la puerta principal manualmente.".into());
        }
    }
    // rótulos de puertas cercanos
    for dr in plan.doors.iter_mut() {
        let mid = dr.mid();
        let mut best: Option<(f64, &str)> = None;
        for t in &d.texts {
            let p = t.pos * opts.mm_per_unit;
            let dd = p.dist(mid);
            if dd < 2500.0 && best.map(|b| dd < b.0).unwrap_or(true) {
                best = Some((dd, &t.text));
            }
        }
        if let Some((_, t)) = best {
            dr.label = t.chars().take(30).collect();
        }
    }

    // ---------------------------------------------------------------- ventanas
    // una ventana real tiene recinto de un solo lado
    let one_sided = |a: Pt, b: Pt| -> bool {
        let nrm = (b - a).norm().perp();
        let off = res * 3.0 + 300.0;
        let mut ok = 0;
        for t in [0.25, 0.5, 0.75] {
            let m = a.lerp(b, t);
            if inside(m + nrm * off) != inside(m - nrm * off) {
                ok += 1;
            }
        }
        ok >= 2
    };
    for (r, p, c) in &g.paths {
        if *r != LayerRole::Window {
            continue;
        }
        let n = p.len();
        let segs = if closed_like(p, *c) { n } else { n - 1 };
        for i in 0..segs {
            let (a, b) = (p[i], p[(i + 1) % n]);
            if a.dist(b) < 400.0 {
                continue;
            }
            if in_or_near(a.lerp(b, 0.5), 600.0) && one_sided(a, b) {
                plan.windows.push(Window { a, b });
            }
        }
    }
    plan.windows = merge_windows(plan.windows);

    let mut area = region_mask.iter().filter(|&&b| b).count() as f64 * res * res / 1e6;
    if !outlines.is_empty() {
        area -= plan.obstacles.iter().map(|o| polygon_area(&o.poly).abs() / 1e6).sum::<f64>();
    }
    notes.push(format!("{} puertas, {} obstáculos, {} tramos de ventana detectados.", plan.doors.len(), plan.obstacles.len(), plan.windows.len()));
    Ok(AnalyzeResult { plan, notes, area_m2: area })
}

/// Fusiona tramos de ventana colineales y contiguos; descarta duplicados (vidrio doble).
fn merge_windows(ws: Vec<Window>) -> Vec<Window> {
    let mut out: Vec<Window> = Vec::new();
    for w in ws {
        let dir = (w.b - w.a).norm();
        let mut done = false;
        for o in out.iter_mut() {
            let od = (o.b - o.a).norm();
            if dir.dot(od).abs() < 0.995 {
                continue;
            }
            // distancia perpendicular entre las rectas
            if dist_point_segment(w.a, o.a - od * 1e6, o.b + od * 1e6) > 400.0 {
                continue;
            }
            // proyecciones sobre la recta de o
            let t = |p: Pt| (p - o.a).dot(od);
            let (o0, o1) = (0.0, t(o.b));
            let (w0, w1) = (t(w.a).min(t(w.b)), t(w.a).max(t(w.b)));
            if w1 < o0 - 300.0 || w0 > o1 + 300.0 {
                continue;
            }
            let lo = o0.min(w0);
            let hi = o1.max(w1);
            let base = o.a;
            o.a = base + od * lo;
            o.b = base + od * hi;
            done = true;
            break;
        }
        if !done {
            out.push(w);
        }
    }
    out
}
