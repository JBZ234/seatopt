//! Circulation paths, computed on the empty floor (before desks):
//! - primary: from the main entrance to every office door and exit;
//! - secondary: between office doors (each office to its two nearest ones).
//! Routes run along the middle of the free space (Dijkstra with a cost that grows near walls)
//! and are drawn with straight horizontal/vertical runs, leaving and reaching each door square.

use super::ctx::{Ctx, DoorInfo};
use super::search::{axis_of, bundle_within, orthogonalize, slide_into_door, taut};
use crate::geom::Pt;
use crate::model::*;
use crate::raster::{self, Frame};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};

struct Field {
    pass: Vec<bool>,
    comfy: Vec<bool>,
    clear: Vec<f32>,
}

fn field(ctx: &Ctx) -> Field {
    let f = &ctx.f;
    let n = f.len();
    let blocked: Vec<bool> = ctx.free.iter().map(|b| !b).collect();
    let clear = raster::edt(f, &blocked);
    let pass: Vec<bool> = (0..n).map(|i| ctx.free[i] && ((clear[i] as f64) >= 280.0 || ctx.door_approach[i])).collect();
    let comfy: Vec<bool> = (0..n).map(|i| pass[i] && ((clear[i] as f64) >= 380.0 || ctx.door_approach[i])).collect();
    Field { pass, comfy, clear }
}

/// Shortest routes from `seeds`. The cost favors running along walls and obstacles with the path
/// band just touching them (clearance ≈ half the width), like a real corridor, so the open floor
/// is not cut in two; narrow spots are allowed only where the band still fits or next to doors.
fn dijkstra(f: &Frame, fd: &Field, seeds: &[usize], half: f64) -> (Vec<f64>, Vec<u32>) {
    let n = f.len();
    let cost = |i: usize| -> f64 { 1.0 + 2.0 * (((fd.clear[i] as f64) - half).abs() / 1500.0).min(1.0) };
    let mut dist = vec![f64::INFINITY; n];
    let mut par = vec![u32::MAX; n];
    let mut pq: BinaryHeap<Reverse<(u64, usize)>> = BinaryHeap::new();
    for &s in seeds {
        if s < n && fd.pass[s] {
            dist[s] = 0.0;
            pq.push(Reverse((0, s)));
        }
    }
    let (w, h) = (f.w as isize, f.h as isize);
    while let Some(Reverse((dk, i))) = pq.pop() {
        let d = dk as f64 / 1000.0;
        if d > dist[i] + 1e-9 {
            continue;
        }
        let (x, y) = ((i as isize) % w, (i as isize) / w);
        for (dx, dy, l) in [(1, 0, 1.0), (-1, 0, 1.0), (0, 1, 1.0), (0, -1, 1.0), (1, 1, 1.4142), (1, -1, 1.4142), (-1, 1, 1.4142), (-1, -1, 1.4142)] {
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || ny < 0 || nx >= w || ny >= h {
                continue;
            }
            let j = (ny * w + nx) as usize;
            if !fd.pass[j] {
                continue;
            }
            let nd = d + l * cost(j);
            if nd < dist[j] {
                dist[j] = nd;
                par[j] = i as u32;
                pq.push(Reverse(((nd * 1000.0) as u64, j)));
            }
        }
    }
    (dist, par)
}

fn nearest_seed(dist: &[f64], t: &DoorInfo) -> Option<usize> {
    t.seeds.iter().copied().filter(|&s| dist[s].is_finite()).min_by(|&a, &b| dist[a].partial_cmp(&dist[b]).unwrap())
}

/// Polyline from the source door to `to`, ending at the seed `end`.
fn shape(ctx: &Ctx, fd: &Field, par: &[u32], end: usize, from: &DoorInfo, to: &DoorInfo) -> Vec<Pt> {
    let f = &ctx.f;
    let n = f.len();
    let mut line = Vec::new();
    let mut c = end;
    let mut guard = 0;
    while c != u32::MAX as usize && guard < n {
        line.push(f.center_i(c));
        if par[c] == u32::MAX {
            break;
        }
        c = par[c] as usize;
        guard += 1;
    }
    line.reverse();
    let mut t = taut(f, &fd.comfy, &line);
    if t.len() < 2 {
        t = crate::geom::simplify(&line, ctx.res * 1.5);
    }
    t.insert(0, from.door.mid());
    t.push(to.door.mid());
    let mut o = orthogonalize(f, &fd.comfy, &fd.pass, &t, axis_of(from.inward), axis_of(to.inward));
    slide_into_door(&mut o, from.door.a, from.door.b, false);
    slide_into_door(&mut o, to.door.a, to.door.b, true);
    // without repeated points
    o.dedup_by(|a, b| a.dist(*b) < 1.0);
    o
}

/// All primary and secondary paths for the plan, on the empty floor.
pub fn compute(problem: &Problem) -> Result<Vec<CircPath>, String> {
    // the routes do not depend on paths already kept: leave them out so they do not block
    let mut pr = problem.clone();
    pr.plan.paths.clear();
    let ctx = Ctx::build(&pr)?;
    let fd = field(&ctx);
    let f = &ctx.f;
    let Some(main) = ctx.doors.iter().find(|d| d.door.kind == DoorKind::Main && !d.seeds.is_empty()) else {
        return Err("Mark the main entrance first.".into());
    };
    let mut prim: Vec<(String, String, Vec<Pt>)> = Vec::new();
    let (dist, par) = dijkstra(f, &fd, &main.seeds, ctx.p.path_w1 * 0.5);
    for t in ctx.doors.iter().filter(|d| matches!(d.door.kind, DoorKind::Office | DoorKind::Exit) && !d.seeds.is_empty()) {
        if let Some(end) = nearest_seed(&dist, t) {
            prim.push((String::new(), t.door.id.clone(), shape(&ctx, &fd, &par, end, main, t)));
        }
    }
    // secondary: each office door to its two nearest office doors (by walking distance)
    let offices: Vec<&DoorInfo> = ctx.doors.iter().filter(|d| d.door.kind == DoorKind::Office && !d.seeds.is_empty()).collect();
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    let mut sec: Vec<(String, String, Vec<Pt>)> = Vec::new();
    for (ai, a) in offices.iter().enumerate() {
        let (dist, par) = dijkstra(f, &fd, &a.seeds, ctx.p.path_w2 * 0.5);
        let mut near: Vec<(f64, usize, usize)> = offices
            .iter()
            .enumerate()
            .filter(|(bi, _)| *bi != ai)
            .filter_map(|(bi, b)| nearest_seed(&dist, b).map(|s| (dist[s], bi, s)))
            .collect();
        near.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
        for &(_, bi, end) in near.iter().take(2) {
            let key = (ai.min(bi), ai.max(bi));
            if !seen.insert(key) {
                continue;
            }
            sec.push((a.door.id.clone(), offices[bi].door.id.clone(), shape(&ctx, &fd, &par, end, a, offices[bi])));
        }
    }
    // parallel runs that are almost together become one line
    let mut pp: Vec<Vec<Pt>> = prim.iter().map(|x| x.2.clone()).collect();
    // two bands closer than a width apart waste the floor between them: merge into one
    bundle_within(f, &fd.pass, &mut pp, ctx.p.path_w1 * 1.6);
    let mut sp: Vec<Vec<Pt>> = sec.iter().map(|x| x.2.clone()).collect();
    bundle_within(f, &fd.pass, &mut sp, ctx.p.path_w2 * 1.6);
    let mut out = Vec::new();
    for ((from, to, _), pts) in prim.into_iter().zip(pp) {
        out.push(CircPath { id: format!("p:{to}"), kind: PathKind::Primary, from, to, pts, width: None, keep: true, edited: false });
    }
    for ((from, to, _), pts) in sec.into_iter().zip(sp) {
        out.push(CircPath { id: format!("s:{from}:{to}"), kind: PathKind::Secondary, from, to, pts, width: None, keep: false, edited: false });
    }
    Ok(out)
}
