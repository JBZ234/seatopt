//! Contexto precalculado de una corrida: máscaras del mundo, despejes, pasillo principal,
//! zonas y rejillas rotadas para cada orientación candidata.

use super::dial;
use crate::defaults::RuleView;
use crate::geom::*;
use crate::model::*;
use crate::raster::{self, Frame};
use rayon::prelude::*;
use std::collections::BinaryHeap;

/// Parámetros resueltos de las reglas (mm salvo indicación).
#[derive(Clone, Debug)]
pub struct P {
    pub c1_on: bool,
    pub c1_w: f64,
    pub c1_exits: bool,
    pub c1_offices: bool,
    pub c1_straight: bool,
    pub c2_w: f64,
    pub squeeze: f64,
    pub c3: Option<(f64, f64)>,
    pub c4_on: bool,
    pub c4_swing: bool,
    pub c4_extra: f64,
    pub c4_exits: bool,
    pub c4_offices: bool,
    pub c5: Option<f64>,
    pub c6: Option<f64>,
    pub c7_on: bool,
    pub c7_from_main: bool,
    pub c7_doors: bool,
    pub c8: Option<(f64, f64, f64)>,
    pub e1_wall: f64,
    pub chair: f64,
    pub e2_ss: f64,
    pub e2_se: f64,
    pub e2_backs: bool,
    pub e3_max: u32,
    pub e3_cut: f64,
    pub e4: f64,
    pub e5: bool,
    pub e6_diag: bool,
    pub e6_tol: f64,
    pub e7_w: f64,
    pub e7_hard: bool,
    pub e8_touch: bool,
    pub e8_sep: f64,
    pub o1: f64,
    pub o2: f64,
    pub o3: f64,
    pub o4: bool,
    pub a1: Option<(f64, f64)>,
    pub a1_w: f64,
    pub a2_door: Option<f64>,
    pub a2_corr: bool,
    pub a2_w: f64,
    pub a3_entry: Option<f64>,
    pub a3_corr: bool,
    pub a3_w: f64,
    pub a4: Option<f64>,
    pub a4_w: f64,
    pub a5: Option<f64>,
    pub a6: Option<f64>,
}

impl P {
    pub fn from_rules(rules: &[Rule]) -> P {
        let r = RuleView::new(rules);
        let chair = r.val_or("E1", "silla", 500.0);
        let c2_w = r.val("C2", "ancho").unwrap_or(600.0);
        let squeeze = r.val("C2", "paso_sillas").unwrap_or(c2_w).min(c2_w);
        P {
            c1_on: r.on("C1"),
            c1_w: r.val("C1", "ancho").unwrap_or(c2_w),
            c1_exits: r.item_on("C1", "salidas"),
            c1_offices: r.item_on("C1", "oficinas"),
            c1_straight: r.item_on("C1", "recto"),
            c2_w,
            squeeze,
            c3: if r.on("C3") { Some((r.val("C3", "profundidad").unwrap_or(0.0), r.val("C3", "ancho").unwrap_or(0.0))) } else { None },
            c4_on: r.on("C4"),
            c4_swing: r.item_on("C4", "barrido"),
            c4_extra: r.val("C4", "extra").unwrap_or(0.0),
            c4_exits: r.item_on("C4", "salidas"),
            c4_offices: r.item_on("C4", "oficinas"),
            c5: r.val("C5", "dist"),
            c6: r.val("C6", "dist"),
            c7_on: r.on("C7"),
            c7_from_main: r.item_on("C7", "desde_entrada"),
            c7_doors: r.item_on("C7", "puertas"),
            c8: if r.on("C8") { Some((r.val_or("C8", "pct", 2.0), r.val_or("C8", "giro", 1500.0), r.val_or("C8", "ruta", 900.0))) } else { None },
            e1_wall: r.val("E1", "muro").unwrap_or(chair).max(chair),
            chair,
            e2_ss: r.val("E2", "silla_silla").unwrap_or(2.0 * chair),
            e2_se: r.val("E2", "silla_espalda").unwrap_or(chair),
            e2_backs: !r.on("E2") || r.item_on("E2", "pegar_espaldas"),
            e3_max: r.val("E3", "max").map(|v| v.max(1.0) as u32).unwrap_or(10_000),
            e3_cut: r.val("E3", "corte").unwrap_or(c2_w),
            e4: r.val("E4", "dist").unwrap_or(f64::INFINITY),
            e5: r.on("E5"),
            e6_diag: !r.on("E6") || r.item_on("E6", "muros_inclinados"),
            e6_tol: r.val("E6", "tolerancia").unwrap_or(4.0),
            e7_w: r.weight("E7"),
            e7_hard: r.on("E7") && r.hard("E7"),
            e8_touch: !r.on("E8") || r.item_on("E8", "tocar"),
            e8_sep: if r.on("E8") && !r.item_on("E8", "tocar") { r.val_or("E8", "separacion", 300.0) } else { 0.0 },
            o1: r.val("O1", "dist").unwrap_or(0.0),
            o2: r.val("O2", "dist").unwrap_or(0.0),
            o3: r.val("O3", "dist").unwrap_or(0.0),
            o4: r.item_on("O4", "activo"),
            a1: if r.on("A1") { Some((r.val_or("A1", "dist", 8000.0), r.val_or("A1", "tolerancia", 45.0))) } else { None },
            a1_w: r.weight("A1"),
            a2_door: r.val("A2", "puerta"),
            a2_corr: r.item_on("A2", "pasillo"),
            a2_w: r.weight("A2"),
            a3_entry: r.val("A3", "entrada"),
            a3_corr: r.item_on("A3", "pasillo"),
            a3_w: r.weight("A3"),
            a4: r.val("A4", "dist"),
            a4_w: r.weight("A4"),
            a5: r.val("A5", "m2"),
            a6: r.val("A6", "pct"),
        }
    }
}

pub const F_HARD: u8 = 1;
/// Cantidad «infinita» para tipos en «máximo posible».
pub const MAX_UNITS: u32 = 20_000;
pub const F_KEEP: u8 = 2;

/// Rejilla en el marco rotado de un ángulo θ (filas paralelas al eje x local).
pub struct RotGrid {
    pub theta: f64,
    pub w: usize,
    pub h: usize,
    /// Origen (esquina) del marco rotado, en coordenadas rotadas (mm).
    pub origin: Pt,
    pub res: f64,
    pub flags: Vec<u8>,
    pub zone: Vec<u16>,
    /// Caja (x0, y0, x1, y1) de cada zona en celdas (índice = id de zona).
    pub zone_bbox: Vec<(usize, usize, usize, usize)>,
    /// Celdas consecutivas hacia +y libres (sin bloqueo ni despeje) y de la misma zona.
    pub up_ok: Vec<u16>,
    /// Celdas consecutivas hacia +y sin bloqueo duro.
    pub up_nohard: Vec<u16>,
}

impl RotGrid {
    #[inline]
    pub fn to_world(&self, x_mm: f64, y_mm: f64) -> Pt {
        pt(self.origin.x + x_mm, self.origin.y + y_mm).rot(self.theta)
    }
}

pub struct ZoneInfo {
    pub cells: usize,
    pub centroid: Pt,
}

/// Variante de pasillo principal (y por lo tanto de zonas).
pub struct Variant {
    pub name: String,
    pub corridor: Vec<bool>,
    pub corridor_polys: Vec<Vec<Pt>>,
    pub zone_map: Vec<u16>,
    pub zones: Vec<ZoneInfo>,
    pub grids: Vec<RotGrid>,
    pub violations: Vec<Violation>,
    /// Distancia (mm) al pasillo principal.
    pub corridor_dist: Vec<f32>,
}

pub struct DoorInfo {
    pub door: Door,
    pub inward: Pt,
    pub seeds: Vec<usize>,
    /// ¿Se llega a esta puerta desde la entrada con el piso vacío? (si no, es un problema del plano)
    pub reachable_empty: bool,
}

pub struct Ctx {
    pub f: Frame,
    pub res: f64,
    pub p: P,
    pub free: Vec<bool>,
    /// Libre antes de restar los escritorios fijados.
    pub free_base: Vec<bool>,
    pub keep0: Vec<bool>,
    pub keep_polys: Vec<Vec<Pt>>,
    pub doors: Vec<DoorInfo>,
    pub main_seeds: Vec<usize>,
    pub exit_seeds: Vec<usize>,
    /// Celdas libres junto a las puertas (siempre transitables para llegar al vano).
    pub door_approach: Vec<bool>,
    pub windows: Vec<Window>,
    pub win_dist: Vec<f32>,
    pub angles: Vec<f64>,
    pub principal: f64,
    pub variants: Vec<Variant>,
    pub catalog: Vec<DeskType>,
    /// Demanda por tipo (unidades del tipo: módulos o puestos).
    pub demand: Vec<u32>,
    /// Tipos en «máximo posible».
    pub is_max: Vec<bool>,
    pub area_m2: f64,
    pub config: Config,
    pub plan: Plan,
    pub pinned: Vec<Module>,
    pub violations: Vec<Violation>,
}

fn auto_res(area_m2: f64) -> f64 {
    if area_m2 > 450.0 {
        100.0
    } else if area_m2 > 160.0 {
        75.0
    } else {
        50.0
    }
}

/// Ángulos candidatos (radianes) a partir de los bordes del recinto y de oficinas fijas.
fn candidate_angles(plan: &Plan, p: &P) -> (Vec<f64>, f64) {
    let mut hist: Vec<(f64, f64)> = Vec::new(); // (ángulo mod 90 en grados, peso)
    let mut add_ring = |r: &[Pt], wmul: f64| {
        let n = r.len();
        for i in 0..n {
            let a = r[i];
            let b = r[(i + 1) % n];
            let len = a.dist(b);
            if len < 300.0 {
                continue;
            }
            let ang = (b.y - a.y).atan2(b.x - a.x).to_degrees();
            let m = ((ang % 90.0) + 90.0) % 90.0;
            hist.push((m, len * wmul));
        }
    };
    for r in &plan.region {
        add_ring(r, 1.0);
    }
    for o in &plan.obstacles {
        if o.kind == ObstacleKind::FixedOffice {
            add_ring(&o.poly, 0.3);
        }
    }
    let tol = p.e6_tol.max(0.5);
    // agrupar
    let mut fams: Vec<(f64, f64)> = Vec::new(); // (ángulo medio, peso)
    hist.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    for (a, w) in hist {
        let mut done = false;
        for f in fams.iter_mut() {
            let d = ((a - f.0 + 45.0).rem_euclid(90.0)) - 45.0;
            if d.abs() <= tol {
                f.0 = (f.0 * f.1 + (f.0 + d) * w) / (f.1 + w);
                f.1 += w;
                done = true;
                break;
            }
        }
        if !done {
            fams.push((a, w));
        }
    }
    fams.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    if fams.is_empty() {
        fams.push((0.0, 1.0));
    }
    let total: f64 = fams.iter().map(|f| f.1).sum();
    let principal = fams[0].0.rem_euclid(90.0);
    let mut out = vec![principal, principal + 90.0];
    if p.e6_diag {
        for f in fams.iter().skip(1).take(2) {
            if f.1 >= 0.08 * total {
                let a = f.0.rem_euclid(90.0);
                out.push(a);
                out.push(a + 90.0);
            }
        }
    }
    (out.into_iter().map(|d| d.to_radians()).collect(), principal.to_radians())
}

impl Ctx {
    pub fn build(problem: &Problem) -> Result<Ctx, String> {
        let plan = &problem.plan;
        let cfg = &problem.config;
        if plan.region.is_empty() {
            return Err("El plano no tiene recinto.".into());
        }
        let p = P::from_rules(&cfg.rules);
        let mut bb = BBox::empty();
        for r in &plan.region {
            for &q in r {
                bb.add(q);
            }
        }
        let mut region_area: f64 = plan.region.iter().map(|r| polygon_area(r).abs()).sum::<f64>() / 1e6;
        region_area = region_area.max(1.0);
        let res = if cfg.settings.resolution > 0.0 { cfg.settings.resolution } else { auto_res(region_area) };
        let f = Frame::covering(bb, res, 600.0);
        let n = f.len();
        if n > 6_000_000 {
            return Err("El plano es demasiado grande para la resolución elegida. Aumenta el tamaño de celda.".into());
        }

        // ------------------------------------------------ libre y obstáculos
        let mut free = vec![false; n];
        raster::fill_rings(&f, &mut free, &plan.region, true);
        let mut col = vec![false; n];
        let mut office = vec![false; n];
        let mut restr = vec![false; n];
        for o in &plan.obstacles {
            match o.kind {
                ObstacleKind::Column => raster::fill_polygon(&f, &mut col, &o.poly, true),
                ObstacleKind::FixedOffice => raster::fill_polygon(&f, &mut office, &o.poly, true),
                ObstacleKind::Restricted => raster::fill_polygon(&f, &mut restr, &o.poly, true),
                ObstacleKind::Reserved => {
                    if p.o4 {
                        raster::fill_polygon(&f, &mut restr, &o.poly, true)
                    }
                }
            }
            // contorno grueso para no perder obstáculos chicos
            let k = o.poly.len();
            for i in 0..k {
                let m: &mut Vec<bool> = match o.kind {
                    ObstacleKind::Column => &mut col,
                    ObstacleKind::FixedOffice => &mut office,
                    ObstacleKind::Restricted => &mut restr,
                    ObstacleKind::Reserved if p.o4 => &mut restr,
                    _ => continue,
                };
                raster::stroke_segment(&f, m, o.poly[i], o.poly[(i + 1) % k], res * 0.5, true);
            }
        }
        for i in 0..n {
            if col[i] || office[i] || restr[i] {
                free[i] = false;
            }
        }
        let area_m2 = free.iter().filter(|&&b| b).count() as f64 * res * res / 1e6;

        let free_base = free.clone();
        // pinned: cuerpos como obstáculos duros
        for m in &cfg.pinned {
            let mut tmp = vec![false; n];
            raster::fill_polygon(&f, &mut tmp, &m.body, true);
            for i in 0..n {
                if tmp[i] {
                    free[i] = false;
                }
            }
        }

        // ------------------------------------------------ despejes (keepouts)
        let mut keep0 = vec![false; n];
        let mut keep_polys = Vec::new();
        let buffer = |src: &Vec<bool>, d: f64, keep: &mut Vec<bool>| {
            if d <= 0.0 || !src.iter().any(|&b| b) {
                return;
            }
            let e = raster::edt(&f, src);
            for i in 0..n {
                if (e[i] as f64) <= d {
                    keep[i] = true;
                }
            }
        };
        buffer(&col, p.o1, &mut keep0);
        buffer(&restr, p.o2, &mut keep0);
        buffer(&office, p.o3, &mut keep0);

        let inside = |q: Pt| f.cell_idx(q).map(|i| free[i]).unwrap_or(false);
        let mut doors = Vec::new();
        let mut main_seeds = Vec::new();
        let mut exit_seeds = Vec::new();
        for d in &plan.doors {
            if d.kind == DoorKind::Ignore {
                continue;
            }
            let w = d.width().max(300.0);
            let u = (d.b - d.a).norm();
            let nrm = u.perp();
            let mid = d.mid();
            let probe = |s: f64| -> usize { (1..=8).filter(|k| inside(mid + nrm * (s * (*k as f64) * 150.0))).count() };
            let inward = if probe(1.0) >= probe(-1.0) { nrm } else { -nrm };
            // semillas: celdas libres justo adentro del vano
            let mut seeds = Vec::new();
            for k in 0..=12 {
                let t = k as f64 / 12.0;
                for off in [0.6, 1.2, 2.0, 3.0] {
                    let q = d.a.lerp(d.b, t) + inward * (res * off);
                    if let Some(i) = f.cell_idx(q) {
                        if free[i] {
                            seeds.push(i);
                        }
                    }
                }
            }
            seeds.sort();
            seeds.dedup();
            // zona de despeje
            let applies = match d.kind {
                DoorKind::Main => true,
                DoorKind::Exit => p.c4_exits,
                DoorKind::Office => p.c4_offices,
                DoorKind::Ignore => false,
            };
            let mut zones: Vec<Vec<Pt>> = Vec::new();
            if p.c4_on && applies {
                let depth = if p.c4_swing { w } else { 0.0 } + p.c4_extra;
                if depth > 0.0 {
                    let half = w * 0.5 + 200.0;
                    zones.push(vec![mid - u * half, mid + u * half, mid + u * half + inward * depth, mid - u * half + inward * depth]);
                }
            }
            if d.kind == DoorKind::Main {
                if let Some((dep, wid)) = p.c3 {
                    let half = wid.max(w) * 0.5;
                    zones.push(vec![mid - u * half, mid + u * half, mid + u * half + inward * dep, mid - u * half + inward * dep]);
                }
                main_seeds.extend(seeds.iter().copied());
            }
            if matches!(d.kind, DoorKind::Main | DoorKind::Exit) {
                exit_seeds.extend(seeds.iter().copied());
            }
            for z in zones {
                raster::fill_polygon(&f, &mut keep0, &z, true);
                keep_polys.push(z);
            }
            doors.push(DoorInfo { door: d.clone(), inward, seeds, reachable_empty: true });
        }
        let mut violations = Vec::new();
        if main_seeds.is_empty() {
            violations.push(Violation { rule_id: "C7".into(), message: "No hay puerta principal marcada (o no toca el recinto).".into(), at: None, hard: true });
            // usar cualquier puerta como respaldo
            if let Some(d) = doors.first() {
                main_seeds = d.seeds.clone();
            }
        }
        if exit_seeds.is_empty() {
            exit_seeds = main_seeds.clone();
        }
        for m in &cfg.pinned {
            let mut tmp = vec![false; n];
            raster::fill_polygon(&f, &mut tmp, &m.body, true);
            let d = raster::edt(&f, &tmp);
            for i in 0..n {
                if (d[i] as f64) <= p.chair + 100.0 {
                    keep0[i] = true;
                }
            }
        }
        for i in 0..n {
            if !free[i] {
                keep0[i] = false;
            }
        }

        let mut door_approach = vec![false; n];
        for d in &doors {
            raster::stroke_segment(&f, &mut door_approach, d.door.a, d.door.b, p.c1_w.max(p.c2_w) * 0.5 + 300.0, true);
        }
        for i in 0..n {
            door_approach[i] = door_approach[i] && free[i];
        }

        // puertas alcanzables con el piso vacío
        {
            let hard: Vec<bool> = free.iter().map(|b| !b).collect();
            let clear = raster::edt(&f, &hard);
            let pass: Vec<bool> = (0..n).map(|i| free[i] && ((clear[i] as f64) >= p.c2_w * 0.5 - res * 0.5 || door_approach[i])).collect();
            let d = super::dial::dial(&f, &pass, &main_seeds, u32::MAX - 1);
            for di in doors.iter_mut() {
                di.reachable_empty = di.seeds.iter().any(|&s| d[s] != super::dial::UNREACH);
                if !di.reachable_empty && di.door.kind != DoorKind::Main {
                    violations.push(Violation {
                        rule_id: "C7".into(),
                        message: format!("La puerta {} no se alcanza desde la entrada ni con el piso vacío (revisa el plano o márcala como «ignorar»).", if di.door.label.is_empty() { &di.door.id } else { &di.door.label }),
                        at: Some(di.door.mid()),
                        hard: false,
                    });
                }
            }
        }

        // ------------------------------------------------ ventanas
        let mut wm = vec![false; n];
        for w in &plan.windows {
            raster::stroke_segment(&f, &mut wm, w.a, w.b, res * 0.6, true);
        }
        let win_dist = if plan.windows.is_empty() { vec![f32::INFINITY; n] } else { raster::edt(&f, &wm) };

        // ------------------------------------------------ catálogo y demanda
        let catalog = cfg.catalog.clone();
        let mut demand = vec![0u32; catalog.len()];
        let mut is_max = vec![false; catalog.len()];
        for d in &cfg.demand {
            if let Some(i) = catalog.iter().position(|t| t.id == d.type_id) {
                if d.max {
                    is_max[i] = true;
                } else {
                    demand[i] += d.quantity;
                }
            }
        }
        for i in 0..catalog.len() {
            if is_max[i] {
                demand[i] = MAX_UNITS;
            }
        }
        // los fijados descuentan demanda
        for m in &cfg.pinned {
            if let Some(i) = catalog.iter().position(|t| t.id == m.type_id) {
                let units = if catalog[i].flexible { catalog[i].seats_per_module(m.per_side) } else { 1 };
                demand[i] = demand[i].saturating_sub(units);
            }
        }

        let (angles, principal) = candidate_angles(plan, &p);

        let mut ctx = Ctx {
            f,
            res,
            p,
            free,
            free_base,
            keep0,
            keep_polys,
            doors,
            main_seeds,
            exit_seeds,
            door_approach,
            windows: plan.windows.clone(),
            win_dist,
            angles,
            principal,
            variants: vec![],
            catalog,
            demand,
            is_max,
            area_m2,
            config: cfg.clone(),
            plan: plan.clone(),
            pinned: cfg.pinned.clone(),
            violations,
        };
        ctx.variants = ctx.build_variants();
        Ok(ctx)
    }

    fn build_variants(&self) -> Vec<Variant> {
        let styles: Vec<u8> = if self.p.c1_on { vec![0, 1, 2] } else { vec![255] };
        styles.par_iter().map(|&s| self.build_variant(s)).collect()
    }

    fn build_variant(&self, style: u8) -> Variant {
        let n = self.f.len();
        let (corridor, violations) = if style == 255 { (vec![false; n], vec![]) } else { self.corridor(style) };
        let name = match style {
            0 => "Pasillo más corto",
            1 => "Pasillo por el centro",
            2 => "Pasillo paralelo a los muros",
            _ => "Sin pasillo principal",
        }
        .to_string();
        let corridor_polys = if corridor.iter().any(|&b| b) {
            raster::contours(&self.f, &corridor).into_iter().map(|r| simplify_ring(&r, self.res * 0.8)).filter(|r| r.len() >= 3).collect()
        } else {
            vec![]
        };
        // zonas
        let zfree: Vec<bool> = (0..n).map(|i| self.free[i] && !self.keep0[i] && !corridor[i]).collect();
        let (lab, sizes) = raster::components(&self.f, &zfree);
        let min_cells = (2.0e6 / (self.res * self.res)) as usize;
        let mut remap = vec![0u16; sizes.len()];
        let mut zones = vec![ZoneInfo { cells: 0, centroid: pt(0.0, 0.0) }];
        for (i, &s) in sizes.iter().enumerate().skip(1) {
            if s >= min_cells && zones.len() < 60_000 {
                remap[i] = zones.len() as u16;
                zones.push(ZoneInfo { cells: s, centroid: pt(0.0, 0.0) });
            }
        }
        let mut zone_map = vec![0u16; n];
        let mut sums = vec![pt(0.0, 0.0); zones.len()];
        for i in 0..n {
            let z = remap[lab[i] as usize];
            zone_map[i] = z;
            if z > 0 {
                sums[z as usize] = sums[z as usize] + self.f.center_i(i);
            }
        }
        for z in 1..zones.len() {
            zones[z].centroid = sums[z] * (1.0 / zones[z].cells as f64);
        }
        let grids = self.angles.iter().map(|&th| self.rot_grid(th, &corridor, &zone_map, zones.len())).collect();
        let corridor_dist = if corridor.iter().any(|&b| b) { raster::edt(&self.f, &corridor) } else { vec![f32::INFINITY; n] };
        Variant { name, corridor, corridor_polys, zone_map, zones, grids, violations, corridor_dist }
    }

    fn rot_grid(&self, theta: f64, corridor: &[bool], zone_map: &[u16], nz: usize) -> RotGrid {
        let f = &self.f;
        // caja del recinto en el marco rotado
        let mut bb = BBox::empty();
        for r in &self.plan.region {
            for &q in r {
                bb.add(q.rot(-theta));
            }
        }
        let bb = bb.expand(self.res);
        let res = self.res;
        let w = (bb.w() / res).ceil() as usize + 1;
        let h = (bb.h() / res).ceil() as usize + 1;
        let mut flags = vec![F_HARD; w * h];
        let mut zone = vec![0u16; w * h];
        let mut zb = vec![(usize::MAX, usize::MAX, 0usize, 0usize); nz];
        for y in 0..h {
            for x in 0..w {
                let c = pt(bb.min.x + (x as f64 + 0.5) * res, bb.min.y + (y as f64 + 0.5) * res).rot(theta);
                let Some(i) = f.cell_idx(c) else { continue };
                let k = y * w + x;
                if !self.free[i] {
                    continue;
                }
                let mut fl = 0u8;
                if self.keep0[i] || corridor[i] {
                    fl |= F_KEEP;
                }
                flags[k] = fl;
                let z = zone_map[i];
                zone[k] = z;
                if z > 0 {
                    let b = &mut zb[z as usize];
                    b.0 = b.0.min(x);
                    b.1 = b.1.min(y);
                    b.2 = b.2.max(x);
                    b.3 = b.3.max(y);
                }
            }
        }
        let mut up_ok = vec![0u16; w * h];
        let mut up_nohard = vec![0u16; w * h];
        for x in 0..w {
            for y in (0..h).rev() {
                let k = y * w + x;
                if flags[k] & F_HARD == 0 {
                    let nxt = if y + 1 < h { up_nohard[k + w] } else { 0 };
                    up_nohard[k] = nxt.saturating_add(1);
                }
                if flags[k] == 0 && zone[k] > 0 {
                    let nxt = if y + 1 < h && zone[k + w] == zone[k] { up_ok[k + w] } else { 0 };
                    up_ok[k] = nxt.saturating_add(1);
                }
            }
        }
        RotGrid { theta, w, h, origin: bb.min, res, flags, zone, zone_bbox: zb, up_ok, up_nohard }
    }

    /// Pasillo principal: árbol de caminos desde la puerta principal a salidas / puertas de oficinas,
    /// con penalización de giros y costo según el estilo.
    fn corridor(&self, style: u8) -> (Vec<bool>, Vec<Violation>) {
        let f = &self.f;
        let n = f.len();
        let p = &self.p;
        let mut viol = Vec::new();
        let hard: Vec<bool> = self.free.iter().map(|b| !b).collect();
        let clear = raster::edt(f, &hard);
        let half = p.c1_w * 0.5;
        // celdas cerca de puertas: se permite angostar para llegar al vano
        let mut near_door = vec![false; n];
        for d in &self.doors {
            raster::stroke_segment(f, &mut near_door, d.door.a, d.door.b, p.c1_w.max(1200.0), true);
        }
        let targets: Vec<&DoorInfo> = self
            .doors
            .iter()
            .filter(|d| d.reachable_empty)
            .filter(|d| match d.door.kind {
                DoorKind::Exit => p.c1_exits,
                DoorKind::Office => p.c1_offices,
                _ => false,
            })
            .collect();
        if targets.is_empty() || self.main_seeds.is_empty() {
            return (vec![false; n], viol);
        }
        let cost_of = |i: usize| -> f64 {
            let c = clear[i] as f64;
            match style {
                1 => 1.0 + 2.5 * (1.0 - (c / 6000.0).min(1.0)),
                2 => {
                    let ideal = half + 600.0 + p.e1_wall;
                    1.0 + 2.0 * ((c - ideal).abs() / ideal).min(1.0)
                }
                _ => 1.0,
            }
        };
        let try_width = |hw: f64| -> (Vec<u32>, Vec<f64>) {
            let passable: Vec<bool> = (0..n).map(|i| self.free[i] && ((clear[i] as f64) >= hw || (near_door[i] && clear[i] as f64 >= self.res))).collect();
            self.turn_dijkstra(&passable, &cost_of, if p.c1_straight { 6.0 } else { 0.5 })
        };
        let (parent, dist) = try_width(half);
        let mut narrow: Option<(Vec<u32>, Vec<f64>)> = None;
        let mut path = vec![false; n];
        for t in targets {
            // mejor estado de llegada
            let pick = |dist: &Vec<f64>| -> Option<usize> {
                let mut best: Option<(f64, usize)> = None;
                for &s in &t.seeds {
                    for d in 0..8 {
                        let st = s * 8 + d;
                        if dist[st].is_finite() && best.map(|b| dist[st] < b.0).unwrap_or(true) {
                            best = Some((dist[st], st));
                        }
                    }
                }
                best.map(|b| b.1)
            };
            let fallback = match pick(&dist) {
                Some(_) => None,
                None => {
                    if narrow.is_none() {
                        narrow = Some(try_width(p.c2_w * 0.5));
                    }
                    let nw = narrow.as_ref().unwrap();
                    Some(pick(&nw.1))
                }
            };
            let (par, st) = match fallback {
                None => (&parent, pick(&dist).unwrap()),
                Some(alt) => match alt {
                    Some(st) => {
                        viol.push(Violation {
                            rule_id: "C1".into(),
                            message: format!("El pasillo principal no alcanza {:.0} mm hasta la puerta {}; se usa el ancho secundario.", p.c1_w, if t.door.label.is_empty() { &t.door.id } else { &t.door.label }),
                            at: Some(t.door.mid()),
                            hard: true,
                        });
                        (&narrow.as_ref().unwrap().0, st)
                    }
                    None => {
                        viol.push(Violation {
                            rule_id: "C1".into(),
                            message: format!("No hay paso desde la entrada hasta la puerta {}.", if t.door.label.is_empty() { &t.door.id } else { &t.door.label }),
                            at: Some(t.door.mid()),
                            hard: true,
                        });
                        continue;
                    }
                },
            };
            let mut s = st;
            let mut guard = 0;
            while s != u32::MAX as usize && guard < n * 2 {
                let cell = s / 8;
                if path[cell] {
                    // ya unido al árbol
                }
                path[cell] = true;
                let ps = par[s];
                if ps == u32::MAX {
                    break;
                }
                s = ps as usize;
                guard += 1;
            }
        }
        // ensanchar el camino al ancho del pasillo
        let d = raster::edt(f, &path);
        let corridor: Vec<bool> = (0..n).map(|i| self.free[i] && (d[i] as f64) <= half).collect();
        (corridor, viol)
    }

    /// Dijkstra con estado (celda, dirección) y penalización por giro. Devuelve padres y distancias por estado.
    fn turn_dijkstra(&self, passable: &[bool], cost: &dyn Fn(usize) -> f64, turn_pen: f64) -> (Vec<u32>, Vec<f64>) {
        let f = &self.f;
        let n = f.len();
        // direcciones alineadas con el ángulo principal: aproximamos con 8 vecinos de la rejilla del mundo;
        // la penalización por giro hace que los tramos sean largos y rectos.
        const D: [(i32, i32); 8] = [(1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1), (0, -1), (1, -1)];
        // diagonales solo si la planta tiene muros a ~45° del principal
        let diag_ok = self.angles.iter().any(|&a| {
            let d = (a - self.principal).to_degrees().rem_euclid(90.0);
            (d - 45.0).abs() < 10.0
        });
        let mut dist = vec![f64::INFINITY; n * 8];
        let mut parent = vec![u32::MAX; n * 8];
        #[derive(PartialEq)]
        struct S(f64, usize);
        impl Eq for S {}
        impl PartialOrd for S {
            fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for S {
            fn cmp(&self, o: &Self) -> std::cmp::Ordering {
                o.0.partial_cmp(&self.0).unwrap_or(std::cmp::Ordering::Equal)
            }
        }
        let mut heap = BinaryHeap::new();
        for &s in &self.main_seeds {
            if passable[s] {
                for d in 0..8 {
                    dist[s * 8 + d] = 0.0;
                    heap.push(S(0.0, s * 8 + d));
                }
            }
        }
        while let Some(S(dcur, st)) = heap.pop() {
            if dcur > dist[st] {
                continue;
            }
            let cell = st / 8;
            let dir = st % 8;
            let (x, y) = ((cell % f.w) as i32, (cell / f.w) as i32);
            for (nd, (dx, dy)) in D.iter().enumerate() {
                let diag = nd % 2 == 1;
                if diag && !diag_ok {
                    continue;
                }
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= f.w as i32 || ny >= f.h as i32 {
                    continue;
                }
                let j = ny as usize * f.w + nx as usize;
                if !passable[j] {
                    continue;
                }
                if diag && (!passable[y as usize * f.w + nx as usize] || !passable[ny as usize * f.w + x as usize]) {
                    continue;
                }
                let turn = {
                    let t = (nd as i32 - dir as i32).rem_euclid(8);
                    let t = t.min(8 - t);
                    match t {
                        0 => 0.0,
                        1 => 0.5,
                        2 => 1.0,
                        _ => 3.0,
                    }
                };
                let step = if diag { std::f64::consts::SQRT_2 } else { 1.0 };
                let ndist = dcur + step * cost(j) + turn * turn_pen;
                let ns = j * 8 + nd;
                if ndist < dist[ns] {
                    dist[ns] = ndist;
                    parent[ns] = st as u32;
                    heap.push(S(ndist, ns));
                }
            }
        }
        (parent, dist)
    }

    /// Celdas transitables a pie (libres, sin muebles) — se usa en evaluación.
    pub fn walk_base(&self) -> Vec<bool> {
        self.free.clone()
    }

    pub fn dial_mm(&self, passable: &[bool], seeds: &[usize], limit_mm: f64) -> Vec<u32> {
        dial::dial(&self.f, passable, seeds, dial::from_mm(limit_mm, self.res))
    }
}
