//! Genoma → filas de escritorios. Cada zona se llena por filas paralelas a un ángulo,
//! desde un lado, con un desfase; dentro de cada fila los módulos se agrupan en bloques
//! de hasta E3 puestos por lado separados por un paso transversal.

use super::ctx::{Ctx, RotGrid};
use crate::model::{DeskShape, DeskType};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ZoneGene {
    pub theta: u8,
    pub off_y: f32,
    pub off_x: f32,
    pub from_top: bool,
    /// 0 = rejilla alineada, 1 = izquierda, 2 = derecha, 3 = centro
    pub align: u8,
    /// Escritorios de un lado: emparejar por la espalda.
    pub pair: bool,
    /// Escritorios de un lado: la primera fila tiene las sillas hacia el lado de inicio.
    pub first_near: bool,
    /// Orden de preferencia de tipos.
    pub pref: Vec<u8>,
    /// Si es false, la zona no se usa (deja espacio libre).
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VarGene {
    pub zones: Vec<ZoneGene>,
    pub order: Vec<u16>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Genome {
    pub variant: usize,
    pub vars: Vec<VarGene>,
}

/// Módulo colocado en el marco rotado de una rejilla.
#[derive(Clone, Debug)]
pub struct PModule {
    pub t: usize,
    pub k: u32,
    pub grid: usize,
    pub x0: f64,
    pub y0: f64,
    pub len: f64,
    pub dep: f64,
    /// Para tipos de un lado (y L): sillas del lado +y.
    pub seats_up: bool,
    pub zone: u16,
}

pub struct Decoded {
    pub modules: Vec<PModule>,
    /// Unidades sin ubicar por tipo.
    pub remaining: Vec<u32>,
    /// Puestos extra que todavía cabrían (reserva estimada).
    pub reserve_seats: u32,
}

pub fn random_genome(ctx: &Ctx, rng: &mut Rng) -> Genome {
    let nt = ctx.catalog.len();
    let vars = ctx
        .variants
        .iter()
        .map(|v| {
            let nz = v.zones.len();
            let mut order: Vec<u16> = (1..nz as u16).collect();
            // por defecto: zonas más grandes primero
            order.sort_by_key(|&z| std::cmp::Reverse(v.zones[z as usize].cells));
            let zones = (0..nz)
                .map(|_| {
                    let mut pref: Vec<u8> = (0..nt as u8).collect();
                    // tipos con demanda primero, más profundos primero (las islas aprovechan mejor el centro)
                    pref.sort_by(|&a, &b| {
                        let (ta, tb) = (&ctx.catalog[a as usize], &ctx.catalog[b as usize]);
                        (ctx.demand[b as usize] > 0)
                            .cmp(&(ctx.demand[a as usize] > 0))
                            .then(tb.module_depth().partial_cmp(&ta.module_depth()).unwrap())
                    });
                    ZoneGene {
                        theta: 0,
                        off_y: rng.f32() * 0.3,
                        off_x: rng.f32(),
                        from_top: rng.f32() < 0.5,
                        align: 0,
                        pair: true,
                        first_near: rng.f32() < 0.5,
                        pref,
                        enabled: true,
                    }
                })
                .collect();
            VarGene { zones, order }
        })
        .collect();
    Genome { variant: 0, vars }
}

#[derive(Clone)]
pub struct Rng(pub u64);
impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    pub fn f32(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
    pub fn f64(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
}

/// Geometría de fila para un tipo.
struct RowSpec {
    d_cells: usize,
    chair_cells: usize,
    wall_cells: usize,
    sep_cells: usize,
}

fn row_spec(ctx: &Ctx, t: &DeskType) -> RowSpec {
    let r = ctx.res;
    let dep = t.module_depth();
    let notch = if t.shape == DeskShape::L { (dep - t.depth).max(0.0) } else { 0.0 };
    RowSpec {
        d_cells: (dep / r).ceil() as usize,
        chair_cells: (((ctx.p.chair - notch).max(0.0)) / r).ceil() as usize,
        wall_cells: (((ctx.p.e1_wall - notch).max(0.0)) / r).ceil() as usize,
        sep_cells: (ctx.p.e8_sep / r).ceil() as usize,
    }
}

/// Columnas aptas para una fila con cuerpo en [ya, yb) (celdas). O(1) por columna gracias a las corridas precalculadas.
#[allow(clippy::too_many_arguments)]
fn column_ok(g: &RotGrid, z: u16, x0: usize, x1: usize, ya: isize, yb: isize, below: bool, above: bool, spec: &RowSpec, touch: bool, out: &mut Vec<bool>) {
    out.clear();
    let h = g.h as isize;
    let w = g.w;
    let d = (yb - ya) as u16;
    let ch = spec.chair_cells as isize;
    let wl = spec.wall_cells as isize;
    let sp = spec.sep_cells as isize;
    // run "ok" (libre y de la zona z) de al menos `len` celdas empezando en y
    let ok_run = |x: usize, y: isize, len: isize| -> bool {
        if len <= 0 {
            return true;
        }
        if y < 0 || y + len > h {
            return false;
        }
        let i = y as usize * w + x;
        g.zone[i] == z && g.up_ok[i] as isize >= len
    };
    let nohard_run = |x: usize, y: isize, len: isize| -> bool {
        if len <= 0 {
            return true;
        }
        if y < 0 || y + len > h {
            return false;
        }
        g.up_nohard[y as usize * w + x] as isize >= len
    };
    for x in x0..=x1 {
        let mut ok = if below && ch > 0 { ok_run(x, ya - ch, ch + d as isize) } else { ok_run(x, ya, d as isize) };
        if ok && above && ch > 0 {
            ok = ok_run(x, yb, ch);
        }
        if ok {
            if below {
                ok = nohard_run(x, ya - wl, wl - ch);
            } else if !touch {
                ok = nohard_run(x, ya - sp, sp);
            }
        }
        if ok {
            if above {
                ok = nohard_run(x, yb + ch, wl - ch);
            } else if !touch {
                ok = nohard_run(x, yb, sp);
            }
        }
        out.push(ok);
    }
}

/// Coloca módulos en los tramos libres de una fila. Devuelve (x_mm, k) y unidades consumidas.
#[allow(clippy::too_many_arguments)]
fn fill_runs(ctx: &Ctx, t: &DeskType, runs: &[(f64, f64)], align: u8, anchor: f64, mut rem: u32, phantom: bool, out: &mut Vec<(f64, u32)>) -> u32 {
    out.clear();
    let p = &ctx.p;
    let cut = p.e3_cut;
    let sides = if t.shape == DeskShape::L { 1 } else { t.sides.max(1) as u32 };
    let eps = 1e-6;
    let mut used = 0u32;
    // tamaños de módulos dentro de un bloque de largo `len`
    let block_modules = |len: f64, rem: u32| -> Vec<u32> {
        let mut v = Vec::new();
        if t.flexible && t.shape == DeskShape::Straight {
            let (mn, mx) = t.per_side_range();
            let mut seats = ((len + eps) / t.seat_width).floor() as u32;
            seats = seats.min(p.e3_max.max(1));
            // limitar por demanda (en puestos)
            if !phantom {
                let need_per_side = rem.div_ceil(sides);
                seats = seats.min(need_per_side);
            }
            if seats == 0 {
                return v;
            }
            let q = seats.div_ceil(mx);
            let base = seats / q;
            let extra = seats % q;
            for i in 0..q {
                let k = base + if i < extra { 1 } else { 0 };
                if k >= mn || (!phantom && k * sides >= rem.min(mn * sides)) {
                    v.push(k);
                }
            }
        } else {
            let k = t.per_side.max(1);
            let l = t.module_length(k);
            let per_block = if t.shape == DeskShape::L { p.e3_max.max(1) } else { (p.e3_max / k).max(1) };
            let mut n = ((len + eps) / l).floor() as u32;
            n = n.min(per_block);
            if !phantom {
                n = n.min(rem);
            }
            for _ in 0..n {
                v.push(k);
            }
        }
        v
    };
    let units = |k: u32| -> u32 {
        if t.flexible && t.shape == DeskShape::Straight {
            k * sides
        } else {
            1
        }
    };
    let mod_len = |k: u32| t.module_length(k);
    let block_len = if t.flexible && t.shape == DeskShape::Straight {
        t.seat_width * p.e3_max.max(1).min(10_000) as f64
    } else {
        let k = t.per_side.max(1);
        let per_block = if t.shape == DeskShape::L { p.e3_max.max(1) } else { (p.e3_max / k).max(1) };
        t.module_length(k) * per_block as f64
    };
    let block_len = block_len.min(1e7);
    let period = block_len + cut;

    for &(ra, rb) in runs {
        if !phantom && rem == 0 {
            break;
        }
        if align == 0 {
            // bloques anclados a la rejilla de la zona
            let j0 = ((ra - anchor) / period).floor() as i64;
            let mut j = j0;
            loop {
                let s = anchor + j as f64 * period;
                if s >= rb {
                    break;
                }
                let a = s.max(ra);
                let b = (s + block_len).min(rb);
                if b - a > eps {
                    let mods = block_modules(b - a, rem);
                    let mut x = a;
                    for k in mods {
                        let l = mod_len(k);
                        if x + l > b + eps {
                            break;
                        }
                        let u = units(k);
                        if !phantom && u > rem && rem > 0 && !t.flexible {
                            break;
                        }
                        out.push((x, k));
                        x += l;
                        used += u;
                        rem = rem.saturating_sub(u);
                        if !phantom && rem == 0 {
                            break;
                        }
                    }
                }
                if !phantom && rem == 0 {
                    break;
                }
                j += 1;
            }
        } else {
            // empaque desde un extremo
            let mut placed: Vec<(f64, u32)> = Vec::new();
            let mut x = 0.0;
            let len = rb - ra;
            loop {
                if !phantom && rem == 0 {
                    break;
                }
                let avail = (len - x).min(block_len);
                if avail <= eps {
                    break;
                }
                let mods = block_modules(avail, rem);
                if mods.is_empty() {
                    break;
                }
                let mut any = false;
                for k in mods {
                    let l = mod_len(k);
                    if x + l > len + eps {
                        break;
                    }
                    placed.push((x, k));
                    x += l;
                    any = true;
                    let u = units(k);
                    used += u;
                    rem = rem.saturating_sub(u);
                    if !phantom && rem == 0 {
                        break;
                    }
                }
                if !any {
                    break;
                }
                x += cut;
            }
            let total = placed.last().map(|&(x, k)| x + mod_len(k)).unwrap_or(0.0);
            let shift = match align {
                2 => len - total,
                3 => (len - total) * 0.5,
                _ => 0.0,
            };
            for (x, k) in placed {
                out.push((ra + x + shift, k));
            }
        }
    }
    used
}

fn row_gap(ctx: &Ctx, prev_far_seated: bool, near_seated: bool) -> f64 {
    let p = &ctx.p;
    match (prev_far_seated, near_seated) {
        (true, true) => p.e2_ss,
        (true, false) | (false, true) => p.e2_se,
        (false, false) => {
            if p.e2_backs {
                0.0
            } else {
                p.e2_se
            }
        }
    }
}

/// Decodifica el genoma. `with_reserve` también estima cuántos puestos extra caben.
pub fn decode(ctx: &Ctx, g: &Genome, with_reserve: bool) -> Decoded {
    let var = &ctx.variants[g.variant.min(ctx.variants.len() - 1)];
    let vg = &g.vars[g.variant.min(g.vars.len() - 1)];
    let mut rem = ctx.demand.clone();
    let mut modules = Vec::new();
    let mut reserve_seats = 0u32;
    let mut colbuf = Vec::new();
    let mut runs: Vec<(f64, f64)> = Vec::new();
    let mut fill = Vec::new();
    let res = ctx.res;
    let touch = ctx.p.e8_touch;
    let specs: Vec<RowSpec> = ctx.catalog.iter().map(|t| row_spec(ctx, t)).collect();
    let look = ((600.0 / res).ceil() as isize).max(1);

    for &z in &vg.order {
        let zg = &vg.zones[z as usize];
        if !zg.enabled {
            continue;
        }
        let all_done = rem.iter().all(|&r| r == 0);
        if all_done && !with_reserve {
            break;
        }
        let gi = (zg.theta as usize).min(var.grids.len() - 1);
        let grid = &var.grids[gi];
        let (bx0, by0, bx1, by1) = grid.zone_bbox[z as usize];
        if bx0 == usize::MAX {
            continue;
        }
        let (by0, by1) = (by0 as isize, by1 as isize + 1);
        let anchor = bx0 as f64 * res + zg.off_x as f64 * 4000.0;
        // estado de filas
        let mut prev_far: Option<(isize, bool)> = None; // (borde lejano en celdas, ¿sillas en el lado lejano?)
        let mut pos: isize = 0; // distancia en celdas desde el lado de inicio
        let span = by1 - by0;
        pos += (zg.off_y as f64 * 12.0) as isize;
        let mut first_one_sided = true;
        while pos < span {
            // elegir tipo y posición
            let phantom_mode = rem.iter().all(|&r| r == 0);
            if phantom_mode && !with_reserve {
                break;
            }
            let mut best: Option<(isize, usize, bool, Vec<(f64, u32)>, u32)> = None;
            for &ti in &zg.pref {
                let ti = ti as usize;
                if ti >= ctx.catalog.len() {
                    continue;
                }
                let t = &ctx.catalog[ti];
                if !phantom_mode && rem[ti] == 0 {
                    continue;
                }
                if phantom_mode && ctx.demand[ti] == 0 {
                    continue;
                }
                let spec = &specs[ti];
                let two = t.is_two_sided();
                // lado de las sillas (relativo al inicio)
                let near_seated = if two {
                    true
                } else if let Some((_, far_seated)) = prev_far {
                    if zg.pair { far_seated } else { zg.first_near }
                } else {
                    zg.first_near
                };
                let far_seated = two || !near_seated;
                let near_seated_eff = two || near_seated;
                let gap = match prev_far {
                    Some((_, pf)) => (row_gap(ctx, pf, near_seated_eff) / res).ceil() as isize,
                    None => 0,
                };
                let start = match prev_far {
                    Some((edge, _)) => pos.max(edge + gap),
                    None => pos,
                };
                let mut local_best: Option<(isize, Vec<(f64, u32)>, u32)> = None;
                for dy in 0..=look {
                    let p0 = start + dy;
                    if p0 + spec.d_cells as isize > span {
                        break;
                    }
                    // celdas reales
                    let (ya, yb, below, above) = if zg.from_top {
                        let yb = by1 - p0;
                        (yb - spec.d_cells as isize, yb, far_seated, near_seated_eff)
                    } else {
                        let ya = by0 + p0;
                        (ya, ya + spec.d_cells as isize, near_seated_eff, far_seated)
                    };
                    column_ok(grid, z, bx0, bx1, ya, yb, below, above, spec, touch, &mut colbuf);
                    runs.clear();
                    let mut x = 0;
                    while x < colbuf.len() {
                        if colbuf[x] {
                            let s = x;
                            while x < colbuf.len() && colbuf[x] {
                                x += 1;
                            }
                            runs.push(((bx0 + s) as f64 * res, (bx0 + x) as f64 * res));
                        } else {
                            x += 1;
                        }
                    }
                    let units = fill_runs(ctx, t, &runs, zg.align, anchor, if phantom_mode { u32::MAX } else { rem[ti] }, phantom_mode, &mut fill);
                    if units > 0 && local_best.as_ref().map(|b| units > b.2).unwrap_or(true) {
                        local_best = Some((p0, fill.clone(), units));
                    }
                }
                if let Some((p0, f, u)) = local_best {
                    best = Some((p0, ti, near_seated, f, u));
                    break;
                }
            }
            let Some((p0, ti, near_seated, fillv, units)) = best else {
                // nada cabe desde aquí: avanzar
                pos = match prev_far {
                    Some((edge, _)) => pos.max(edge) + look + 1,
                    None => pos + look + 1,
                };
                if prev_far.is_some() && pos > span {
                    break;
                }
                continue;
            };
            let t = &ctx.catalog[ti];
            let spec = &specs[ti];
            let two = t.is_two_sided();
            let far_seated = two || !near_seated;
            let near_eff = two || near_seated;
            let (ya, _yb, above_seated) = if zg.from_top {
                let yb = by1 - p0;
                (yb - spec.d_cells as isize, yb, near_eff)
            } else {
                (by0 + p0, by0 + p0 + spec.d_cells as isize, far_seated)
            };
            let dep = t.module_depth();
            // alinear el cuerpo con el borde de inicio de la fila
            let y0_mm = if zg.from_top { (ya + spec.d_cells as isize) as f64 * res - dep } else { ya as f64 * res };
            if phantom_mode {
                for &(_, k) in &fillv {
                    reserve_seats += t.seats_per_module(k);
                }
            } else {
                let used: Vec<(f64, f64)> = fillv.iter().map(|&(x, k)| (x, x + t.module_length(k))).collect();
                for (x, k) in fillv {
                    modules.push(PModule { t: ti, k, grid: gi, x0: x, y0: y0_mm, len: t.module_length(k), dep, seats_up: above_seated, zone: z });
                }
                rem[ti] = rem[ti].saturating_sub(units);
                // E5 apagada: completar la fila con otros tipos del mismo fondo y lados
                if !ctx.p.e5 && t.shape == DeskShape::Straight {
                    let (below, above) = if zg.from_top { (far_seated, near_eff) } else { (near_eff, far_seated) };
                    column_ok(grid, z, bx0, bx1, ya, ya + spec.d_cells as isize, below, above, spec, touch, &mut colbuf);
                    let mut free_runs: Vec<(f64, f64)> = Vec::new();
                    let mut x = 0;
                    while x < colbuf.len() {
                        if colbuf[x] {
                            let s0 = x;
                            while x < colbuf.len() && colbuf[x] {
                                x += 1;
                            }
                            let (mut a, b) = ((bx0 + s0) as f64 * res, (bx0 + x) as f64 * res);
                            // restar lo ya ocupado (con el corte transversal entre bloques)
                            let mut segs = Vec::new();
                            let mut occ: Vec<(f64, f64)> = used.iter().copied().filter(|&(u0, u1)| u1 > a && u0 < b).collect();
                            occ.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap());
                            for (u0, u1) in occ {
                                if u0 - ctx.p.e3_cut > a {
                                    segs.push((a, u0 - ctx.p.e3_cut));
                                }
                                a = a.max(u1 + ctx.p.e3_cut);
                            }
                            if b > a {
                                segs.push((a, b));
                            }
                            free_runs.extend(segs);
                        } else {
                            x += 1;
                        }
                    }
                    for &oi in &zg.pref {
                        let oi = oi as usize;
                        if oi == ti || oi >= ctx.catalog.len() || rem[oi] == 0 || free_runs.is_empty() {
                            continue;
                        }
                        let o = &ctx.catalog[oi];
                        if o.shape != DeskShape::Straight || o.sides != t.sides || (o.module_depth() - dep).abs() > 1.0 {
                            continue;
                        }
                        let mut extra = Vec::new();
                        let u = fill_runs(ctx, o, &free_runs, zg.align, anchor, rem[oi], false, &mut extra);
                        if u == 0 {
                            continue;
                        }
                        for &(x, k) in &extra {
                            modules.push(PModule { t: oi, k, grid: gi, x0: x, y0: y0_mm, len: o.module_length(k), dep, seats_up: above_seated, zone: z });
                            // ya no disponible
                            let (e0, e1) = (x - ctx.p.e3_cut, x + o.module_length(k) + ctx.p.e3_cut);
                            let mut nr = Vec::new();
                            for &(r0, r1) in &free_runs {
                                if e1 <= r0 || e0 >= r1 {
                                    nr.push((r0, r1));
                                } else {
                                    if e0 > r0 {
                                        nr.push((r0, e0));
                                    }
                                    if e1 < r1 {
                                        nr.push((e1, r1));
                                    }
                                }
                            }
                            free_runs = nr;
                        }
                        rem[oi] = rem[oi].saturating_sub(u);
                    }
                }
            }
            if !two {
                first_one_sided = false;
            }
            prev_far = Some((p0 + spec.d_cells as isize, far_seated));
            pos = p0 + spec.d_cells as isize;
        }
        let _ = first_one_sided;
    }
    Decoded { modules, remaining: rem, reserve_seats }
}
