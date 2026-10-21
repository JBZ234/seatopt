//! Búsqueda: recocido simulado paralelo sobre el genoma + funciones de alto nivel.

use super::ctx::Ctx;
use super::decode::{decode, random_genome, Genome, Rng};
use super::eval::{deficit, evaluate, Eval};
use crate::geom::Pt;
use crate::model::*;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use crate::time::Instant;

/// Hilos disponibles para la búsqueda.
pub fn thread_count() -> usize {
    rayon::current_num_threads().max(1)
}

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
    /// Mejores disposiciones distintas encontradas (puntaje, genoma), mejor primero.
    pub pool: Vec<(f64, Genome)>,
}

const POOL: usize = 48;

/// Guarda las mejores disposiciones distintas (por huella del decodificado).
struct Pool {
    items: Vec<(f64, u64, Genome)>,
}
impl Pool {
    fn offer(&mut self, score: f64, key: u64, g: &Genome) {
        if self.items.len() >= POOL && score >= self.items.last().unwrap().0 {
            return;
        }
        if let Some(i) = self.items.iter().position(|x| x.1 == key) {
            if score < self.items[i].0 {
                self.items.remove(i);
            } else {
                return;
            }
        }
        let pos = self.items.partition_point(|x| x.0 <= score);
        self.items.insert(pos, (score, key, g.clone()));
        self.items.truncate(POOL);
    }
}

/// Cuánto buscar: tope duro, mínimo y, si hay `patience`, parar cuando pasa ese tiempo sin mejoras.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub cap: f64,
    pub min: f64,
    pub patience: Option<f64>,
}

impl Budget {
    pub fn fixed(secs: f64) -> Self {
        Budget { cap: secs, min: secs, patience: None }
    }
    /// Automático: la paciencia crece con el área del plano y es mayor con pocos núcleos.
    pub fn auto(area_m2: f64) -> Self {
        let threads = rayon::current_num_threads().max(1) as f64;
        let patience = (3.0 + area_m2 / 120.0).clamp(4.0, 14.0) * (8.0 / threads).clamp(1.0, 2.5);
        Budget { cap: 120.0, min: (patience * 0.8).max(3.0), patience: Some(patience) }
    }
}

/// Recocido simulado con varias cadenas en paralelo (tiempo fijo).
pub fn search(ctx: &Ctx, weights: &Weights, time_limit: f64, seed: u64, init: Option<Genome>, cancel: &AtomicBool, progress: ProgressFn, phase: &str) -> SearchOut {
    search_budget(ctx, weights, Budget::fixed(time_limit), seed, init, cancel, progress, phase)
}

/// Recocido simulado con varias cadenas en paralelo.
#[allow(clippy::too_many_arguments)]
pub fn search_budget(ctx: &Ctx, weights: &Weights, budget: Budget, seed: u64, init: Option<Genome>, cancel: &AtomicBool, progress: ProgressFn, phase: &str) -> SearchOut {
    let start = Instant::now();
    let time_limit = budget.cap;
    // la temperatura baja sobre un horizonte estimado; después queda en búsqueda local
    let horizon = match budget.patience { Some(p) => (p * 3.0).max(budget.min), None => time_limit };
    // momento (ms desde el inicio) de la última mejora del mejor resultado
    let last_imp = AtomicU64::new(0);
    let chains = rayon::current_num_threads().clamp(2, 16);
    let mut base_rng = Rng(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1);
    let base = match init {
        Some(g) => g,
        None => greedy_init(ctx, &mut base_rng),
    };
    // multi-inicio barato: solo decodificador
    let lb_of = |g: &Genome| -> (u64, u32) {
        let d = decode(ctx, g, true);
        (deficit(ctx, &d.remaining) as u64, d.reserve_seats)
    };
    let mut starts: Vec<(u64, u32, Genome)> = Vec::new();
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
    let pool = Mutex::new(Pool { items: Vec::new() });
    let pool_worst = AtomicU64::new(f64::INFINITY.to_bits());
    let iters = AtomicU64::new(0);
    let last_report = Mutex::new(Instant::now());
    let requested = ctx.demand.iter().enumerate().filter(|(i, _)| !ctx.is_max[*i]).map(|(i, &d)| {
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
                if st.2.variant == want_var && st.0 as f64 <= best_lb as f64 + 8.0 * super::eval::W_UNPLACED {
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
            if let Some(p) = budget.patience {
                let since = el - last_imp.load(Ordering::Relaxed) as f64 / 1000.0;
                if el >= budget.min && since >= p {
                    break;
                }
            }
            let frac = (el / horizon).min(1.0);
            let temp = t0 * (t1 / t0).powf(frac);
            let mut cand = cur.clone();
            let nm = 1 + (rng.f32() < 0.3) as usize;
            for _ in 0..nm {
                mutate(ctx, &mut cand, &mut rng);
            }
            let d = decode(ctx, &cand, weights.reserve > 0.0);
            // cota inferior barata: lo que el decodificador no pudo ubicar
            let lb = deficit(ctx, &d.remaining);
            iters.fetch_add(1, Ordering::Relaxed);
            if lb > cur_ev.score + temp * 4.0 {
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
            if ev.score < f64::from_bits(pool_worst.load(Ordering::Relaxed)) {
                let mut pl = pool.lock().unwrap();
                pl.offer(ev.score, key, &cand);
                if pl.items.len() >= POOL {
                    pool_worst.store(pl.items.last().unwrap().0.to_bits(), Ordering::Relaxed);
                }
            }
            let delta = ev.score - cur_ev.score;
            if delta <= 0.0 || rng.f64() < (-delta / temp).exp() {
                cur = cand;
                cur_ev = ev;
                let mut b = best.lock().unwrap();
                if b.as_ref().map(|x| cur_ev.score < x.1.score - 1e-9).unwrap_or(true) {
                    // mejoras de verdad (no solo desempates mínimos) reinician la paciencia
                    if b.as_ref().map(|x| x.1.score - cur_ev.score > 50.0).unwrap_or(true) {
                        last_imp.store((start.elapsed().as_secs_f64() * 1000.0) as u64, Ordering::Relaxed);
                    }
                    *b = Some((cur.clone(), cur_ev.clone()));
                }
            }
            if c == 0 {
                let mut lr = last_report.lock().unwrap();
                if lr.elapsed().as_secs_f64() > 0.3 {
                    *lr = Instant::now();
                    let b = best.lock().unwrap();
                    if let Some((_, e)) = b.as_ref() {
                        let el = start.elapsed().as_secs_f64();
                        let since = el - last_imp.load(Ordering::Relaxed) as f64 / 1000.0;
                        let (fraction, message) = match budget.patience {
                            Some(p) => ((since / p).clamp(0.0, 1.0), format!("{} seats · last improvement {:.0} s ago", e.seats_placed, since.max(0.0))),
                            None => (frac, String::new()),
                        };
                        progress(Progress {
                            phase: phase.into(),
                            fraction,
                            best_score: e.score,
                            seats_placed: e.seats_placed,
                            seats_requested: requested,
                            iterations: iters.load(Ordering::Relaxed),
                            message,
                        });
                    }
                }
            }
        }
    });
    let (genome, eval) = best.into_inner().unwrap().expect("no result");
    if std::env::var("SEAT_DEBUG").is_ok() { eprintln!("search: {} iterations in {:.1}s ({} chains)", iters.load(Ordering::Relaxed), start.elapsed().as_secs_f64(), chains); }
    let mut pool: Vec<(f64, Genome)> = pool.into_inner().unwrap().items.into_iter().map(|x| (x.0, x.2)).collect();
    pool.insert(0, (eval.score, genome.clone()));
    SearchOut { genome, eval, iterations: iters.load(Ordering::Relaxed), pool }
}

/// Construye el resultado final (Layout) a partir del mejor genoma.
pub fn to_layout(ctx: &Ctx, g: &Genome, weights: &Weights) -> Layout {
    let d = decode(ctx, g, true);
    to_layout_from(ctx, &d, g.variant, serde_json::to_value(g).ok(), weights)
}

/// Resultado final a partir de un decodificado (por ejemplo, ya con huecos rellenados).
pub fn to_layout_from(ctx: &Ctx, d: &super::decode::Decoded, variant: usize, genome: Option<serde_json::Value>, weights: &Weights) -> Layout {
    let (ev, mods, seats) = evaluate(ctx, d, variant, weights, true);
    let var = &ctx.variants[variant];
    let mut out_mods = Vec::new();
    let mut out_seats = Vec::new();
    // numeración en orden de lectura: por franjas horizontales de 2 m
    // los fijados se muestran siempre (aunque tengan avisos); los demás solo si son válidos
    let shown = |mi: usize| ev.module_ok[mi] || mods[mi].pinned;
    let mut order: Vec<usize> = (0..seats.len()).filter(|&i| shown(seats[i].module)).collect();
    order.sort_by(|&a, &b| {
        let (pa, pb) = (seats[a].pos, seats[b].pos);
        let (ba, bb) = ((-pa.y / 2400.0).floor(), (-pb.y / 2400.0).floor());
        ba.partial_cmp(&bb).unwrap().then(pa.x.partial_cmp(&pb.x).unwrap())
    });
    let mut mod_ids = vec![String::new(); mods.len()];
    let fixed: std::collections::HashSet<String> = mods.iter().filter_map(|m| m.fixed_id.clone()).collect();
    let mut counter = 0usize;
    for (mi, m) in mods.iter().enumerate() {
        if !shown(mi) {
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
            message: format!("{invalid} desks were discarded for lacking access or a valid route."),
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
        if ctx.is_max[i] {
            continue;
        }
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
        max_mode: ctx.is_max.iter().any(|&m| m),
        max_seats: ev.max_seats,
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
        genome,
        corridor_lines: var.corridor_lines.clone(),
        // with circulation paths kept by the user, those are the routes; no extra drawing
        access_paths: if ctx.p.paths_on && ctx.plan.paths.iter().any(|c| c.keep) {
            vec![]
        } else {
            access_paths(ctx, &mods, &seats, &(0..mods.len()).map(|i| ev.module_ok[i] || mods[i].pinned).collect::<Vec<_>>())
        },
        corridor_width: if var.corridor_lines.is_empty() { 0.0 } else { ctx.p.c1_w },
        alternatives: vec![],
    }
}

/// «Tensa» una polilínea de celdas: desde cada punto salta al más lejano que se ve en línea recta
/// por celdas permitidas. Queda una sucesión de rectas (las curvas las pone el dibujo en las esquinas).
pub(crate) fn taut(f: &crate::raster::Frame, ok: &[bool], pts: &[Pt]) -> Vec<Pt> {
    if pts.len() <= 2 {
        return pts.to_vec();
    }
    let step = f.res * 0.5;
    let visible = |a: Pt, b: Pt| -> bool {
        let n = ((a.dist(b) / step).ceil() as usize).max(1);
        (0..=n).all(|k| f.cell_idx(a.lerp(b, k as f64 / n as f64)).map(|i| ok[i]).unwrap_or(false))
    };
    let mut out = vec![pts[0]];
    let mut i = 0;
    while i < pts.len() - 1 {
        // el más lejano visible (buscando desde el final del tramo)
        let mut j = pts.len() - 1;
        while j > i + 1 && !visible(pts[i], pts[j]) {
            j -= 1;
        }
        out.push(pts[j]);
        i = j;
    }
    out
}

/// Recorridos desde la puerta principal hasta cada puerta de oficina, por el piso ya amoblado.
/// Solo para mostrar: van por el centro de los pasillos (el costo sube cerca de muros y escritorios).
pub fn access_paths(ctx: &Ctx, mods: &[super::eval::WModule], seats: &[super::eval::WSeat], shown: &[bool]) -> Vec<Vec<Pt>> {
    use crate::raster;
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    let f = &ctx.f;
    let n = f.len();
    let targets: Vec<&super::ctx::DoorInfo> = ctx.doors.iter().filter(|d| d.door.kind == DoorKind::Office && !d.seeds.is_empty()).collect();
    if targets.is_empty() || ctx.main_seeds.is_empty() {
        return vec![];
    }
    let mut free = ctx.free_base.clone();
    for (i, m) in mods.iter().enumerate() {
        if shown.get(i).copied().unwrap_or(false) {
            raster::fill_polygon(f, &mut free, &m.body, false);
        }
    }
    for s in seats {
        if shown.get(s.module).copied().unwrap_or(false) {
            raster::fill_polygon(f, &mut free, &s.chair, false);
        }
    }
    let blocked: Vec<bool> = free.iter().map(|b| !b).collect();
    let clear = raster::edt(f, &blocked);
    let res = ctx.res;
    let pass: Vec<bool> = (0..n).map(|i| free[i] && ((clear[i] as f64) >= 280.0 || ctx.door_approach[i])).collect();
    // Dijkstra con costo mayor cerca de obstáculos: el recorrido va por el medio
    let cost = |i: usize| -> f64 { 1.0 + 3.0 * (1.0 - ((clear[i] as f64) / 1500.0).min(1.0)) };
    let mut dist = vec![f64::INFINITY; n];
    let mut par = vec![u32::MAX; n];
    let mut pq: BinaryHeap<Reverse<(u64, usize)>> = BinaryHeap::new();
    for &s in &ctx.main_seeds {
        if s < n {
            dist[s] = 0.0;
            pq.push(Reverse((0, s)));
        }
    }
    let w = f.w as isize;
    let h = f.h as isize;
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
            if !pass[j] {
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
    let main_mid = ctx.doors.iter().find(|d| d.door.kind == DoorKind::Main).map(|d| d.door.mid());
    let main_in = ctx.doors.iter().find(|d| d.door.kind == DoorKind::Main).map(|d| d.inward);
    let main_door = ctx.doors.iter().find(|d| d.door.kind == DoorKind::Main).map(|d| &d.door);
    let comfy: Vec<bool> = (0..n).map(|i| pass[i] && ((clear[i] as f64) >= 380.0 || ctx.door_approach[i])).collect();
    let mut out = Vec::new();
    for t in targets {
        let Some(&end) = t.seeds.iter().filter(|&&s| dist[s].is_finite()).min_by(|&&a, &&b| dist[a].partial_cmp(&dist[b]).unwrap()) else { continue };
        let t_door_mid = t.door.mid();
        let t_door = &t.door;
        let t_in = t.inward;
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
        // tensar por celdas con holgura (se aleja de muros y escritorios); los extremos son el vano
        let mut t = taut(f, &comfy, &line);
        if t.len() < 2 {
            t = crate::geom::simplify(&line, res * 1.5);
        }
        if let Some(m) = main_mid {
            t.insert(0, m);
        }
        t.push(t_door_mid);
        // solo para el dibujo: tramos horizontales y verticales (L o S), saliendo y llegando de frente
        let first = main_in.and_then(axis_of);
        let last = axis_of(t_in);
        let mut o = orthogonalize(f, &comfy, &pass, &t, first, last);
        if let Some(m) = main_door {
            slide_into_door(&mut o, m.a, m.b, false);
        }
        slide_into_door(&mut o, t_door.a, t_door.b, true);
        out.push(o);
    }
    bundle(f, &pass, &mut out);
    out
}

/// Eje de una dirección: Some(true) horizontal, Some(false) vertical, None si es diagonal.
pub(crate) fn axis_of(v: Pt) -> Option<bool> {
    let (ax, ay) = (v.x.abs(), v.y.abs());
    if ax.max(ay) < 1e-9 || ax.min(ay) / ax.max(ay) > 0.18 {
        return None;
    }
    Some(ax >= ay)
}

/// Si dos recorridos van casi juntos por el mismo pasillo, el segundo se monta sobre el primero
/// (una sola línea en vez de dos paralelas pegadas). Solo mueve tramos interiores y si sigue libre.
pub(crate) fn bundle(f: &crate::raster::Frame, pass: &[bool], paths: &mut [Vec<Pt>]) {
    bundle_within(f, pass, paths, 450.0);
}

/// Like `bundle`, merging parallel runs closer than `reach` (mm).
pub(crate) fn bundle_within(f: &crate::raster::Frame, pass: &[bool], paths: &mut [Vec<Pt>], reach: f64) {
    let eps = 1.0;
    let step = f.res * 0.5;
    let see = |a: Pt, b: Pt| -> bool {
        let n = ((a.dist(b) / step).ceil() as usize).max(1);
        (1..n).all(|k| f.cell_idx(a.lerp(b, k as f64 / n as f64)).map(|i| pass[i]).unwrap_or(false))
    };
    // (horizontal?, coordenada fija, desde, hasta)
    let mut lanes: Vec<(bool, f64, f64, f64)> = Vec::new();
    let add = |lanes: &mut Vec<(bool, f64, f64, f64)>, p: &[Pt]| {
        for w in p.windows(2) {
            let h = (w[0].y - w[1].y).abs() < eps;
            let v = (w[0].x - w[1].x).abs() < eps;
            if h { lanes.push((true, w[0].y, w[0].x.min(w[1].x), w[0].x.max(w[1].x))); }
            if v { lanes.push((false, w[0].x, w[0].y.min(w[1].y), w[0].y.max(w[1].y))); }
        }
    };
    for k in 0..paths.len() {
        if k > 0 {
            let p = &mut paths[k];
            // tramos interiores: ni el que sale de la entrada ni el que llega a la oficina
            for s in 1..p.len().saturating_sub(2) {
                let (a, b) = (p[s], p[s + 1]);
                let h = (a.y - b.y).abs() < eps;
                let v = (a.x - b.x).abs() < eps;
                if !h && !v { continue; }
                let (c, lo, hi) = if h { (a.y, a.x.min(b.x), a.x.max(b.x)) } else { (a.x, a.y.min(b.y), a.y.max(b.y)) };
                let best = lanes.iter().filter(|l| l.0 == h && (l.1 - c).abs() > eps && (l.1 - c).abs() < reach && l.2.max(lo) < l.3.min(hi))
                    .min_by(|x, y| (x.1 - c).abs().partial_cmp(&(y.1 - c).abs()).unwrap()).map(|l| l.1);
                let Some(nc) = best else { continue };
                let mv = |q: Pt| if h { Pt { x: q.x, y: nc } } else { Pt { x: nc, y: q.y } };
                let (na, nb) = (mv(a), mv(b));
                // el tramo movido y sus vecinos (que solo cambian de largo) deben seguir libres
                if see(na, nb) && see(p[s - 1], na) && see(nb, p[s + 2]) {
                    p[s] = na;
                    p[s + 1] = nb;
                }
            }
        }
        let snapshot = paths[k].clone();
        add(&mut lanes, &snapshot);
    }
}

/// Si el recorrido llega a la puerta con un escalón corto al final (para apuntar al centro del vano),
/// lo quita y entra derecho por otro punto del vano. `at_end`: la puerta es el último punto (si no, el primero).
pub(crate) fn slide_into_door(pts: &mut Vec<Pt>, a: Pt, b: Pt, at_end: bool) {
    if !at_end {
        pts.reverse();
    }
    let eps = 1.0;
    let n = pts.len();
    if n >= 4 {
        let (p3, p2, p1, p0) = (pts[n - 4], pts[n - 3], pts[n - 2], pts[n - 1]);
        let h = |u: Pt, v: Pt| (u.y - v.y).abs() < eps;
        let v = |u: Pt, w: Pt| (u.x - w.x).abs() < eps;
        // p3→p2 y p1→p0 en el mismo eje, p2→p1 es el escalón corto
        let same_axis = (v(p3, p2) && v(p1, p0) && h(p2, p1)) || (h(p3, p2) && h(p1, p0) && v(p2, p1));
        if same_axis && p2.dist(p1) < 1200.0 {
            // punto del vano alineado con el tramo p3→p2
            let d = b - a;
            let l2 = d.x * d.x + d.y * d.y;
            if l2 > 1.0 {
                let vertical_leg = v(p3, p2);
                // intersección de la recta del tramo con la línea de la puerta
                let t = if vertical_leg {
                    if d.x.abs() > 1e-6 { (p2.x - a.x) / d.x } else { f64::NAN }
                } else if d.y.abs() > 1e-6 {
                    (p2.y - a.y) / d.y
                } else {
                    f64::NAN
                };
                let margin = (150.0 / l2.sqrt()).min(0.45);
                if t.is_finite() && t >= margin && t <= 1.0 - margin {
                    let q = a + d * t;
                    pts.truncate(n - 3);
                    pts.push(if vertical_leg { Pt { x: p2.x, y: q.y } } else { Pt { x: q.x, y: p2.y } });
                } else if t.is_finite() && n >= 5 {
                    // el tramo pasa justo al lado del vano: correrlo un poco (≤ 0,6 m) para entrar derecho
                    let tc = t.clamp(margin, 1.0 - margin);
                    let q = a + d * tc;
                    let shift = if vertical_leg { (q.x - p2.x).abs() } else { (q.y - p2.y).abs() };
                    if shift <= 600.0 {
                        let mv = |p: Pt| if vertical_leg { Pt { x: q.x, y: p.y } } else { Pt { x: p.x, y: q.y } };
                        let (np3, np2) = (mv(p3), mv(p2));
                        pts.truncate(n - 4);
                        pts.push(np3);
                        pts.push(if vertical_leg { Pt { x: q.x, y: q.y } } else { Pt { x: q.x, y: q.y } });
                        let _ = np2;
                    }
                }
            }
        }
    }
    if !at_end {
        pts.reverse();
    }
}

/// Redibuja un recorrido (ya calculado) solo con tramos horizontales y verticales.
/// Desde cada punto salta al más lejano del recorrido que se pueda unir con una recta, una L o
/// una S (Z) por celdas libres, sin tramos cortos; así quedan pocas vueltas y nada de escalones.
/// `first`/`last`: eje con el que sale de la entrada y llega a la oficina (perpendicular a la puerta).
pub(crate) fn orthogonalize(f: &crate::raster::Frame, comfy: &[bool], pass: &[bool], pts: &[Pt], first: Option<bool>, last: Option<bool>) -> Vec<Pt> {
    if pts.len() < 2 {
        return pts.to_vec();
    }
    let (p0, pn) = (pts[0], pts[pts.len() - 1]);
    let step = f.res * 0.5;
    let near_door = |q: Pt| q.dist(p0) < f.res * 2.5 || q.dist(pn) < f.res * 2.5;
    let see = |ok: &[bool], a: Pt, b: Pt| -> bool {
        let n = ((a.dist(b) / step).ceil() as usize).max(1);
        (0..=n).all(|k| {
            let q = a.lerp(b, k as f64 / n as f64);
            near_door(q) || f.cell_idx(q).map(|i| ok[i]).unwrap_or(false)
        })
    };
    let eps = 1.0;
    let is_h = |a: Pt, b: Pt| (a.y - b.y).abs() < eps;
    let ok_path = |ok: &[bool], ps: &[Pt]| ps.windows(2).all(|w| see(ok, w[0], w[1]));
    // formas candidatas de a a b (sin a); `fa`/`la`: eje exigido al primer/último tramo
    let shapes = |a: Pt, b: Pt, fa: Option<bool>, la: Option<bool>, min_leg: f64| -> Vec<Vec<Pt>> {
        let mut out: Vec<Vec<Pt>> = Vec::new();
        let (dx, dy) = ((b.x - a.x).abs(), (b.y - a.y).abs());
        let axis_ok = |ps: &[Pt]| -> bool {
            let mut prev = a;
            let mut legs = Vec::new();
            for &q in ps {
                legs.push((is_h(prev, q), prev.dist(q)));
                prev = q;
            }
            let fine = legs.iter().all(|&(_, l)| l >= min_leg || l < eps);
            let f_ok = fa.map_or(true, |v| legs.first().map_or(true, |&(h, l)| l < eps || h == v));
            let l_ok = la.map_or(true, |v| legs.last().map_or(true, |&(h, l)| l < eps || h == v));
            fine && f_ok && l_ok
        };
        if dx < eps || dy < eps {
            if axis_ok(&[b]) { out.push(vec![b]); }
            return out;
        }
        for h in [true, false] {
            let c = if h { Pt { x: b.x, y: a.y } } else { Pt { x: a.x, y: b.y } };
            let ps = vec![c, b];
            if axis_ok(&ps) { out.push(ps); }
        }
        // S: horizontal-vertical-horizontal (tramo vertical en x = t) o al revés; t desde el medio hacia afuera
        let nt = 24;
        for k in 0..=nt {
            let off = if k % 2 == 0 { k as f64 / 2.0 } else { -((k as f64 + 1.0) / 2.0) } / nt as f64;
            let r = 0.5 + off;
            if !(0.0..=1.0).contains(&r) { continue; }
            let tx = a.x + (b.x - a.x) * r;
            let ps = vec![Pt { x: tx, y: a.y }, Pt { x: tx, y: b.y }, b];
            if axis_ok(&ps) { out.push(ps); }
            let ty = a.y + (b.y - a.y) * r;
            let ps = vec![Pt { x: a.x, y: ty }, Pt { x: b.x, y: ty }, b];
            if axis_ok(&ps) { out.push(ps); }
        }
        out
    };
    let last_i = pts.len() - 1;
    let mut out = vec![p0];
    let mut i = 0;
    let mut prev_h: Option<bool> = None;
    while i < last_i {
        let a = *out.last().unwrap();
        let mut done = false;
        'outer: for min_leg in [600.0, 250.0, 0.0] {
            for ok in [comfy, pass] {
                for j in (i + 1..=last_i).rev() {
                    let fa = if i == 0 { first } else { None };
                    let la = if j == last_i { last } else { None };
                    let mut cands = shapes(a, pts[j], fa, la, min_leg);
                    // seguir en el mismo eje que el tramo anterior ahorra una vuelta
                    if let Some(ph) = prev_h {
                        cands.sort_by_key(|ps| (ps.len(), if is_h(a, ps[0]) == ph { 0 } else { 1 }));
                    } else {
                        cands.sort_by_key(|ps| ps.len());
                    }
                    if let Some(ps) = cands.into_iter().find(|ps| { let mut full = vec![a]; full.extend_from_slice(ps); ok_path(ok, &full) }) {
                        out.extend_from_slice(&ps);
                        i = j;
                        done = true;
                        break 'outer;
                    }
                }
            }
        }
        if !done {
            // sin forma ortogonal posible: se deja el tramo como está
            out.push(pts[i + 1]);
            i += 1;
        }
        let l = out.len();
        if l >= 2 && out[l - 1].dist(out[l - 2]) >= eps {
            prev_h = Some(is_h(out[l - 2], out[l - 1]));
        }
    }
    out.dedup_by(|a, b| a.dist(*b) < eps);
    let mut r: Vec<Pt> = Vec::with_capacity(out.len());
    for p in out {
        while r.len() >= 2 {
            let (a, b) = (r[r.len() - 2], r[r.len() - 1]);
            let col = ((a.x - b.x).abs() < eps && (b.x - p.x).abs() < eps) || ((a.y - b.y).abs() < eps && (b.y - p.y).abs() < eps);
            if col { r.pop(); } else { break; }
        }
        r.push(p);
    }
    // quitar escalones cortos en medio (< 0,35 m): correr uno de los tramos vecinos para alinearlos
    let merge = |v: Vec<Pt>| -> Vec<Pt> {
        let mut r: Vec<Pt> = Vec::with_capacity(v.len());
        for p in v {
            if r.last().map_or(false, |q: &Pt| q.dist(p) < eps) { continue; }
            while r.len() >= 2 {
                let (a, b) = (r[r.len() - 2], r[r.len() - 1]);
                let col = ((a.x - b.x).abs() < eps && (b.x - p.x).abs() < eps) || ((a.y - b.y).abs() < eps && (b.y - p.y).abs() < eps);
                if col { r.pop(); } else { break; }
            }
            r.push(p);
        }
        r
    };
    for _ in 0..8 {
        let mut changed = false;
        let l = r.len();
        for i in 1..l.saturating_sub(2) {
            let (a, b) = (r[i], r[i + 1]);
            if a.dist(b) >= 350.0 { continue; }
            let jog_h = is_h(a, b);
            // opción 1: mover el tramo anterior (r[i-1], r[i]) a la coordenada de b
            let shift = |p: Pt, to: Pt| if jog_h { Pt { x: to.x, y: p.y } } else { Pt { x: p.x, y: to.y } };
            if i >= 2 {
                let (q0, q1) = (shift(r[i - 1], b), b);
                if see(pass, r[i - 2], q0) && see(pass, q0, q1) {
                    r[i - 1] = q0;
                    r[i] = q1;
                    changed = true;
                    break;
                }
            }
            // opción 2: mover el tramo siguiente (r[i+1], r[i+2]) a la coordenada de a
            if i + 3 < l {
                let (q1, q2) = (a, shift(r[i + 2], a));
                if see(pass, q1, q2) && see(pass, q2, r[i + 3]) {
                    r[i + 1] = q1;
                    r[i + 2] = q2;
                    changed = true;
                    break;
                }
            }
        }
        if !changed { break; }
        r = merge(r);
    }
    r
}

/// Huella de un resultado para comparar parecido: celdas de 600 mm ocupadas por escritorios.
fn footprint(l: &Layout) -> std::collections::HashSet<(i64, i64)> {
    l.seats.iter().map(|s| ((s.pos.x / 600.0).round() as i64, (s.pos.y / 600.0).round() as i64)).collect()
}

/// Decodifica, rellena huecos y arma el resultado de cada genoma del conjunto;
/// devuelve hasta `n` resultados distintos, el mejor primero.
pub fn finish(ctx: &Ctx, pool: &[(f64, Genome)], weights: &Weights, n: usize, fill_secs: f64, cancel: &AtomicBool) -> Vec<Layout> {
    if pool.is_empty() {
        return vec![];
    }
    // candidatos: los mejores del conjunto sin repetir disposición, más variantes del mejor
    // (otro pasillo principal, filas giradas) para que haya alternativas realmente distintas
    let want = (n * 2).clamp(4, 24);
    let mut picks: Vec<Genome> = Vec::new();
    let mut prints: Vec<std::collections::HashSet<(i64, i64)>> = Vec::new();
    let fp_dec = |g: &Genome| -> std::collections::HashSet<(i64, i64)> {
        let d = decode(ctx, g, false);
        d.modules.iter().map(|m| ((m.x0 / 600.0).round() as i64 + 100_000 * m.grid as i64, (m.y0 / 600.0).round() as i64)).collect()
    };
    let similar = |a: &std::collections::HashSet<(i64, i64)>, b: &std::collections::HashSet<(i64, i64)>| {
        let inter = a.intersection(b).count() as f64;
        let uni = (a.len() + b.len()) as f64 - inter;
        uni <= 0.0 || inter / uni > 0.95
    };
    let mut offer = |g: Genome, picks: &mut Vec<Genome>| {
        if picks.len() >= want {
            return;
        }
        let fp = fp_dec(&g);
        if prints.iter().any(|p| similar(p, &fp)) {
            return;
        }
        prints.push(fp);
        picks.push(g);
    };
    for (_, g) in pool.iter() {
        offer(g.clone(), &mut picks);
    }
    let best = pool[0].1.clone();
    for v in 0..ctx.variants.len() {
        let mut g = best.clone();
        g.variant = v;
        offer(g.clone(), &mut picks);
        // filas giradas 90° en todas las zonas
        let na = ctx.variants[v].grids.len().max(1);
        for zg in g.vars[v].zones.iter_mut() {
            zg.theta = ((zg.theta as usize + 1) % na) as u8;
        }
        offer(g.clone(), &mut picks);
        for zg in g.vars[v].zones.iter_mut() {
            zg.from_top = !zg.from_top;
        }
        offer(g, &mut picks);
    }
    let run = |g: &Genome, until: Instant| -> (f64, Layout) {
        let mut d = decode(ctx, g, true);
        let ev = if cancel.load(Ordering::Relaxed) {
            evaluate(ctx, &d, g.variant, weights, false).0
        } else {
            super::fill::fill_gaps(ctx, &mut d, g.variant, weights, until)
        };
        let l = to_layout_from(ctx, &d, g.variant, serde_json::to_value(g).ok(), weights);
        (ev.score, l)
    };
    // el mejor recibe la mitad del tiempo de relleno; el resto se reparte en tandas paralelas
    let first = run(&picks[0], Instant::now() + crate::time::Duration::from_secs_f64((fill_secs * 0.5).max(0.3)));
    let rest = &picks[1..];
    let waves = rest.len().div_ceil(rayon::current_num_threads().max(1)).max(1);
    let per = crate::time::Duration::from_secs_f64((fill_secs * 0.5 / waves as f64).max(0.15));
    let mut outs: Vec<(f64, Layout)> = rest.par_iter().map(|g| run(g, Instant::now() + per)).collect();
    outs.push(first);
    // orden para mostrar: primero las que ubican todos los fijos y cumplen las reglas duras;
    // entre ellas, más puestos primero (y a igual cantidad, mejor puntaje)
    let missing = |l: &Layout| l.kpis.unplaced_by_type.iter().map(|x| x.1).sum::<u32>();
    outs.sort_by(|a, b| {
        (missing(&a.1), a.1.kpis.hard_violations.min(1))
            .cmp(&(missing(&b.1), b.1.kpis.hard_violations.min(1)))
            .then(b.1.kpis.seats_placed.cmp(&a.1.kpis.seats_placed))
            .then(a.0.partial_cmp(&b.0).unwrap())
    });
    let mut chosen: Vec<Layout> = Vec::new();
    let mut prints: Vec<std::collections::HashSet<(i64, i64)>> = Vec::new();
    for (_, l) in outs {
        let fp = footprint(&l);
        let similar = prints.iter().any(|p| {
            let inter = p.intersection(&fp).count() as f64;
            let uni = (p.len() + fp.len()) as f64 - inter;
            uni > 0.0 && inter / uni > 0.95
        });
        if similar {
            continue;
        }
        prints.push(fp);
        chosen.push(l);
        if chosen.len() >= n.max(1) {
            break;
        }
    }
    chosen
}

// ============================================================================ alto nivel

pub fn optimize(problem: &Problem, cancel: &AtomicBool, progress: ProgressFn) -> Result<Layout, String> {
    progress(Progress { phase: "Preparing the plan".into(), ..Default::default() });
    let ctx = Ctx::build(problem)?;
    let st = &problem.config.settings;
    // tiempo 0 = automático: busca hasta que deja de mejorar
    let budget = if st.time_limit > 0.0 { Budget::fixed(st.time_limit * 0.8) } else { Budget::auto(ctx.area_m2) };
    // «Seguir buscando»: arrancar desde el genoma de la solución actual (si sigue siendo compatible)
    let init: Option<Genome> = st.resume.as_ref().and_then(|v| serde_json::from_value::<Genome>(v.clone()).ok()).filter(|g| {
        g.vars.len() == ctx.variants.len() && g.vars.iter().zip(&ctx.variants).all(|(gv, cv)| gv.zones.len() == cv.zones.len() && gv.order.len() + 1 == cv.zones.len().max(1))
    });
    let t0 = Instant::now();
    let out = search_budget(&ctx, &problem.config.weights, budget, st.seed.max(1), init, cancel, progress, "Optimizing the layout");
    let spent = t0.elapsed().as_secs_f64();
    progress(Progress { phase: "Filling gaps and comparing layouts".into(), fraction: 0.85, seats_placed: out.eval.seats_placed, ..Default::default() });
    let fill = if st.time_limit > 0.0 { (st.time_limit * 0.2).max(1.0) } else { (spent * 0.25).clamp(1.5, 10.0) };
    let mut alts = finish(&ctx, &out.pool, &problem.config.weights, st.alternatives.clamp(1, 30) as usize, fill, cancel);
    if crate::assign::has_people(&problem.config.org) {
        progress(Progress { phase: "Assigning roles and clients".into(), fraction: 1.0, ..Default::default() });
        for l in alts.iter_mut() {
            crate::assign::assign(&ctx, &problem.config, l);
        }
    }
    let mut layout = alts[0].clone();
    if alts.len() > 1 {
        layout.alternatives = alts;
    }
    Ok(layout)
}

/// Exploración de pesos: varias corridas con pesos alrededor de los del usuario y frente de Pareto.
pub fn explore(problem: &Problem, n: usize, cancel: &AtomicBool, progress: ProgressFn) -> Result<Vec<crate::model::Variant>, String> {
    progress(Progress { phase: "Preparing the plan".into(), ..Default::default() });
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
    let each = if problem.config.settings.time_limit > 0.0 { (problem.config.settings.time_limit * 0.6).clamp(1.0, 10.0) } else { 4.0 };
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
            phase: "Exploring weights".into(),
            fraction: d as f64 / total as f64,
            best_score: l.kpis.score,
            seats_placed: l.kpis.seats_placed,
            seats_requested: l.kpis.seats_requested,
            iterations: d,
            message: format!("Variant {d} of {total}"),
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
            progress(Progress { phase: "Calculating capacity".into(), fraction: d as f64 / total as f64, message: t.name.clone(), ..Default::default() });
            Capacity { type_id: t.id.clone(), max_seats: seats, max_units: units }
        })
        .collect();
    let _ = ctx0;
    Ok(res)
}

/// Corrida completa con presupuesto corto: búsqueda + relleno de huecos. Devuelve el mejor resultado.
fn quick_run(p: &Problem, secs: f64, cancel: &AtomicBool) -> Option<Layout> {
    let ctx = Ctx::build(p).ok()?;
    let out = search(&ctx, &p.config.weights, secs * 0.75, 5, None, cancel, &|_| {}, "Probando");
    finish(&ctx, &out.pool, &p.config.weights, 1, secs * 0.25, cancel).into_iter().next()
}

/// «Buscar mejores valores»: prueba relajar cada medida de *Espacios* hasta su valor mínimo
/// razonable y cambiar de tipo de escritorio; cada prueba es una corrida completa (con el mismo
/// tiempo que la base), así que los puestos que se muestran son los de una distribución real.
pub fn suggest(problem: &Problem, cancel: &AtomicBool, progress: ProgressFn) -> Result<Vec<Suggestion>, String> {
    // medidas que deciden la capacidad (las de la pantalla Espacios)
    const ITEMS: [(&str, &str); 11] = [
        ("C1", "ancho"), ("C2", "ancho"), ("C2", "paso_sillas"), ("E1", "muro"), ("E2", "silla_silla"), ("E2", "silla_espalda"),
        ("E3", "corte"), ("C3", "profundidad"), ("C4", "extra"), ("O1", "dist"), ("O3", "dist"),
    ];
    let tl = problem.config.settings.time_limit;
    let secs = if tl > 0.0 { (tl * 0.35).clamp(1.5, 5.0) } else { 3.0 };
    let mut cands: Vec<(Suggestion, Problem)> = Vec::new();
    let sug = |rule_id: &str, item_id: &str, description: String, new_value: Option<f64>, disable: bool| Suggestion {
        rule_id: rule_id.into(), item_id: item_id.into(), description, new_value, disable, gain_seats: 0, seats_placed: 0, layout: None,
    };
    for (rid, iid) in ITEMS {
        let Some(ri) = problem.config.rules.iter().position(|r| r.id == rid) else { continue };
        let r = &problem.config.rules[ri];
        if !r.enabled || r.locked {
            continue;
        }
        let Some(ii) = r.items.iter().position(|i| i.id == iid) else { continue };
        let it = &r.items[ii];
        let (Some(v), Some(to)) = (it.value, it.relax_to) else { continue };
        if !it.enabled || (v - to).abs() < 1e-9 {
            continue;
        }
        let fmt = |x: f64| if it.unit == "mm" { format!("{:.2} m", x / 1000.0) } else { format!("{x} {}", it.unit) };
        let mut p = problem.clone();
        p.config.rules[ri].items[ii].value = Some(to);
        cands.push((sug(rid, iid, format!("{} · {}: from {} to {}", r.name, it.label, fmt(v), fmt(to)), Some(to), false), p));
    }
    if let Some(ri) = problem.config.rules.iter().position(|r| r.id == "C3" && r.enabled && !r.locked) {
        let mut p = problem.clone();
        p.config.rules[ri].enabled = false;
        cands.push((sug("C3", "", "No free area in front of the entrance".into(), None, true), p));
    }
    // cambio de tipo: probar los tipos con menos área por puesto
    let area_per_seat = |t: &DeskType| t.module_length(t.per_side_range().1) * t.module_depth() / t.seats_per_module(t.per_side_range().1).max(1) as f64;
    for d in &problem.config.demand {
        if d.quantity == 0 || d.max {
            continue;
        }
        let Some(t) = problem.config.catalog.iter().find(|t| t.id == d.type_id) else { continue };
        let mut alts: Vec<&DeskType> = problem.config.catalog.iter().filter(|a| a.id != t.id && area_per_seat(a) <= area_per_seat(t) * 1.1).collect();
        alts.sort_by(|a, b| b.flexible.cmp(&a.flexible).then(area_per_seat(a).partial_cmp(&area_per_seat(b)).unwrap()));
        for alt in alts.into_iter().take(2) {
            let mut p = problem.clone();
            let seats_t = if t.flexible { d.quantity } else { d.quantity * t.seats_per_module(t.per_side) };
            let qty_alt = if alt.flexible { seats_t } else { seats_t.div_ceil(alt.seats_per_module(alt.per_side).max(1)) };
            p.config.demand.retain(|x| x.type_id != t.id);
            if let Some(x) = p.config.demand.iter_mut().find(|x| x.type_id == alt.id) {
                x.quantity += qty_alt;
            } else {
                p.config.demand.push(Demand { type_id: alt.id.clone(), quantity: qty_alt, max: false });
            }
            cands.push((sug("TIPO", &format!("{}>{}", t.id, alt.id), format!("Use “{}” instead of “{}”", alt.name, t.name), None, false), p));
        }
    }
    let total = cands.len() + 1;
    progress(Progress { phase: "Measuring the current layout".into(), fraction: 0.0, message: format!("1 of {total}"), ..Default::default() });
    let base = quick_run(problem, secs, cancel).ok_or("The plan could not be optimized.")?;
    let base_seats = base.kpis.seats_placed as i32;
    let base_unplaced: u32 = base.kpis.unplaced_by_type.iter().map(|x| x.1).sum();
    let mut results = Vec::new();
    for (i, (mut s, p)) in cands.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        progress(Progress { phase: "Looking for better values".into(), fraction: (i + 1) as f64 / total as f64, seats_placed: base.kpis.seats_placed, message: s.description.clone(), ..Default::default() });
        let Some(l) = quick_run(&p, secs, cancel) else { continue };
        s.seats_placed = l.kpis.seats_placed;
        s.gain_seats = if s.rule_id == "TIPO" {
            base_unplaced as i32 - l.kpis.unplaced_by_type.iter().map(|x| x.1).sum::<u32>() as i32
        } else {
            l.kpis.seats_placed as i32 - base_seats
        };
        if s.gain_seats > 0 {
            s.layout = Some(l);
            results.push(s);
        }
    }
    results.sort_by(|a, b| b.gain_seats.cmp(&a.gain_seats));
    results.truncate(12);
    Ok(results)
}
