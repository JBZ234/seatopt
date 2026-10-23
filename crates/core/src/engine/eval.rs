//! Evaluación de una disposición: geometría real, validación (accesibilidad, recorridos,
//! recorrido común, PMR), métricas blandas y puntaje lexicográfico.

use super::ctx::Ctx;
use super::decode::{Decoded, PModule};
use super::dial::{self, UNREACH};
use crate::geom::*;
use crate::model::*;
use crate::raster;

/// Puesto en coordenadas del mundo.
#[derive(Clone, Debug)]
pub struct WSeat {
    pub module: usize,
    pub pos: Pt,
    pub facing: Pt,
    pub chair: Vec<Pt>,
    pub access: Pt,
    /// Llegadas por los costados de la silla (sirven en los extremos de fila y escritorios sueltos).
    pub side_access: [Pt; 2],
}

#[derive(Clone, Debug)]
pub struct WModule {
    pub pm: PModule,
    pub center: Pt,
    pub angle: f64,
    pub body: Vec<Pt>,
    pub seats: std::ops::Range<usize>,
    pub pinned: bool,
    /// id del módulo fijado (se conserva en el resultado)
    pub fixed_id: Option<String>,
    pub people: Vec<SeatPerson>,
}

/// Do two desks touch (or nearly, ≤ 150 mm)? Compared on their bounding boxes along the first
/// desk's axes, which is exact for parallel or perpendicular desks.
fn touching(a: &WModule, b: &WModule) -> bool {
    const GAP: f64 = 150.0;
    if a.center.dist(b.center) > (a.pm.len + a.pm.dep + b.pm.len + b.pm.dep) * 0.5 + GAP {
        return false;
    }
    let u = Pt::from_angle(a.angle.to_radians());
    let v = u.perp();
    let ext = |m: &WModule, ax: Pt| -> (f64, f64) {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for q in &m.body {
            let t = q.dot(ax);
            lo = lo.min(t);
            hi = hi.max(t);
        }
        (lo, hi)
    };
    let (au0, au1) = ext(a, u);
    let (bu0, bu1) = ext(b, u);
    let (av0, av1) = ext(a, v);
    let (bv0, bv1) = ext(b, v);
    au0 <= bu1 + GAP && bu0 <= au1 + GAP && av0 <= bv1 + GAP && bv0 <= av1 + GAP
}

/// Puesto que falta de un tipo en «máximo posible» (maximizar).
pub const W_UNPLACED: f64 = 10_000.0;
/// Peso relativo de un puesto de relleno frente a uno en fila flexible.
pub const W_FILLER: f64 = 0.7;
/// Puesto que falta de un tipo con cantidad fija: siempre antes que maximizar.
pub const W_FIXED: f64 = 100_000.0;
/// Regla dura incumplida: lo primero.
pub const W_HARD: f64 = 1_000_000.0;

/// Penalización por unidades sin ubicar, según el tipo (fijo o máximo).
/// Con «máximo posible», los tipos de largo flexible (filas, islas) son los principales y los
/// escritorios sueltos (1–2 personas) rellenan: un puesto de relleno vale `W_FILLER`.
pub fn deficit(ctx: &Ctx, remaining: &[u32]) -> f64 {
    let flex_max = ctx.catalog.iter().zip(&ctx.is_max).any(|(t, &m)| m && t.flexible);
    remaining
        .iter()
        .enumerate()
        .map(|(i, &r)| {
            let t = &ctx.catalog[i];
            let seats = if t.flexible && t.shape == DeskShape::Straight { r } else { r * t.seats_per_module(t.per_side) };
            let w = if !ctx.is_max[i] {
                W_FIXED
            } else if flex_max && !t.flexible {
                W_UNPLACED * W_FILLER
            } else {
                W_UNPLACED
            };
            seats as f64 * w
        })
        .sum()
}

#[derive(Clone, Debug, Default)]
pub struct Eval {
    pub score: f64,
    pub seats_requested: u32,
    pub seats_placed: u32,
    /// Puestos fijos sin ubicar.
    pub unplaced_seats: u32,
    /// Puestos ubicados de tipos «máximo posible».
    pub max_seats: u32,
    pub deficit: f64,
    pub hard: u32,
    pub module_ok: Vec<bool>,
    pub seat_flags: Vec<Vec<String>>,
    pub seat_travel: Vec<f64>,
    pub avg_travel: f64,
    pub max_travel: f64,
    pub daylight_frac: f64,
    pub window_side_frac: f64,
    pub privacy_issues: u32,
    pub orientations: u32,
    pub reserve_seats: u32,
    pub pmr: u32,
    pub violations: Vec<Violation>,
    pub soft: f64,
}

/// Geometría del mundo: módulos fijados + módulos decodificados de la variante.
pub fn world_geometry_var(ctx: &Ctx, dec: &Decoded, variant: usize) -> (Vec<WModule>, Vec<WSeat>) {
    let mut mods = Vec::new();
    let mut seats = Vec::new();
    for pm in &ctx.pinned {
        let Some(ti) = ctx.catalog.iter().position(|t| t.id == pm.type_id) else { continue };
        let t = &ctx.catalog[ti];
        let p = PModule { t: ti, k: pm.per_side, grid: 0, x0: 0.0, y0: 0.0, len: pm.length, dep: pm.depth, seats_up: pm.seats_up, zone: 0 };
        let s0 = seats.len();
        let body = module_shapes(ctx, t, &p, pm.center, pm.angle.to_radians(), &mut seats, mods.len());
        mods.push(WModule { pm: p, center: pm.center, angle: pm.angle, body, seats: s0..seats.len(), pinned: true, fixed_id: Some(pm.id.clone()), people: pm.people.clone() });
    }
    let var = &ctx.variants[variant];
    for pm in &dec.modules {
        let g = &var.grids[pm.grid];
        let t = &ctx.catalog[pm.t];
        let c_rot = pt(pm.x0 + pm.len * 0.5, pm.y0 + pm.dep * 0.5);
        let center = g.to_world(c_rot.x, c_rot.y);
        let s0 = seats.len();
        let body = module_shapes(ctx, t, pm, center, g.theta, &mut seats, mods.len());
        mods.push(WModule { pm: pm.clone(), center, angle: g.theta.to_degrees(), body, seats: s0..seats.len(), pinned: false, fixed_id: None, people: vec![] });
    }
    (mods, seats)
}

pub fn module_shapes(ctx: &Ctx, t: &DeskType, pm: &PModule, center: Pt, ang: f64, seats: &mut Vec<WSeat>, mi: usize) -> Vec<Pt> {
    module_shapes_c(ctx.p.chair, t, pm, center, ang, seats, mi)
}

/// Contorno real de un escritorio fijado (la L o la forma del Studio, no su rectángulo).
pub fn pinned_body(chair: f64, catalog: &[DeskType], m: &crate::model::Module) -> Vec<Pt> {
    let Some(ti) = catalog.iter().position(|t| t.id == m.type_id) else { return m.body.clone() };
    let t = &catalog[ti];
    if t.shape == DeskShape::Straight && m.body.len() >= 3 {
        return m.body.clone();
    }
    let p = PModule { t: ti, k: m.per_side, grid: 0, x0: 0.0, y0: 0.0, len: m.length, dep: m.depth, seats_up: m.seats_up, zone: 0 };
    let mut tmp = Vec::new();
    module_shapes_c(chair, t, &p, m.center, m.angle.to_radians(), &mut tmp, 0)
}

pub fn module_shapes_c(chair: f64, t: &DeskType, pm: &PModule, center: Pt, ang: f64, seats: &mut Vec<WSeat>, mi: usize) -> Vec<Pt> {
    let u = Pt::from_angle(ang);
    let v = u.perp();
    let w = |a: f64, b: f64| center + u * a + v * b;
    let (hl, hd) = (pm.len * 0.5, pm.dep * 0.5);
    let cw = 560.0f64.min(t.seat_width * 0.8);
    let mut add_seat = |a: f64, b: f64, face_sign: f64| {
        let pos = w(a, b);
        let facing = v * face_sign;
        let side = u * (cw * 0.5);
        let back = facing * (-chair * 0.5);
        let front = facing * (chair * 0.5);
        let chair_poly = vec![pos - side + back, pos + side + back, pos + side + front, pos - side + front];
        let access = pos - facing * (chair * 0.5 + 250.0);
        let side_access = [pos - u * (cw * 0.5 + 250.0), pos + u * (cw * 0.5 + 250.0)];
        seats.push(WSeat { module: mi, pos, facing, chair: chair_poly, access, side_access });
    };
    match t.shape {
        DeskShape::Straight => {
            let k = pm.k as usize;
            let two = t.is_two_sided();
            for i in 0..k {
                let a = -hl + (i as f64 + 0.5) * t.seat_width;
                if two || !pm.seats_up {
                    add_seat(a, -hd - chair * 0.5, 1.0);
                }
                if two || pm.seats_up {
                    add_seat(a, hd + chair * 0.5, -1.0);
                }
            }
            vec![w(-hl, -hd), w(hl, -hd), w(hl, hd), w(-hl, hd)]
        }
        DeskShape::L => {
            // lado de la silla: -v (abajo) salvo seats_up; la barra queda del lado opuesto
            let s = if pm.seats_up { -1.0 } else { 1.0 };
            let d = t.depth.min(pm.dep);
            let rd = t.return_depth.max(300.0).min(pm.len * 0.6);
            let body = vec![
                w(-hl, -hd * s),
                w(-hl + rd, -hd * s),
                w(-hl + rd, (hd - d) * s),
                w(hl, (hd - d) * s),
                w(hl, hd * s),
                w(-hl, hd * s),
            ];
            let a = (-hl + rd + hl) * 0.5;
            add_seat(a, (hd - d - chair * 0.5) * s, s);
            if s < 0.0 {
                body.into_iter().rev().collect()
            } else {
                body
            }
        }
        DeskShape::Custom => {
            // diseño del estudio: +y es el lado principal de las sillas; con las sillas «abajo» se refleja
            let my = if pm.seats_up || t.is_two_sided() { 1.0 } else { -1.0 };
            let kx = if t.seat_width > 1.0 { pm.len / t.seat_width } else { 1.0 };
            let ky = if t.depth > 1.0 { pm.dep / t.depth } else { 1.0 };
            for cs in &t.seats {
                let pos = w(cs.x * kx, cs.y * ky * my);
                let facing = (u * cs.facing.x + v * (cs.facing.y * my)).norm();
                let side = facing.perp() * (cw * 0.5);
                let back = facing * (-chair * 0.5);
                let front = facing * (chair * 0.5);
                let chair_poly = vec![pos - side + back, pos + side + back, pos + side + front, pos - side + front];
                let access = pos - facing * (chair * 0.5 + 250.0);
                let sd = facing.perp() * (cw * 0.5 + 250.0);
                seats.push(WSeat { module: mi, pos, facing, chair: chair_poly, access, side_access: [pos - sd, pos + sd] });
            }
            let poly: Vec<Pt> = if t.body.len() >= 3 {
                t.body.iter().map(|q| w(q.x * kx, q.y * ky * my)).collect()
            } else {
                vec![w(-hl, -hd), w(hl, -hd), w(hl, hd), w(-hl, hd)]
            };
            if my < 0.0 { poly.into_iter().rev().collect() } else { poly }
        }
    }
}

fn near_min(f: &raster::Frame, field: &[u32], p: Pt) -> u32 {
    let Some((x, y)) = f.cell(p) else { return UNREACH };
    let mut best = UNREACH;
    for dy in -1i64..=1 {
        for dx in -1i64..=1 {
            let (nx, ny) = (x as i64 + dx, y as i64 + dy);
            if nx < 0 || ny < 0 || nx >= f.w as i64 || ny >= f.h as i64 {
                continue;
            }
            best = best.min(field[ny as usize * f.w + nx as usize]);
        }
    }
    best
}

pub fn evaluate(ctx: &Ctx, dec: &Decoded, variant: usize, weights: &Weights, full: bool) -> (Eval, Vec<WModule>, Vec<WSeat>) {
    let (mods, seats) = world_geometry_var(ctx, dec, variant);
    let f = &ctx.f;
    let n = f.len();
    let p = &ctx.p;
    let res = ctx.res;
    let var = &ctx.variants[variant];

    // ------------------------------------------------ ocupación
    let mut walk = ctx.free.clone();
    for m in &mods {
        raster::fill_polygon(f, &mut walk, &m.body, false);
    }
    for s in &seats {
        raster::fill_polygon(f, &mut walk, &s.chair, false);
    }
    let blocked: Vec<bool> = walk.iter().map(|b| !b).collect();
    let clear = raster::edt(f, &blocked);
    let pass_c2: Vec<bool> = (0..n).map(|i| walk[i] && (clear[i] as f64) >= p.c2_w * 0.5 - res * 0.5).collect();
    let pass_sq: Vec<bool> = (0..n).map(|i| walk[i] && (clear[i] as f64) >= p.squeeze * 0.5 - res * 0.5).collect();
    // las semillas de puertas siempre transitables
    let mut pass_c2 = pass_c2;
    let mut pass_sq = pass_sq;
    for i in 0..n {
        if ctx.door_approach[i] && walk[i] {
            pass_c2[i] = true;
            pass_sq[i] = true;
        }
    }

    let mut ev = Eval { module_ok: vec![true; mods.len()], seat_flags: vec![Vec::new(); seats.len()], ..Default::default() };

    // ------------------------------------------------ acceso real: las sillas se corren, los escritorios no.
    // Un puesto es accesible si se llega a él caminando (por cualquier ruta, no necesariamente directa
    // desde la entrada) sin pasar por un hueco entre escritorios/muros más angosto que el paso mínimo.
    let mut walk_d = ctx.free.clone();
    for m in &mods {
        raster::fill_polygon(f, &mut walk_d, &m.body, false);
    }
    let blocked_d: Vec<bool> = walk_d.iter().map(|b| !b).collect();
    let clear_d = raster::edt(f, &blocked_d);
    let mut pass_d: Vec<bool> = (0..n).map(|i| walk_d[i] && (clear_d[i] as f64) >= p.squeeze * 0.5 - res * 0.5).collect();
    let mut pass_dw: Vec<bool> = (0..n).map(|i| walk_d[i] && (clear_d[i] as f64) >= p.c2_w * 0.5 - res * 0.5).collect();
    for i in 0..n {
        if ctx.door_approach[i] && walk_d[i] {
            pass_d[i] = true;
            pass_dw[i] = true;
        }
    }

    // ------------------------------------------------ accesibilidad (C7 / C2 / E4)
    let seeds: Vec<usize> = if p.c7_from_main || !p.c7_on {
        // separate areas (not connected to the entrance) are reached through their own doors
        let mut s = ctx.main_seeds.clone();
        s.extend(ctx.doors.iter().filter(|d| !d.reachable_empty).flat_map(|d| d.seeds.iter().copied()));
        s.extend(ctx.area_seeds.iter().copied());
        s
    } else {
        let mut s: Vec<usize> = ctx.doors.iter().flat_map(|d| d.seeds.iter().copied()).collect();
        s.extend(ctx.area_seeds.iter().copied());
        s
    };
    let d_any = dial::dial(f, &pass_d, &seeds, u32::MAX - 1);
    let d_doors = if p.c7_on && p.c7_doors { dial::dial(f, &pass_dw, &seeds, u32::MAX - 1) } else { vec![] };
    let d1 = dial::dial(f, &pass_c2, &seeds, u32::MAX - 1);
    let reach: Vec<usize> = (0..n).filter(|&i| d1[i] != UNREACH).collect();
    let lim = p.c2_w * 0.5 + if p.e4.is_finite() { p.e4 } else { 1e9 };
    let d2 = dial::dial(f, &pass_sq, &reach, dial::from_mm(lim, res));
    // puertas bloqueadas por el mobiliario
    if p.c7_on && p.c7_doors {
        for d in &ctx.doors {
            if d.door.kind == DoorKind::Main || !d.reachable_empty {
                continue;
            }
            if !d.seeds.iter().any(|&s| d_doors[s] != UNREACH) {
                ev.hard += 1;
                ev.violations.push(Violation {
                    rule_id: "C7".into(),
                    message: format!("Door {} is blocked.", if d.door.label.is_empty() { &d.door.id } else { &d.door.label }),
                    at: Some(d.door.mid()),
                    hard: true,
                });
            }
        }
    }
    // se llega por detrás de la silla o por uno de sus costados (extremo de fila, escritorio suelto)
    let ok_at = |q: Pt| dial::to_mm(near_min(f, &d2, q), res) <= lim + res;
    // cómodo = por pasillos del ancho preferido y con las sillas en su sitio (solo preferencia)
    let seat_comfy: Vec<bool> = seats.iter().map(|s| ok_at(s.access) || ok_at(s.side_access[0]) || ok_at(s.side_access[1])).collect();
    let ok_any = |q: Pt| near_min(f, &d_any, q) != UNREACH;
    let seat_access_ok: Vec<bool> = seats.iter().map(|s| ok_any(s.pos) || ok_any(s.access) || ok_any(s.side_access[0]) || ok_any(s.side_access[1])).collect();

    // ------------------------------------------------ recorridos a salidas (C5) y recorrido común (C6)
    let need_travel = p.c5.is_some() || weights.travel > 0.0 || full;
    let travel_cap = p.c5.map(|c| c + 2000.0).unwrap_or(120_000.0).max(30_000.0);
    let d3 = if need_travel { dial::dial(f, &pass_d, &ctx.exit_seeds, dial::from_mm(travel_cap, res)) } else { vec![] };
    let d4 = match p.c6 {
        Some(c6) if var.corridor.iter().any(|&b| b) => {
            let cs: Vec<usize> = (0..n).filter(|&i| var.corridor[i] && walk[i]).collect();
            Some((dial::dial(f, &pass_d, &cs, dial::from_mm(c6 + 500.0, res)), c6))
        }
        _ => None,
    };

    let mut travel = vec![0.0; seats.len()];
    for (si, s) in seats.iter().enumerate() {
        let mut bad = false;
        if p.c7_on && !seat_access_ok[si] {
            ev.seat_flags[si].push(format!("No access: boxed in by desks (gaps under {:.2} m)", p.squeeze / 1000.0));
            bad = true;
        }
        if need_travel {
            let t = dial::to_mm(near_min(f, &d3, s.access), res);
            let t = if t.is_finite() { t } else if seat_access_ok[si] { travel_cap } else { f64::INFINITY };
            travel[si] = t;
            if let Some(c5) = p.c5 {
                if t > c5 {
                    ev.seat_flags[si].push(format!("Travel to an exit {:.0} m > {:.0} m", t / 1000.0, c5 / 1000.0));
                    bad = true;
                }
            }
        }
        if let Some((d4, c6)) = &d4 {
            let t = dial::to_mm(near_min(f, d4, s.access), res);
            if t > *c6 {
                ev.seat_flags[si].push(format!("Common travel {:.0} m > {:.0} m", t.min(99_000.0) / 1000.0, c6 / 1000.0));
                bad = true;
            }
        }
        if bad && !mods[s.module].pinned {
            ev.module_ok[s.module] = false;
        }
    }

    // ------------------------------------------------ conteo de ubicados
    let nt = ctx.catalog.len();
    let mut placed_units = vec![0u32; nt];
    let mut placed_seats_t = vec![0u32; nt];
    for (mi, m) in mods.iter().enumerate() {
        if !ev.module_ok[mi] || m.pinned {
            continue;
        }
        let t = &ctx.catalog[m.pm.t];
        placed_units[m.pm.t] += if t.flexible && t.shape == DeskShape::Straight { t.seats_per_module(m.pm.k) } else { 1 };
        placed_seats_t[m.pm.t] += t.seats_per_module(m.pm.k);
    }
    let mut unplaced = 0u32;
    let mut requested = 0u32;
    let mut remaining = vec![0u32; nt];
    for ti in 0..nt {
        let t = &ctx.catalog[ti];
        let seats_per_unit = if t.flexible && t.shape == DeskShape::Straight { 1 } else { t.seats_per_module(t.per_side) };
        let missing_units = ctx.demand[ti].saturating_sub(placed_units[ti]);
        remaining[ti] = missing_units;
        if ctx.is_max[ti] {
            ev.max_seats += placed_seats_t[ti];
            continue;
        }
        requested += ctx.demand[ti] * seats_per_unit;
        unplaced += missing_units * seats_per_unit;
    }
    ev.deficit = deficit(ctx, &remaining);
    let pinned_seats: u32 = mods.iter().filter(|m| m.pinned).map(|m| m.seats.len() as u32).sum();
    ev.seats_requested = requested + pinned_seats;
    let valid_seats: Vec<usize> = (0..seats.len()).filter(|&i| ev.module_ok[seats[i].module]).collect();
    ev.seats_placed = valid_seats.len() as u32;
    ev.unplaced_seats = unplaced;

    // ------------------------------------------------ métricas blandas
    let vs = valid_seats.len().max(1) as f64;
    // orientaciones: direcciones distintas (paralelas = misma dirección)
    let mut fams: Vec<f64> = Vec::new();
    for (mi, m) in mods.iter().enumerate() {
        if !ev.module_ok[mi] {
            continue;
        }
        let a = norm_angle_180(m.angle);
        if !fams.iter().any(|&b| {
            let d = (a - b).abs();
            d < 1.0 || d > 179.0
        }) {
            fams.push(a);
        }
    }
    ev.orientations = fams.len() as u32;

    // luz natural, ventana lateral
    let mut daylight = 0usize;
    let mut window_bad = 0usize;
    let mut window_near = 0usize;
    let has_windows = !ctx.windows.is_empty();
    for &si in &valid_seats {
        let s = &seats[si];
        if has_windows {
            if let Some(i) = f.cell_idx(s.pos) {
                let dw = ctx.win_dist[i] as f64;
                if let Some(a4) = p.a4 {
                    if dw <= a4 {
                        daylight += 1;
                    } else {
                        ev.seat_flags[si].push("Far from windows".into());
                    }
                }
                if let Some((a1d, tol)) = p.a1 {
                    if dw <= a1d {
                        window_near += 1;
                        // ventana más cercana
                        let mut best = (f64::INFINITY, pt(1.0, 0.0));
                        for w in &ctx.windows {
                            let d = dist_point_segment(s.pos, w.a, w.b);
                            if d < best.0 {
                                best = (d, (w.b - w.a).norm());
                            }
                        }
                        let along = s.facing.dot(best.1).abs();
                        if along < tol.to_radians().cos() {
                            window_bad += 1;
                            ev.seat_flags[si].push("Window in front or behind".into());
                        }
                    }
                }
            }
        }
    }
    ev.daylight_frac = if p.a4.is_some() && has_windows { daylight as f64 / vs } else { 0.0 };
    ev.window_side_frac = if window_near > 0 { 1.0 - window_bad as f64 / window_near as f64 } else { 1.0 };

    // privacidad
    let main_mid: Option<Pt> = ctx.doors.iter().find(|d| d.door.kind == DoorKind::Main).map(|d| d.door.mid());
    let mut back_issues = 0usize;
    let mut vis_issues = 0usize;
    for &si in &valid_seats {
        let s = &seats[si];
        if let Some(mm) = main_mid {
            let to_door = mm - s.pos;
            let dd = to_door.len();
            let behind = s.facing.dot(to_door.norm()) < -0.5;
            if let Some(lim) = p.a2_door {
                if behind && dd <= lim {
                    back_issues += 1;
                    ev.seat_flags[si].push("Back to the entrance".into());
                }
            }
            if let Some(lim) = p.a3_entry {
                if behind && dd <= lim && dd > p.a2_door.unwrap_or(0.0) {
                    vis_issues += 1;
                    ev.seat_flags[si].push("Screen visible from the entrance".into());
                }
            }
        }
        if p.a2_corr || p.a3_corr {
            let behind_pt = s.pos - s.facing * (p.chair * 0.5 + 400.0);
            if f.cell_idx(behind_pt).map(|i| var.corridor[i]).unwrap_or(false) {
                if p.a2_corr {
                    back_issues += 1;
                }
                if p.a3_corr {
                    vis_issues += 1;
                }
                ev.seat_flags[si].push("Back to the main corridor".into());
            }
        }
    }
    ev.privacy_issues = (back_issues + vis_issues) as u32;

    // recorridos
    let tv: Vec<f64> = valid_seats.iter().map(|&i| travel[i]).filter(|t| t.is_finite()).collect();
    ev.avg_travel = if tv.is_empty() { 0.0 } else { tv.iter().sum::<f64>() / tv.len() as f64 };
    ev.max_travel = tv.iter().cloned().fold(0.0, f64::max);
    ev.seat_travel = travel;
    let compact = match main_mid {
        Some(mm) => valid_seats.iter().map(|&i| seats[i].pos.dist(mm)).sum::<f64>() / vs,
        None => 0.0,
    };

    // reserva
    ev.reserve_seats = dec.reserve_seats;

    // PMR
    let c8_hard = crate::defaults::RuleView::new(&ctx.config.rules).hard("C8");
    if let Some((pct, giro, _ruta)) = p.c8.filter(|_| full || c8_hard) {
        let pmr_seeds: Vec<usize> = reach.iter().copied().filter(|&i| (clear[i] as f64) >= giro * 0.5 - res).collect();
        let d5 = dial::dial(f, &pass_sq, &pmr_seeds, dial::from_mm(1500.0, res));
        let cnt = valid_seats.iter().filter(|&&i| dial::to_mm(near_min(f, &d5, seats[i].access), res) <= 1500.0 + res).count();
        ev.pmr = cnt as u32;
        let need = (pct / 100.0 * vs).ceil() as usize;
        if cnt < need {
            let hard = crate::defaults::RuleView::new(&ctx.config.rules).hard("C8");
            ev.violations.push(Violation { rule_id: "C8".into(), message: format!("Only {cnt} wheelchair-accessible seats ({need} required)."), at: None, hard });
            if hard {
                ev.hard += 1;
            }
        }
    }

    // ------------------------------------------------ reglas personalizadas (etapa de distribución)
    let custom_pen = super::custom::layout_penalty(ctx, &mods, &seats, &valid_seats, &ev.module_ok, var, &mut ev.seat_flags, &mut ev.violations, &mut ev.hard);

    // ------------------------------------------------ módulos fijados / editados a mano: superposiciones
    if mods.iter().any(|m| m.pinned) {
        let mut count = vec![0u8; n];
        for m in &mods {
            let mut tmp = vec![false; n];
            raster::fill_polygon(f, &mut tmp, &m.body, true);
            for i in 0..n {
                if tmp[i] {
                    count[i] = count[i].saturating_add(1);
                }
            }
        }
        for (mi, m) in mods.iter().enumerate() {
            if !m.pinned {
                continue;
            }
            let mut tmp = vec![false; n];
            raster::fill_polygon(f, &mut tmp, &m.body, true);
            for s in &seats[m.seats.clone()] {
                raster::fill_polygon(f, &mut tmp, &s.chair, true);
            }
            let cells = tmp.iter().filter(|&&b| b).count().max(1);
            let outside = (0..n).filter(|&i| tmp[i] && !ctx.free_base[i]).count();
            let overlap = (0..n).filter(|&i| tmp[i] && count[i] > 1).count();
            let _ = mi;
            if outside * 10 > cells {
                ev.hard += 1;
                ev.violations.push(Violation { rule_id: "O1".into(), message: "A desk is on a wall, column or obstacle.".into(), at: Some(m.center), hard: true });
            }
            if overlap * 20 > cells {
                ev.hard += 1;
                ev.violations.push(Violation { rule_id: "E2".into(), message: "Two desks overlap.".into(), at: Some(m.center), hard: true });
            }
        }
    }

    // ------------------------------------------------ violaciones globales
    for v in &var.violations {
        ev.violations.push(v.clone());
        if v.hard {
            ev.hard += 1;
        }
    }
    for v in &ctx.violations {
        ev.violations.push(v.clone());
        if v.hard {
            ev.hard += 1;
        }
    }
    if p.e7_hard && ev.orientations > 1 {
        ev.hard += ev.orientations - 1;
        ev.violations.push(Violation { rule_id: "E7".into(), message: format!("There are {} different row directions.", ev.orientations), at: None, hard: true });
    }
    let density_pen = match p.a5 {
        Some(m2) if ev.seats_placed > 0 => {
            let per = ctx.area_m2 / ev.seats_placed as f64;
            if per < m2 {
                let a5_hard = crate::defaults::RuleView::new(&ctx.config.rules).hard("A5");
                ev.violations.push(Violation { rule_id: "A5".into(), message: format!("Density {per:.1} m²/seat, below {m2:.1}."), at: None, hard: a5_hard });
                if a5_hard {
                    ev.hard += 1;
                }
                (m2 - per) / m2
            } else {
                0.0
            }
        }
        _ => 0.0,
    };

    // ------------------------------------------------ puntaje
    let w = weights;
    let mut soft = 0.0;
    // preferencia: que a cada puesto se llegue por pasillos cómodos (C2) sin mover sillas
    if p.c7_on {
        let uncomfy = valid_seats.iter().filter(|&&i| !seat_comfy[i]).count() as f64;
        soft += 2.0 * uncomfy / vs;
    }
    soft += w.order * (ev.orientations.max(1) - 1) as f64 * 0.8;
    soft += p.e7_w * (ev.orientations.max(1) - 1) as f64 * 2.0;
    if p.a4.is_some() && has_windows {
        soft += w.daylight * p.a4_w * (1.0 - ev.daylight_frac) * 2.0;
    }
    if p.a1.is_some() && has_windows {
        soft += w.window_side * p.a1_w * (1.0 - ev.window_side_frac) * 2.0;
    }
    soft += w.privacy * (p.a2_w * back_issues as f64 + p.a3_w * vis_issues as f64) / vs * 3.0;
    soft += w.travel * ev.avg_travel / 40_000.0 * 2.0;
    soft += w.compact * compact / 30_000.0 * 2.0;
    let target = p.a6.unwrap_or(10.0).max(1.0) / 100.0;
    let ratio = ev.reserve_seats as f64 / vs;
    soft += w.reserve * ((target - ratio).max(0.0) / target) * 1.5;
    soft += density_pen * 10.0;
    soft += custom_pen;
    // escritorios de un lado con la espalda contra un muro
    if w.wall > 0.0 {
        let (mut on, mut tot) = (0usize, 0usize);
        for (mi, m) in mods.iter().enumerate() {
            if m.pinned || !ev.module_ok.get(mi).copied().unwrap_or(false) {
                continue;
            }
            let t = &ctx.catalog[m.pm.t];
            if t.is_two_sided() {
                continue;
            }
            tot += 1;
            let u = Pt::from_angle(m.angle.to_radians());
            let v = u.perp();
            let back = if m.pm.seats_up { -1.0 } else { 1.0 };
            let off = m.pm.dep * 0.5 + t.clear_back.max(0.0) + 160.0;
            let hit = [-0.35, 0.0, 0.35].iter().filter(|&&a| {
                let q = m.center + u * (a * m.pm.len) + v * (back * off);
                // muro, oficina, columna o su franja de separación: cuenta como «apoyado»
                !ctx.f.cell_idx(q).map(|i| ctx.free[i] && !ctx.keep0[i]).unwrap_or(false)
            }).count();
            if hit >= 2 {
                on += 1;
            }
        }
        if tot > 0 {
            soft += w.wall * (1.0 - on as f64 / tot as f64) * 1.0;
        }
    }
    ev.soft = soft * 10.0;
    // tidiness, measured in seats: lone small desks and extra row directions
    if w.tidy > 0.0 {
        let ok: Vec<usize> = (0..mods.len()).filter(|&i| ev.module_ok.get(i).copied().unwrap_or(false) || mods[i].pinned).collect();
        let strays = ok.iter().filter(|&&i| !mods[i].pinned && mods[i].seats.len() <= 2 && !ok.iter().any(|&j| j != i && touching(&mods[i], &mods[j]))).count();
        ev.soft += w.tidy * W_UNPLACED * (strays as f64 + (ev.orientations.max(1) - 1) as f64);
    }
    // a igual cantidad de puestos, preferir menos módulos (islas grandes en vez de muchos sueltos)
    if ctx.is_max.iter().any(|&m| m) {
        ev.soft += mods.iter().filter(|m| !m.pinned).count() as f64 * 40.0;
    }
    ev.score = ev.deficit + W_HARD * ev.hard as f64 + ev.soft;
    (ev, mods, seats)
}
