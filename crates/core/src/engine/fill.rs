//! Relleno de huecos: después de armar las filas, busca dónde caben más escritorios
//! (de los tipos que todavía faltan o en «máximo posible»), en las dos orientaciones,
//! prefiriendo los que quedan con la espalda contra un muro o pegados a otros escritorios.
//! Cada propuesta se valida con la evaluación completa (accesos, pasillos, puertas):
//! solo se acepta si el puntaje mejora.

use super::ctx::{Ctx, RotGrid, F_HARD, F_KEEP};
use super::decode::{chair_clear, Decoded, PModule};
use super::eval::{evaluate, Eval, WModule, WSeat};
use crate::geom::*;
use crate::model::*;
use crate::raster::{self, Frame};

/// Tabla de sumas acumuladas para contar celdas bloqueadas en un rectángulo en O(1).
struct Integral {
    w: usize,
    h: usize,
    s: Vec<u32>,
}

impl Integral {
    fn new(m: &[bool], w: usize, h: usize) -> Self {
        let mut s = vec![0u32; (w + 1) * (h + 1)];
        for y in 0..h {
            let mut row = 0u32;
            for x in 0..w {
                row += m[y * w + x] as u32;
                s[(y + 1) * (w + 1) + x + 1] = s[y * (w + 1) + x + 1] + row;
            }
        }
        Integral { w, h, s }
    }
    /// Celdas bloqueadas en [x0,x1)×[y0,y1); lo que cae fuera de la rejilla cuenta como bloqueado.
    fn blocked(&self, x0: isize, y0: isize, x1: isize, y1: isize) -> u32 {
        if x1 <= x0 || y1 <= y0 {
            return 0;
        }
        let total = ((x1 - x0) * (y1 - y0)) as u32;
        let cx0 = x0.clamp(0, self.w as isize) as usize;
        let cy0 = y0.clamp(0, self.h as isize) as usize;
        let cx1 = x1.clamp(0, self.w as isize) as usize;
        let cy1 = y1.clamp(0, self.h as isize) as usize;
        if cx1 <= cx0 || cy1 <= cy0 {
            return total;
        }
        let inside = ((cx1 - cx0) * (cy1 - cy0)) as u32;
        let w1 = self.w + 1;
        let sum = self.s[cy1 * w1 + cx1] + self.s[cy0 * w1 + cx0] - self.s[cy0 * w1 + cx1] - self.s[cy1 * w1 + cx0];
        total - inside + sum
    }
}

/// Rectángulo local [a0,a1]×[b0,b1] en los ejes (u, v) de un módulo, en el mundo.
fn local_rect(c: Pt, u: Pt, v: Pt, a0: f64, a1: f64, b0: f64, b1: f64) -> Vec<Pt> {
    vec![c + u * a0 + v * b0, c + u * a1 + v * b0, c + u * a1 + v * b1, c + u * a0 + v * b1]
}

struct Masks {
    /// Para el cuerpo de un escritorio nuevo.
    body_test: Integral,
    /// Para la zona de sillas nueva (no se solapa con otras zonas de sillas).
    chair_test: Integral,
    /// Para la extensión de la zona de sillas: solo cuerpos.
    bodies: Integral,
    /// Para las separaciones atrás y a los lados: muros y cuerpos.
    hard_bodies: Integral,
    /// Franja trasera de los escritorios de un lado (para espalda con espalda).
    backface: Integral,
    /// Muros y despejes de obstáculos/puertas/pasillo (apoyo «contra algo fijo»).
    wallish: Integral,
}

fn build_masks(ctx: &Ctx, g: &RotGrid, mods: &[WModule], seats: &[WSeat], ok: &[bool]) -> Masks {
    let (w, h) = (g.w, g.h);
    let n = w * h;
    let fr = Frame { origin: g.origin, res: g.res, w, h };
    let to_g = |poly: &[Pt]| -> Vec<Pt> { poly.iter().map(|q| q.rot(-g.theta)).collect() };
    let mut hard = vec![false; n];
    let mut keep = vec![false; n];
    for i in 0..n {
        hard[i] = g.flags[i] & F_HARD != 0;
        keep[i] = g.flags[i] & F_KEEP != 0;
    }
    let mut body = vec![false; n];
    let mut chair = vec![false; n];
    let mut ext = vec![false; n];
    let mut backface = vec![false; n];
    let p = &ctx.p;
    for (mi, m) in mods.iter().enumerate() {
        if !ok[mi] && !m.pinned {
            continue;
        }
        raster::fill_polygon(&fr, &mut body, &to_g(&m.body), true);
        let t = &ctx.catalog[m.pm.t];
        let ang = m.angle.to_radians();
        let u = Pt::from_angle(ang);
        let v = u.perp();
        let (hl, hd) = (m.pm.len * 0.5, m.pm.dep * 0.5);
        if !t.is_two_sided() {
            let sgn = if m.pm.seats_up { -1.0 } else { 1.0 };
            let (b0, b1) = if sgn > 0.0 { (hd - g.res, hd) } else { (-hd, -hd + g.res) };
            raster::fill_polygon(&fr, &mut backface, &to_g(&local_rect(m.center, u, v, -hl, hl, b0, b1)), true);
        }
        // separaciones propias atrás / a los lados
        if t.clear_back > 0.0 && !t.is_two_sided() {
            let sgn = if m.pm.seats_up { -1.0 } else { 1.0 };
            let (b0, b1) = if sgn > 0.0 { (hd, hd + t.clear_back) } else { (-hd - t.clear_back, -hd) };
            raster::fill_polygon(&fr, &mut ext, &to_g(&local_rect(m.center, u, v, -hl, hl, b0, b1)), true);
        }
        if t.clear_left > 0.0 {
            raster::fill_polygon(&fr, &mut ext, &to_g(&local_rect(m.center, u, v, -hl - t.clear_left, -hl, -hd, hd)), true);
        }
        if t.clear_right > 0.0 {
            raster::fill_polygon(&fr, &mut ext, &to_g(&local_rect(m.center, u, v, hl, hl + t.clear_right, -hd, hd)), true);
        }
    }
    for s in seats {
        let m = &mods[s.module];
        if !ok[s.module] && !m.pinned {
            continue;
        }
        let t = &ctx.catalog[m.pm.t];
        let cc = chair_clear(ctx, t);
        let ce = cc.max(p.e2_se);
        let sw = if t.shape == DeskShape::L { 900.0 } else { t.seat_width };
        let edge = s.pos + s.facing * (p.chair * 0.5);
        let side = s.facing.perp();
        let out = -s.facing;
        let r = |d: f64| vec![edge - side * (sw * 0.5), edge + side * (sw * 0.5), edge + side * (sw * 0.5) + out * d, edge - side * (sw * 0.5) + out * d];
        // la franja de la silla de un candidato no puede pisar esta: entre las dos debe quedar
        // «silla con silla» (E2), ni más ni menos (antes se pedía el doble del espacio de silla)
        raster::fill_polygon(&fr, &mut chair, &to_g(&r((p.e2_ss - cc).clamp(g.res, cc.max(g.res)))), true);
        raster::fill_polygon(&fr, &mut ext, &to_g(&r(ce)), true);
    }
    let comb = |a: &[&Vec<bool>]| -> Vec<bool> { (0..n).map(|i| a.iter().any(|m| m[i])).collect() };
    Masks {
        body_test: Integral::new(&comb(&[&hard, &keep, &body, &chair, &ext]), w, h),
        chair_test: Integral::new(&comb(&[&hard, &keep, &body, &chair]), w, h),
        bodies: Integral::new(&body, w, h),
        hard_bodies: Integral::new(&comb(&[&hard, &body]), w, h),
        backface: Integral::new(&backface, w, h),
        wallish: Integral::new(&comb(&[&hard, &keep]), w, h),
    }
}

#[derive(Clone, Debug)]
struct Cand {
    t: usize,
    k: u32,
    /// Celda de la esquina del cuerpo en la rejilla.
    x: isize,
    y: isize,
    vertical: bool,
    /// Lado con sillas: +1 hacia +eje corto, -1 hacia -eje corto (sin uso en los de dos lados).
    side: i8,
    score: f64,
}

type R = (isize, isize, isize, isize);

/// Medidas en celdas de un tipo para el relleno.
struct TG {
    two: bool,
    ccl: isize,
    cel: isize,
    bk: isize,
    ll: isize,
    lr: isize,
}

fn tg(ctx: &Ctx, t: &DeskType, res: f64) -> TG {
    let cells = |mm: f64| (mm / res).ceil() as isize;
    let cc = chair_clear(ctx, t);
    let notch = if t.shape == DeskShape::L { (t.module_depth() - t.depth).max(0.0) } else { 0.0 };
    let ccl = cells((cc - notch).max(0.0));
    TG {
        two: t.is_two_sided(),
        ccl,
        cel: cells((cc.max(ctx.p.e2_se) - notch).max(0.0)) - ccl,
        bk: cells(t.clear_back.max(0.0)),
        ll: cells(t.clear_left.max(0.0)),
        lr: cells(t.clear_right.max(0.0)),
    }
}

fn dims(t: &DeskType, k: u32, res: f64, vertical: bool) -> (isize, isize) {
    let len = (t.module_length(k) / res).ceil() as isize;
    let dep = (t.module_depth() / res).ceil() as isize;
    if vertical { (dep, len) } else { (len, dep) }
}

/// Franja al costado largo (s=+1/−1 sobre el eje corto) entre d0 y d1 celdas del cuerpo.
fn strip(x: isize, y: isize, bw: isize, bh: isize, vertical: bool, s: i8, d0: isize, d1: isize) -> R {
    if vertical {
        if s > 0 { (x + bw + d0, y, x + bw + d1, y + bh) } else { (x - d1, y, x - d0, y + bh) }
    } else if s > 0 {
        (x, y + bh + d0, x + bw, y + bh + d1)
    } else {
        (x, y - d1, x + bw, y - d0)
    }
}

/// Franja en un extremo (s=+1/−1 sobre el eje largo).
fn endr(x: isize, y: isize, bw: isize, bh: isize, vertical: bool, s: i8, d0: isize, d1: isize) -> R {
    if vertical {
        if s > 0 { (x, y + bh + d0, x + bw, y + bh + d1) } else { (x, y - d1, x + bw, y - d0) }
    } else if s > 0 {
        (x + bw + d0, y, x + bw + d1, y + bh)
    } else {
        (x - d1, y, x - d0, y + bh)
    }
}

/// ¿Cabe el escritorio en (x, y) con estas máscaras? Si cabe, devuelve su puntaje.
#[allow(clippy::too_many_arguments)]
fn check(m: &Masks, t: &DeskType, g: &TG, k: u32, x: isize, y: isize, bw: isize, bh: isize, vertical: bool, side: i8, wall_w: f64, order: f64) -> Option<f64> {
    if m.body_test.blocked(x, y, x + bw, y + bh) != 0 {
        return None;
    }
    let free_in = |ig: &Integral, r: R| ig.blocked(r.0, r.1, r.2, r.3) == 0;
    let st = |s: i8, d0: isize, d1: isize| strip(x, y, bw, bh, vertical, s, d0, d1);
    let en = |s: i8, d0: isize, d1: isize| endr(x, y, bw, bh, vertical, s, d0, d1);
    let seated: &[i8] = if g.two { &[1, -1] } else if side > 0 { &[1] } else { &[-1] };
    for &s in seated {
        if !free_in(&m.chair_test, st(s, 0, g.ccl)) || (g.cel > 0 && !free_in(&m.bodies, st(s, g.ccl, g.ccl + g.cel))) {
            return None;
        }
    }
    if !g.two && g.bk > 0 && !free_in(&m.hard_bodies, st(-side, 0, g.bk)) {
        return None;
    }
    if g.ll > 0 && !free_in(&m.hard_bodies, en(-1, 0, g.ll)) {
        return None;
    }
    if g.lr > 0 && !free_in(&m.hard_bodies, en(1, 0, g.lr)) {
        return None;
    }
    let frac = |ig: &Integral, r: R| -> f64 {
        let a = ((r.2 - r.0) * (r.3 - r.1)).max(1) as f64;
        ig.blocked(r.0, r.1, r.2, r.3) as f64 / a
    };
    let (back_wall, back_desk) = if !g.two {
        let b = st(-side, g.bk, g.bk + 1);
        (frac(&m.wallish, b), frac(&m.backface, b))
    } else {
        (0.0, 0.0)
    };
    let end_wall = frac(&m.wallish, en(-1, g.ll, g.ll + 1)).max(frac(&m.wallish, en(1, g.lr, g.lr + 1)));
    let end_any = frac(&m.hard_bodies, en(-1, g.ll, g.ll + 1)) + frac(&m.hard_bodies, en(1, g.lr, g.lr + 1));
    // orden: con «orden» alto solo se aceptan escritorios apoyados (espalda o extremo contra un
    // muro, o espalda con espalda); con orden bajo, cualquier hueco sirve
    let anchored = if g.two { end_wall >= 0.5 } else { back_wall >= 0.6 || (order < 6.0 && back_desk >= 0.6) };
    if order >= 2.0 && !anchored {
        return None;
    }
    Some(wall_w * 0.6 * back_wall + 2.0 * back_desk + 1.5 * end_wall + 0.5 * end_any + 0.3 * t.seats_per_module(k) as f64)
}

/// Candidatos válidos en la rejilla `g`, ordenados por puntaje (mejor primero).
fn candidates(ctx: &Ctx, g: &RotGrid, m: &Masks, remaining: &[u32], wall_w: f64, order: f64, limit: usize) -> Vec<Cand> {
    let res = g.res;
    let step = ((100.0 / res).round() as isize).max(1);
    let mut out: Vec<Cand> = Vec::new();
    for (ti, t) in ctx.catalog.iter().enumerate() {
        if remaining[ti] == 0 {
            continue;
        }
        let (kmin, kmax) = t.per_side_range();
        let flex = t.flexible && t.shape == DeskShape::Straight;
        let geo = tg(ctx, t, res);
        let sides = if geo.two { 2 } else { 1 };
        let mut ks: Vec<u32> = (kmin..=kmax).rev().collect();
        if flex {
            ks.retain(|&k| k * sides <= remaining[ti].max(kmin * sides));
        }
        for &k in &ks {
            for vertical in [false, true] {
                let (bw, bh) = dims(t, k, res, vertical);
                let side_opts: &[i8] = if geo.two { &[1] } else { &[1, -1] };
                for &side in side_opts {
                    let mut y = 0isize;
                    while y + bh <= g.h as isize {
                        let mut x = 0isize;
                        while x + bw <= g.w as isize {
                            if let Some(score) = check(m, t, &geo, k, x, y, bw, bh, vertical, side, wall_w, order) {
                                out.push(Cand { t: ti, k, x, y, vertical, side, score });
                            }
                            x += step;
                        }
                        y += step;
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    // no repetir casi la misma posición
    let mut picked: Vec<Cand> = Vec::new();
    for c in out {
        if picked.iter().rev().take(400).any(|q| q.t == c.t && q.vertical == c.vertical && (q.x - c.x).abs() < 3 && (q.y - c.y).abs() < 3) {
            continue;
        }
        picked.push(c);
        if picked.len() >= limit {
            break;
        }
    }
    picked
}

/// Caja que ocupa un candidato con sus zonas de silla y separaciones (para no proponer dos que choquen).
fn envelope(ctx: &Ctx, c: &Cand, res: f64) -> R {
    let t = &ctx.catalog[c.t];
    let g = tg(ctx, t, res);
    let (bw, bh) = dims(t, c.k, res, c.vertical);
    let ch = g.ccl + g.cel;
    let (lo, hi) = if g.two { (ch, ch) } else if c.side > 0 { (g.bk, ch) } else { (ch, g.bk) };
    if c.vertical {
        (c.x - lo, c.y - g.ll, c.x + bw + hi, c.y + bh + g.lr)
    } else {
        (c.x - g.ll, c.y - lo, c.x + bw + g.lr, c.y + bh + hi)
    }
}

fn overlap(a: R, b: R) -> bool {
    a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3
}

/// Pasa un candidato (en la rejilla `gi`) a un módulo decodificado.
fn to_pmodule(ctx: &Ctx, variant: usize, gi: usize, c: &Cand) -> Option<PModule> {
    let var = &ctx.variants[variant];
    let g = &var.grids[gi];
    let t = &ctx.catalog[c.t];
    let len = t.module_length(c.k);
    let dep = t.module_depth();
    let res = g.res;
    if !c.vertical {
        return Some(PModule { t: c.t, k: c.k, grid: gi, x0: c.x as f64 * res, y0: c.y as f64 * res, len, dep, seats_up: c.side > 0, zone: 0 });
    }
    // vertical en `gi` = horizontal en la rejilla girada 90°
    let target = g.theta + std::f64::consts::FRAC_PI_2;
    let gj = var.grids.iter().position(|q| {
        let d = (q.theta - target).rem_euclid(std::f64::consts::PI);
        d < 1e-6 || (std::f64::consts::PI - d) < 1e-6
    })?;
    let g2 = &var.grids[gj];
    let c_rot = pt(g.origin.x + c.x as f64 * res + dep * 0.5, g.origin.y + c.y as f64 * res + len * 0.5);
    let world = c_rot.rot(g.theta);
    // ¿g2 apunta a θ+90° o a θ−90°?
    let up2 = pt(0.0, 1.0).rot(g2.theta);
    let side_world = pt(c.side as f64, 0.0).rot(g.theta);
    let cj = world.rot(-g2.theta) - g2.origin;
    Some(PModule { t: c.t, k: c.k, grid: gj, x0: cj.x - len * 0.5, y0: cj.y - dep * 0.5, len, dep, seats_up: up2.dot(side_world) > 0.0, zone: 0 })
}

fn units(ctx: &Ctx, pm: &PModule) -> u32 {
    let t = &ctx.catalog[pm.t];
    if t.flexible && t.shape == DeskShape::Straight {
        t.seats_per_module(pm.k)
    } else {
        1
    }
}

/// Quita los módulos inválidos y rellena huecos. Devuelve la evaluación final.
pub fn fill_gaps(ctx: &Ctx, dec: &mut Decoded, variant: usize, weights: &Weights, deadline: std::time::Instant) -> Eval {
    use std::time::Instant;
    let (mut ev, mut mods, mut seats) = evaluate(ctx, dec, variant, weights, false);
    // descartar inválidos (se devuelven sus unidades)
    let np = ctx.pinned.len();
    if ev.module_ok.iter().skip(np).any(|&b| !b) {
        let mut keep = Vec::new();
        for (i, pm) in dec.modules.iter().enumerate() {
            if ev.module_ok[np + i] {
                keep.push(pm.clone());
            } else if dec.remaining[pm.t] < super::ctx::MAX_UNITS {
                dec.remaining[pm.t] += units(ctx, pm);
            }
        }
        dec.modules = keep;
        let r = evaluate(ctx, dec, variant, weights, false);
        ev = r.0;
        mods = r.1;
        seats = r.2;
    }
    if ctx.variants[variant].grids.is_empty() {
        return ev;
    }
    let gi = 0usize;
    let g = &ctx.variants[variant].grids[gi];
    let res = g.res;
    let mut masks = build_masks(ctx, g, &mods, &seats, &ev.module_ok);
    let mut list = candidates(ctx, g, &masks, &dec.remaining, weights.wall, weights.order, 4000);
    let mut since_scan = 0;
    // intenta agregar `batch` candidatos a la vez; devuelve true si se aceptó
    let try_add = |dec: &mut Decoded, batch: &[Cand], ev: &mut Eval, mods: &mut Vec<WModule>, seats: &mut Vec<WSeat>| -> bool {
        let n0 = dec.modules.len();
        let rem0 = dec.remaining.clone();
        for c in batch {
            let Some(pm) = to_pmodule(ctx, variant, gi, c) else { continue };
            let u = units(ctx, &pm);
            dec.remaining[c.t] = dec.remaining[c.t].saturating_sub(u);
            dec.modules.push(pm);
        }
        let (ev2, mods2, seats2) = evaluate(ctx, dec, variant, weights, false);
        if ev2.score < ev.score - 1e-6 && ev2.module_ok.iter().all(|&b| b) {
            *ev = ev2;
            *mods = mods2;
            *seats = seats2;
            return true;
        }
        dec.modules.truncate(n0);
        dec.remaining = rem0;
        false
    };
    while !list.is_empty() && Instant::now() < deadline {
        // tanda de candidatos válidos que no se pisan entre sí
        let mut batch: Vec<Cand> = Vec::new();
        let mut envs: Vec<R> = Vec::new();
        let mut used_units = vec![0u32; ctx.catalog.len()];
        let mut i = 0;
        while i < list.len() && batch.len() < 6 {
            let c = &list[i];
            let t = &ctx.catalog[c.t];
            let geo = tg(ctx, t, res);
            let (bw, bh) = dims(t, c.k, res, c.vertical);
            let u = if t.flexible && t.shape == DeskShape::Straight { t.seats_per_module(c.k) } else { 1 };
            let ok = dec.remaining[c.t] >= used_units[c.t] + u
                && check(&masks, t, &geo, c.k, c.x, c.y, bw, bh, c.vertical, c.side, weights.wall, weights.order).is_some();
            if !ok {
                list.remove(i);
                continue;
            }
            let e = envelope(ctx, c, res);
            if envs.iter().any(|&o| overlap(o, e)) {
                i += 1;
                continue;
            }
            used_units[c.t] += u;
            envs.push(e);
            batch.push(c.clone());
            i += 1;
        }
        if batch.is_empty() {
            break;
        }
        let mut accepted = batch.len() > 1 && try_add(dec, &batch, &mut ev, &mut mods, &mut seats);
        if !accepted {
            // uno por uno: el primero que mejore
            for c in &batch {
                if Instant::now() >= deadline {
                    break;
                }
                if try_add(dec, std::slice::from_ref(c), &mut ev, &mut mods, &mut seats) {
                    accepted = true;
                    break;
                }
                // no sirve: sacarlo de la lista
                if let Some(j) = list.iter().position(|q| q.t == c.t && q.k == c.k && q.x == c.x && q.y == c.y && q.vertical == c.vertical && q.side == c.side) {
                    list.remove(j);
                }
            }
        }
        if accepted {
            masks = build_masks(ctx, g, &mods, &seats, &ev.module_ok);
            since_scan += 1;
            if since_scan >= 12 {
                // los nuevos escritorios crean nuevos apoyos: volver a puntuar
                list = candidates(ctx, g, &masks, &dec.remaining, weights.wall, weights.order, 4000);
                since_scan = 0;
            }
        }
    }
    ev
}
