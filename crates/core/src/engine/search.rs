//! Búsqueda: recocido simulado paralelo sobre el genoma + funciones de alto nivel.

use super::ctx::Ctx;
use super::decode::{decode, random_genome, Genome, Rng};
use super::eval::{evaluate, Eval, W_UNPLACED};
use crate::model::*;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub phase: String,
    pub fraction: f64,
    pub best_score: f64,
    pub seats_placed: u32,
    pub seats_requested: u32,
    pub iterations: u64,
    pub message: String,
}

pub type ProgressFn<'a> = &'a (dyn Fn(Progress) + Sync);

pub fn no_progress(_: Progress) {}

/// Inicialización voraz: para cada zona, el ángulo con mayor capacidad.
fn greedy_init(ctx: &Ctx, rng: &mut Rng) -> Genome {
    let mut g = random_genome(ctx, rng);
    for v in 0..ctx.variants.len() {
        g.variant = v;
        let nz = ctx.variants[v].zones.len();
        let na = ctx.variants[v].grids.len();
        for z in 1..nz {
            let mut best = (0u32, 0u8, false);
            for a in 0..na {
                for top in [false, true] {
                    let mut probe = g.clone();
                    for (zi, zg) in probe.vars[v].zones.iter_mut().enumerate() {
                        zg.enabled = zi == z;
                    }
                    probe.vars[v].zones[z].theta = a as u8;
                    probe.vars[v].zones[z].from_top = top;
                    let d = decode(ctx, &probe, true);
                    let placed: u32 = d.modules.iter().map(|m| ctx.catalog[m.t].seats_per_module(m.k)).sum::<u32>() + d.reserve_seats;
                    if placed > best.0 {
                        best = (placed, a as u8, top);
                    }
                }
            }
            g.vars[v].zones[z].theta = best.1;
            g.vars[v].zones[z].from_top = best.2;
        }
    }
    g.variant = 0;
    g
}

fn mutate(ctx: &Ctx, g: &mut Genome, rng: &mut Rng) {
    let nv = ctx.variants.len();
    if nv > 1 && rng.f32() < 0.06 {
        g.variant = rng.below(nv);
        return;
    }
    let v = g.variant;
    let var = &ctx.variants[v];
    let nz = var.zones.len();
    if nz <= 1 {
        return;
    }
    // zona ponderada por tamaño
    let total: usize = var.zones.iter().skip(1).map(|z| z.cells).sum();
    let mut r = rng.below(total.max(1));
    let mut z = 1;
    for (i, zi) in var.zones.iter().enumerate().skip(1) {
        if r < zi.cells {
            z = i;
            break;
        }
        r -= zi.cells;
    }
    let na = var.grids.len();
    let nt = ctx.catalog.len();
    let vg = &mut g.vars[v];
    let x = rng.f32();
    let zg = &mut vg.zones[z];
    if x < 0.22 {
        zg.theta = rng.below(na) as u8;
    } else if x < 0.42 {
        if rng.f32() < 0.5 {
            zg.off_y = rng.f32();
        } else {
            zg.off_y = (zg.off_y + (rng.f32() - 0.5) * 0.2).clamp(0.0, 1.0);
        }
    } else if x < 0.55 {
        zg.off_x = rng.f32();
    } else if x < 0.63 {
        zg.from_top = !zg.from_top;
    } else if x < 0.71 {
        zg.align = rng.below(4) as u8;
    } else if x < 0.76 {
        zg.pair = !zg.pair;
    } else if x < 0.82 {
        zg.first_near = !zg.first_near;
    } else if x < 0.90 && nt > 1 {
        let a = rng.below(nt);
        let b = rng.below(nt);
        zg.pref.swap(a, b);
    } else if x < 0.93 {
        zg.enabled = !zg.enabled;
    } else if vg.order.len() > 1 {
        let a = rng.below(vg.order.len());
        let b = rng.below(vg.order.len());
        vg.order.swap(a, b);
    }
}

fn decode_hash(d: &super::decode::Decoded, variant: usize) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    variant.hash(&mut h);
    d.reserve_seats.hash(&mut h);
    for m in &d.modules {
        (m.t, m.k, m.grid, (m.x0 * 0.1) as i64, (m.y0 * 0.1) as i64, m.seats_up).hash(&mut h);
    }
    h.finish()
}

pub struct SearchOut {
    pub genome: Genome,
    pub eval: Eval,
    pub iterations: u64,
}

/// Recocido simulado con varias cadenas en paralelo.
pub fn search(ctx: &Ctx, weights: &Weights, time_limit: f64, seed: u64, init: Option<Genome>, cancel: &AtomicBool, progress: ProgressFn, phase: &str) -> SearchOut {
    let start = Instant::now();
    let chains = rayon::current_num_threads().clamp(2, 16);
    let mut base_rng = Rng(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1);
    let base = match init {
        Some(g) => g,
        None => greedy_init(ctx, &mut base_rng),
    };
    // multi-inicio barato: solo decodificador
    let lb_of = |g: &Genome| -> (u32, u32) {
        let d = decode(ctx, g, true);
        let lb: u32 = d.remaining.iter().enumerate().map(|(i, &r)| {
            let t = &ctx.catalog[i];
            if t.flexible { r } else { r * t.seats_per_module(t.per_side) }
        }).sum();
        (lb, d.reserve_seats)
    };
    let mut starts: Vec<(u32, u32, Genome)> = Vec::new();
    for v in 0..ctx.variants.len() {
        let mut g0 = base.clone();
        g0.variant = v;
        let (lb, rs) = lb_of(&g0);
        starts.push((lb, rs, g0.clone()));
        let tries = 24 / ctx.variants.len().max(1);
        for _ in 0..tries {
            let mut g = g0.clone();
            for _ in 0..(1 + base_rng.below(4)) {
                mutate(ctx, &mut g, &mut base_rng);
            }
            g.variant = v;
            let (lb, rs) = lb_of(&g);
            starts.push((lb, rs, g));
        }
    }
    starts.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    let best: Mutex<Option<(Genome, Eval)>> = Mutex::new(None);
    let iters = AtomicU64::new(0);
    let last_report = Mutex::new(Instant::now());
    let requested = ctx.demand.iter().enumerate().map(|(i, &d)| {
        let t = &ctx.catalog[i];
        if t.flexible { d } else { d * t.seats_per_module(t.per_side) }
    }).sum::<u32>();

    (0..chains).into_par_iter().for_each(|c| {
        let mut rng = Rng((seed ^ (c as u64 + 1).wrapping_mul(0xD1B54A32D192ED03)) | 1);
        // cada cadena arranca desde uno de los mejores inicios (variantes distintas si es posible)
        let mut cur = {
            let mut picked: Option<&Genome> = None;
            let want_var = c % ctx.variants.len().max(1);
            let best_lb = starts[0].0;
            for st in &starts {
                if st.2.variant == want_var && st.0 <= best_lb + 8 {
                    picked = Some(&st.2);
                    break;
                }
            }
            picked.cloned().unwrap_or_else(|| starts[c.min(starts.len() - 1)].2.clone())
        };
        let mut cache: std::collections::HashMap<u64, f64> = std::collections::HashMap::new();
        let d = decode(ctx, &cur, weights.reserve > 0.0);
        let (mut cur_ev, _, _) = evaluate(ctx, &d, cur.variant, weights, false);
        {
            let mut b = best.lock().unwrap();
            if b.as_ref().map(|x| cur_ev.score < x.1.score).unwrap_or(true) {
                *b = Some((cur.clone(), cur_ev.clone()));
            }
        }
        let t0: f64 = 300.0;
        let t1: f64 = 2.0;
        loop {
            let el = start.elapsed().as_secs_f64();
            if el >= time_limit || cancel.load(Ordering::Relaxed) {
                break;
            }
            let frac = el / time_limit;
            let temp = t0 * (t1 / t0).powf(frac);
            let mut cand = cur.clone();
            let nm = 1 + (rng.f32() < 0.3) as usize;
            for _ in 0..nm {
                mutate(ctx, &mut cand, &mut rng);
            }
            let d = decode(ctx, &cand, weights.reserve > 0.0);
            // cota inferior barata: lo que el decodificador no pudo ubicar
            let lb: u32 = d.remaining.iter().enumerate().map(|(i, &r)| {
                let t = &ctx.catalog[i];
                if t.flexible { r } else { r * t.seats_per_module(t.per_side) }
            }).sum();
            iters.fetch_add(1, Ordering::Relaxed);
            if lb as f64 * W_UNPLACED > cur_ev.score + temp * 4.0 {
                continue;
            }
            let key = decode_hash(&d, cand.variant);
            if let Some(&sc) = cache.get(&key) {
                // misma disposición ya evaluada: aceptar el cambio de genoma si no empeora (deriva neutra)
                if sc <= cur_ev.score + 1e-9 {
                    cur = cand;
                }
                continue;
            }
            let (ev, _, _) = evaluate(ctx, &d, cand.variant, weights, false);
            if cache.len() > 20_000 {
                cache.clear();
            }
            cache.insert(key, ev.score);
            let delta = ev.score - cur_ev.score;
            if delta <= 0.0 || rng.f64() < (-delta / temp).exp() {
                cur = cand;
                cur_ev = ev;
                let mut b = best.lock().unwrap();
                if b.as_ref().map(|x| cur_ev.score < x.1.score - 1e-9).unwrap_or(true) {
                    *b = Some((cur.clone(), cur_ev.clone()));
                }
            }
            if c == 0 {
                let mut lr = last_report.lock().unwrap();
                if lr.elapsed().as_secs_f64() > 0.3 {
                    *lr = Instant::now();
                    let b = best.lock().unwrap();
                    if let Some((_, e)) = b.as_ref() {
                        progress(Progress {
                            phase: phase.into(),
                            fraction: frac,
                            best_score: e.score,
                            seats_placed: e.seats_placed,
                            seats_requested: requested,
                            iterations: iters.load(Ordering::Relaxed),
                            message: String::new(),
                        });
                    }
                }
            }
        }
    });
    let (genome, eval) = best.into_inner().unwrap().expect("sin resultado");
    if std::env::var("SEAT_DEBUG").is_ok() { eprintln!("search: {} iteraciones en {:.1}s ({} cadenas)", iters.load(Ordering::Relaxed), start.elapsed().as_secs_f64(), chains); }
    SearchOut { genome, eval, iterations: iters.load(Ordering::Relaxed) }
}

/// Construye el resultado final (Layout) a partir del mejor genoma.
pub fn to_layout(ctx: &Ctx, g: &Genome, weights: &Weights) -> Layout {
    let d = decode(ctx, g, true);
    let (ev, mods, seats) = evaluate(ctx, &d, g.variant, weights, true);
    let var = &ctx.variants[g.variant];
    let mut out_mods = Vec::new();
    let mut out_seats = Vec::new();
    // numeración en orden de lectura: por franjas horizontales de 2 m
    let mut order: Vec<usize> = (0..seats.len()).filter(|&i| ev.module_ok[seats[i].module]).collect();
    order.sort_by(|&a, &b| {
        let (pa, pb) = (seats[a].pos, seats[b].pos);
        let (ba, bb) = ((-pa.y / 2400.0).floor(), (-pb.y / 2400.0).floor());
        ba.partial_cmp(&bb).unwrap().then(pa.x.partial_cmp(&pb.x).unwrap())
    });
    let mut mod_ids = vec![String::new(); mods.len()];
    let fixed: std::collections::HashSet<String> = mods.iter().filter_map(|m| m.fixed_id.clone()).collect();
    let mut counter = 0usize;
    for (mi, m) in mods.iter().enumerate() {
        if !ev.module_ok[mi] {
            continue;
        }
        let id = match &m.fixed_id {
            Some(id) => id.clone(),
            None => loop {
                counter += 1;
                let c = format!("m{counter}");
                if !fixed.contains(&c) {
                    break c;
                }
            },
        };
        mod_ids[mi] = id.clone();
        let t = &ctx.catalog[m.pm.t];
        out_mods.push(Module {
            id,
            type_id: t.id.clone(),
            center: m.center,
            angle: m.angle,
            length: m.pm.len,
            depth: m.pm.dep,
            per_side: m.pm.k,
            seats_up: m.pm.seats_up,
            body: m.body.clone(),
            pinned: m.pinned,
            people: m.people.clone(),
        });
    }
    for (num, &si) in order.iter().enumerate() {
        let s = &seats[si];
        let m = &mods[s.module];
        // persona bloqueada (módulos fijados): el puesto más cercano a la posición guardada
        let person = if m.pinned {
            m.people.iter().filter(|p| p.pos.dist(s.pos) < 700.0).min_by(|a, b| a.pos.dist(s.pos).partial_cmp(&b.pos.dist(s.pos)).unwrap())
        } else {
            None
        };
        out_seats.push(Seat {
            id: format!("s{}", num + 1),
            number: num as u32 + 1,
            module_id: mod_ids[s.module].clone(),
            type_id: ctx.catalog[m.pm.t].id.clone(),
            pos: s.pos,
            facing: s.facing,
            chair: s.chair.clone(),
            role_id: person.map(|p| p.role_id.clone()).unwrap_or_default(),
            client_id: person.map(|p| p.client_id.clone()).unwrap_or_default(),
            flags: ev.seat_flags[si].clone(),
            locked: m.pinned && person.map(|p| !p.role_id.is_empty()).unwrap_or(false),
        });
    }
    // módulos inválidos → avisos
    let mut violations = ev.violations.clone();
    let invalid = ev.module_ok.iter().filter(|&&b| !b).count();
    if invalid > 0 {
        violations.push(Violation {
            rule_id: "C7".into(),
            message: format!("{invalid} módulos se descartaron por no tener acceso o recorrido válido."),
            at: None,
            hard: false,
        });
    }
    let mut unplaced_by_type = Vec::new();
    let mut placed_units = vec![0u32; ctx.catalog.len()];
    for (mi, m) in mods.iter().enumerate() {
        if ev.module_ok[mi] && !m.pinned {
            let t = &ctx.catalog[m.pm.t];
            placed_units[m.pm.t] += if t.flexible && t.shape == DeskShape::Straight { t.seats_per_module(m.pm.k) } else { 1 };
        }
    }
    let mut modules_requested = 0;
    for (i, t) in ctx.catalog.iter().enumerate() {
        let miss = ctx.demand[i].saturating_sub(placed_units[i]);
        if !t.flexible {
            modules_requested += ctx.demand[i];
        }
        if miss > 0 {
            unplaced_by_type.push((t.id.clone(), miss));
        }
    }
    let corridor_area = var.corridor.iter().filter(|&&b| b).count() as f64 * ctx.res * ctx.res / 1e6;
    let kpis = Kpis {
        seats_requested: ev.seats_requested,
        seats_placed: ev.seats_placed,
        modules_requested,
        modules_placed: out_mods.len() as u32,
        unplaced_by_type,
        area_m2: ctx.area_m2,
        m2_per_seat: if ev.seats_placed > 0 { ctx.area_m2 / ev.seats_placed as f64 } else { 0.0 },
        avg_travel_exit_m: ev.avg_travel / 1000.0,
        max_travel_exit_m: ev.max_travel / 1000.0,
        daylight_pct: ev.daylight_frac * 100.0,
        window_side_pct: ev.window_side_frac * 100.0,
        privacy_issues: ev.privacy_issues,
        orientations: ev.orientations,
        reserve_seats: d.reserve_seats,
        reserve_pct: if ev.seats_placed > 0 { d.reserve_seats as f64 / ev.seats_placed as f64 * 100.0 } else { 0.0 },
        accessible_pmr: ev.pmr,
        hard_violations: ev.hard,
        grouping_score: 0.0,
        misplaced_people: 0,
        corridor_area_m2: corridor_area,
        score: ev.score,
    };
    let mut keepouts = ctx.keep_polys.clone();
    keepouts.truncate(400);
    Layout {
        modules: out_mods,
        seats: out_seats,
        corridors: var.corridor_polys.clone(),
        keepouts,
        kpis,
        violations,
        weights: weights.clone(),
        genome: serde_json::to_value(g).ok(),
    }
}

// ============================================================================ alto nivel

pub fn optimize(problem: &Problem, cancel: &AtomicBool, progress: ProgressFn) -> Result<Layout, String> {
    progress(Progress { phase: "Preparando el plano".into(), ..Default::default() });
    let ctx = Ctx::build(problem)?;
    let st = &problem.config.settings;
    let out = search(&ctx, &problem.config.weights, st.time_limit.max(0.5), st.seed.max(1), None, cancel, progress, "Optimizando distribución");
    let mut layout = to_layout(&ctx, &out.genome, &problem.config.weights);
    if crate::assign::has_people(&problem.config.org) {
        progress(Progress { phase: "Asignando roles y clientes".into(), fraction: 1.0, ..Default::default() });
        crate::assign::assign(&ctx, &problem.config, &mut layout);
    }
    Ok(layout)
}

/// Exploración de pesos: varias corridas con pesos alrededor de los del usuario y frente de Pareto.
pub fn explore(problem: &Problem, n: usize, cancel: &AtomicBool, progress: ProgressFn) -> Result<Vec<crate::model::Variant>, String> {
    progress(Progress { phase: "Preparando el plano".into(), ..Default::default() });
    let ctx = Ctx::build(problem)?;
    let base = problem.config.weights.as_vec();
    let mut rng = Rng(problem.config.settings.seed.max(7) * 2654435761 | 1);
    let mut samples: Vec<Weights> = vec![problem.config.weights.clone()];
    // énfasis en cada criterio
    for k in 0..7 {
        let mut v = base;
        v[k] = (v[k] * 2.5).max(6.0);
        samples.push(Weights::from_vec(v));
    }
    while samples.len() < n.max(2) {
        let mut v = base;
        for x in v.iter_mut() {
            let z = (rng.f64() - 0.5) * 2.4;
            *x = (*x * z.exp()).clamp(0.0, 10.0);
            if rng.f64() < 0.12 {
                *x = 0.0;
            }
        }
        samples.push(Weights::from_vec(v));
    }
    samples.truncate(n.max(2));
    let each = (problem.config.settings.time_limit * 0.6).clamp(1.0, 10.0);
    let total = samples.len();
    let done = AtomicU64::new(0);
    let base_out = search(&ctx, &problem.config.weights, each, 11, None, cancel, &|_| {}, "Explorando");
    let init = base_out.genome.clone();
    let mut layouts: Vec<Layout> = Vec::new();
    for (i, w) in samples.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let out = search(&ctx, w, each, 101 + i as u64, Some(init.clone()), cancel, &|_| {}, "Explorando");
        let mut l = to_layout(&ctx, &out.genome, w);
        if crate::assign::has_people(&problem.config.org) {
            crate::assign::assign(&ctx, &problem.config, &mut l);
        }
        let d = done.fetch_add(1, Ordering::Relaxed) + 1;
        progress(Progress {
            phase: "Explorando pesos".into(),
            fraction: d as f64 / total as f64,
            best_score: l.kpis.score,
            seats_placed: l.kpis.seats_placed,
            seats_requested: l.kpis.seats_requested,
            iterations: d,
            message: format!("Variante {d} de {total}"),
        });
        layouts.push(l);
    }
    // Pareto sobre indicadores fijos (independientes de los pesos)
    let kv = |k: &Kpis| -> [f64; 8] {
        [
            k.seats_placed as f64,
            -k.avg_travel_exit_m,
            k.daylight_pct,
            k.window_side_pct,
            -(k.privacy_issues as f64),
            -(k.orientations as f64),
            k.reserve_pct,
            k.grouping_score,
        ]
    };
    let vals: Vec<[f64; 8]> = layouts.iter().map(|l| kv(&l.kpis)).collect();
    let mut rank = vec![0u32; layouts.len()];
    let mut remaining: Vec<usize> = (0..layouts.len()).collect();
    let mut r = 1;
    while !remaining.is_empty() {
        let front: Vec<usize> = remaining
            .iter()
            .copied()
            .filter(|&i| {
                !remaining.iter().any(|&j| j != i && (0..8).all(|k| vals[j][k] >= vals[i][k] - 1e-9) && (0..8).any(|k| vals[j][k] > vals[i][k] + 1e-9))
            })
            .collect();
        for &i in &front {
            rank[i] = r;
        }
        remaining.retain(|i| !front.contains(i));
        r += 1;
    }
    let mut out: Vec<crate::model::Variant> = layouts.into_iter().zip(rank).map(|(layout, pareto_rank)| crate::model::Variant { layout, pareto_rank }).collect();
    out.sort_by(|a, b| a.pareto_rank.cmp(&b.pareto_rank).then(b.layout.kpis.seats_placed.cmp(&a.layout.kpis.seats_placed)));
    Ok(out)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capacity {
    pub type_id: String,
    pub max_seats: u32,
    pub max_units: u32,
}

/// Capacidad máxima estimada por tipo (si se usara solo ese tipo).
pub fn capacity(problem: &Problem, cancel: &AtomicBool, progress: ProgressFn) -> Result<Vec<Capacity>, String> {
    let ctx0 = Ctx::build(problem)?;
    let types: Vec<usize> = (0..ctx0.catalog.len()).collect();
    let done = AtomicU64::new(0);
    let total = types.len();
    let res: Vec<Capacity> = types
        .par_iter()
        .map(|&ti| {
            let mut ctx = Ctx::build(problem).unwrap();
            ctx.demand = vec![0; ctx.catalog.len()];
            ctx.demand[ti] = 100_000;
            let mut w = problem.config.weights.clone();
            w.reserve = 0.0;
            let out = search(&ctx, &w, 1.2, 3, None, cancel, &|_| {}, "Capacidad");
            let t = &ctx.catalog[ti];
            let seats = out.eval.seats_placed;
            let units = if t.flexible { seats } else { seats / t.seats_per_module(t.per_side).max(1) };
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            progress(Progress { phase: "Calculando capacidad".into(), fraction: d as f64 / total as f64, message: t.name.clone(), ..Default::default() });
            Capacity { type_id: t.id.clone(), max_seats: seats, max_units: units }
        })
        .collect();
    let _ = ctx0;
    Ok(res)
}

/// Si no caben todos: prueba relajar reglas (no bloqueadas) y cambiar tipos, y mide cuántos puestos se ganan.
pub fn suggest(problem: &Problem, cancel: &AtomicBool, progress: ProgressFn) -> Result<Vec<Suggestion>, String> {
    progress(Progress { phase: "Midiendo la base".into(), ..Default::default() });
    let ctx = Ctx::build(problem)?;
    let base = search(&ctx, &problem.config.weights, 2.0, 5, None, cancel, &|_| {}, "Base");
    let base_seats = base.eval.seats_placed as i32;
    drop(ctx);
    let mut cands: Vec<(Suggestion, Problem)> = Vec::new();
    for (ri, r) in problem.config.rules.iter().enumerate() {
        if !r.enabled || r.locked {
            continue;
        }
        if r.group.starts_with("Organización") || r.group.starts_with("Confort") && !["A5"].contains(&r.id.as_str()) {
            continue;
        }
        for (ii, it) in r.items.iter().enumerate() {
            let (Some(v), Some(to)) = (it.value, it.relax_to) else { continue };
            if !it.enabled || (v - to).abs() < 1e-9 {
                continue;
            }
            for frac in [0.5, 1.0] {
                let nv = v + (to - v) * frac;
                let nv = if it.step > 0.0 { (nv / it.step).round() * it.step } else { nv };
                if (nv - v).abs() < 1e-9 {
                    continue;
                }
                let mut p = problem.clone();
                p.config.rules[ri].items[ii].value = Some(nv);
                let unit = if it.unit == "mm" { format!("{:.2} m", nv / 1000.0) } else { format!("{nv} {}", it.unit) };
                let from = if it.unit == "mm" { format!("{:.2} m", v / 1000.0) } else { format!("{v} {}", it.unit) };
                cands.push((
                    Suggestion { rule_id: r.id.clone(), item_id: it.id.clone(), description: format!("{} · {}: de {} a {}", r.name, it.label, from, unit), new_value: Some(nv), disable: false, gain_seats: 0, seats_placed: 0 },
                    p,
                ));
            }
        }
        // apagar reglas no estructurales
        if ["O3", "E7", "E5", "C3"].contains(&r.id.as_str()) {
            let mut p = problem.clone();
            p.config.rules[ri].enabled = false;
            cands.push((Suggestion { rule_id: r.id.clone(), item_id: String::new(), description: format!("Desactivar «{}»", r.name), new_value: None, disable: true, gain_seats: 0, seats_placed: 0 }, p));
        }
        if r.id == "E6" && !r.items.iter().any(|i| i.id == "muros_inclinados" && i.enabled) {
            let mut p = problem.clone();
            if let Some(it) = p.config.rules[ri].items.iter_mut().find(|i| i.id == "muros_inclinados") {
                it.enabled = true;
            }
            cands.push((Suggestion { rule_id: "E6".into(), item_id: "muros_inclinados".into(), description: "Permitir filas alineadas a muros inclinados".into(), new_value: None, disable: false, gain_seats: 0, seats_placed: 0 }, p));
        }
        if r.id == "C1" && r.items.iter().any(|i| i.id == "oficinas" && i.enabled) {
            let mut p = problem.clone();
            if let Some(it) = p.config.rules[ri].items.iter_mut().find(|i| i.id == "oficinas") {
                it.enabled = false;
            }
            cands.push((Suggestion { rule_id: "C1".into(), item_id: "oficinas".into(), description: "No llevar el pasillo principal hasta cada puerta de oficina".into(), new_value: None, disable: true, gain_seats: 0, seats_placed: 0 }, p));
        }
    }
    // cambio de tipo: probar los tipos con menos área por puesto (también de un lado ↔ dos lados)
    let area_per_seat = |t: &DeskType| t.module_length(t.per_side_range().1) * t.module_depth() / t.seats_per_module(t.per_side_range().1).max(1) as f64;
    for d in &problem.config.demand {
        if d.quantity == 0 {
            continue;
        }
        let Some(t) = problem.config.catalog.iter().find(|t| t.id == d.type_id) else { continue };
        let mut alts: Vec<&DeskType> = problem.config.catalog.iter().filter(|a| a.id != t.id && area_per_seat(a) <= area_per_seat(t) * 1.1).collect();
        // preferir los flexibles (se adaptan mejor a espacios irregulares) y luego menor área por puesto
        alts.sort_by(|a, b| b.flexible.cmp(&a.flexible).then(area_per_seat(a).partial_cmp(&area_per_seat(b)).unwrap()));
        for alt in alts.into_iter().take(3) {
            let mut p = problem.clone();
            let seats_t = if t.flexible { d.quantity } else { d.quantity * t.seats_per_module(t.per_side) };
            let qty_alt = if alt.flexible { seats_t } else { seats_t.div_ceil(alt.seats_per_module(alt.per_side).max(1)) };
            p.config.demand.retain(|x| x.type_id != t.id);
            if let Some(x) = p.config.demand.iter_mut().find(|x| x.type_id == alt.id) {
                x.quantity += qty_alt;
            } else {
                p.config.demand.push(Demand { type_id: alt.id.clone(), quantity: qty_alt });
            }
            cands.push((Suggestion { rule_id: "TIPO".into(), item_id: format!("{}>{}", t.id, alt.id), description: format!("Usar «{}» en lugar de «{}»", alt.name, t.name), new_value: None, disable: false, gain_seats: 0, seats_placed: 0 }, p));
        }
    }
    let total = cands.len();
    let done = AtomicU64::new(0);
    let mut results: Vec<Suggestion> = cands
        .into_par_iter()
        .filter_map(|(mut s, p)| {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            let ctx = Ctx::build(&p).ok()?;
            let out = search(&ctx, &p.config.weights, 1.5, 5, None, cancel, &|_| {}, "Sugerencias");
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            progress(Progress { phase: "Probando cambios de reglas".into(), fraction: d as f64 / total.max(1) as f64, message: s.description.clone(), ..Default::default() });
            s.seats_placed = out.eval.seats_placed;
            // con cambio de tipo, lo que importa es si se ubica todo
            s.gain_seats = if s.rule_id == "TIPO" {
                base.eval.unplaced_seats as i32 - out.eval.unplaced_seats as i32
            } else {
                out.eval.seats_placed as i32 - base_seats
            };
            Some(s)
        })
        .filter(|s| s.gain_seats > 0)
        .collect();
    results.sort_by(|a, b| b.gain_seats.cmp(&a.gain_seats));
    results.truncate(12);
    Ok(results)
}
