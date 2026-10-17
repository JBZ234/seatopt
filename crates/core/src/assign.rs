//! Asignación de personas (cliente + rol) a los puestos ya ubicados.
//! Costos unarios (rol ↔ tipo, rol cerca de un elemento, reglas personalizadas con objetivo fijo)
//! y por pares (bloques por cliente, afinidad entre roles, separación acústica),
//! optimizados con recocido simulado por intercambios.

use crate::defaults::RuleView;
use crate::engine::ctx::Ctx;
use crate::engine::custom::{self, SeatInfo};
use crate::engine::decode::Rng;
use crate::geom::*;
use crate::model::*;

pub fn has_people(org: &Organization) -> bool {
    org.teams.iter().any(|t| t.count > 0)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Kind {
    client: i32,
    role: i32,
}

const EMPTY: usize = usize::MAX;

pub fn assign(ctx: &Ctx, cfg: &Config, layout: &mut Layout) {
    let org = &cfg.org;
    let rv = RuleView::new(&cfg.rules);
    let w_group = cfg.weights.grouping.max(0.0) / 5.0;
    let ns = layout.seats.len();
    if ns == 0 {
        return;
    }
    // ------------------------------------------------ tipos de persona
    let mut kinds: Vec<Kind> = Vec::new();
    let mut slots: Vec<usize> = Vec::new(); // índice de tipo por persona
    for t in &org.teams {
        let role = org.roles.iter().position(|r| r.id == t.role_id).map(|x| x as i32).unwrap_or(-1);
        if role < 0 {
            continue;
        }
        let client = org.clients.iter().position(|c| c.id == t.client_id).map(|x| x as i32).unwrap_or(-1);
        let k = Kind { client, role };
        let ki = match kinds.iter().position(|x| *x == k) {
            Some(i) => i,
            None => {
                kinds.push(k);
                kinds.len() - 1
            }
        };
        for _ in 0..t.count {
            slots.push(ki);
        }
    }
    let people = slots.len();
    if people == 0 {
        return;
    }
    let nk = kinds.len();

    // ------------------------------------------------ costos unarios
    let r1_on = rv.on("R1");
    let r1_w = if rv.hard("R1") { 50.0 } else { rv.weight("R1") };
    let r4_on = rv.on("R4");
    let r4_w = rv.weight("R4");
    let org_rules: Vec<&CustomRule> = cfg.custom_rules.iter().filter(|r| r.enabled && custom::is_org_rule(r)).collect();
    let mut unary = vec![0.0f64; ns * nk];
    let mut unary_why: Vec<Vec<String>> = vec![Vec::new(); ns * nk];
    for (si, s) in layout.seats.iter().enumerate() {
        for (ki, k) in kinds.iter().enumerate() {
            let role = &org.roles[k.role as usize];
            let mut c = 0.0;
            if r1_on && !role.desk_types.is_empty() && !role.desk_types.contains(&s.type_id) {
                c += r1_w;
                unary_why[si * nk + ki].push(format!("{} no usa este tipo de escritorio", role.name));
            }
            if r4_on {
                for nt in &role.near {
                    if let Some((d, _)) = custom::target_nearest(ctx, None, &nt.target, s.pos) {
                        if d > nt.max_dist {
                            c += r4_w * ((d - nt.max_dist) / nt.max_dist.max(1000.0)).min(3.0);
                            unary_why[si * nk + ki].push(format!("{} lejos de su referencia", role.name));
                        }
                    }
                }
            }
            for r in &org_rules {
                if r.target.kind == TargetKind::Seats || matches!(r.predicate, Predicate::KeepTogether | Predicate::CountMax) {
                    continue;
                }
                let client_id = if k.client >= 0 { org.clients[k.client as usize].id.as_str() } else { "" };
                let info = SeatInfo { pos: s.pos, facing: s.facing, type_id: &s.type_id, role_id: &role.id, client_id };
                if !custom::matches(&r.subject, &info) {
                    continue;
                }
                let (bad, _) = custom::eval_rule(ctx, None, r, &[info], &[0]);
                if !bad.is_empty() {
                    c += if r.hard { 50.0 } else { r.weight };
                    unary_why[si * nk + ki].push(r.name.clone());
                }
            }
            unary[si * nk + ki] = c * w_group.max(0.2);
        }
    }

    // ------------------------------------------------ vecindad
    let r2 = if rv.on("R2") { rv.val_or("R2", "radio", 3000.0) } else { 0.0 };
    let r3 = if rv.on("R3") { rv.val_or("R3", "radio", 4000.0) } else { 0.0 };
    let r5 = if rv.on("R5") { rv.val_or("R5", "dist", 5000.0) } else { 0.0 };
    let rmax = r2.max(r3).max(r5).max(2500.0);
    let (w2, w3, w5) = (rv.weight("R2"), rv.weight("R3"), rv.weight("R5"));
    let mut aff = vec![0.0f64; org.roles.len() * org.roles.len()];
    for a in &org.affinities {
        let (Some(i), Some(j)) = (org.roles.iter().position(|r| r.id == a.a), org.roles.iter().position(|r| r.id == a.b)) else { continue };
        aff[i * org.roles.len() + j] = a.value;
        aff[j * org.roles.len() + i] = a.value;
    }
    let nr = org.roles.len();
    let mut nbr: Vec<Vec<(usize, f64)>> = vec![Vec::new(); ns];
    // rejilla espacial simple
    let cell = rmax;
    let mut buckets: std::collections::HashMap<(i64, i64), Vec<usize>> = std::collections::HashMap::new();
    for (i, s) in layout.seats.iter().enumerate() {
        buckets.entry(((s.pos.x / cell).floor() as i64, (s.pos.y / cell).floor() as i64)).or_default().push(i);
    }
    for (i, s) in layout.seats.iter().enumerate() {
        let (cx, cy) = ((s.pos.x / cell).floor() as i64, (s.pos.y / cell).floor() as i64);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(v) = buckets.get(&(cx + dx, cy + dy)) {
                    for &j in v {
                        if j != i {
                            let d = s.pos.dist(layout.seats[j].pos);
                            if d <= rmax {
                                nbr[i].push((j, d));
                            }
                        }
                    }
                }
            }
        }
    }
    // los puestos del mismo módulo son vecinos fuertes
    let pair_cost = |ka: usize, kb: usize, d: f64, same_module: bool| -> f64 {
        let (a, b) = (kinds[ka], kinds[kb]);
        let mut c = 0.0;
        if r2 > 0.0 && d <= r2 && a.client >= 0 && b.client >= 0 && a.client != b.client {
            c += w2 * if same_module { 1.5 } else { 1.0 };
        }
        if r2 > 0.0 && d <= r2 && a.client >= 0 && a.client == b.client {
            c -= w2 * 0.35;
        }
        if r3 > 0.0 && d <= r3 && (a.client == b.client || a.client < 0 || b.client < 0) {
            c -= w3 * aff[a.role as usize * nr + b.role as usize] * (1.0 - d / r3);
        }
        if r5 > 0.0 && d < r5 {
            let (na, nb) = (org.roles[a.role as usize].noise, org.roles[b.role as usize].noise);
            if (na == Noise::Loud && nb == Noise::Quiet) || (na == Noise::Quiet && nb == Noise::Loud) {
                c += w5 * (1.0 - d / r5) * 2.0;
            }
        }
        c * w_group
    };
    let module_of: Vec<&str> = layout.seats.iter().map(|s| s.module_id.as_str()).collect();

    // ------------------------------------------------ asignación inicial: bloques por cliente
    let mut seat_kind: Vec<usize> = vec![EMPTY; ns];
    let mut order_clients: Vec<i32> = kinds.iter().map(|k| k.client).collect();
    order_clients.sort();
    order_clients.dedup();
    let count_client = |c: i32| slots.iter().filter(|&&k| kinds[k].client == c).count();
    order_clients.sort_by_key(|&c| std::cmp::Reverse(count_client(c)));
    let mut free_seats: Vec<bool> = vec![true; ns];
    for &c in &order_clients {
        let mut need: Vec<usize> = slots.iter().copied().filter(|&k| kinds[k].client == c).collect();
        // roles con tipo preferido primero
        need.sort_by_key(|&k| std::cmp::Reverse(org.roles[kinds[k].role as usize].desk_types.len()));
        if need.is_empty() {
            continue;
        }
        // semilla: puesto libre con menos vecinos libres (esquina)
        let seed = (0..ns).filter(|&i| free_seats[i]).min_by_key(|&i| nbr[i].iter().filter(|(j, _)| free_seats[*j]).count());
        let Some(seed) = seed else { break };
        let sp = layout.seats[seed].pos;
        let mut cand: Vec<usize> = (0..ns).filter(|&i| free_seats[i]).collect();
        cand.sort_by(|&a, &b| layout.seats[a].pos.dist(sp).partial_cmp(&layout.seats[b].pos.dist(sp)).unwrap());
        for (k, &si) in need.iter().zip(cand.iter()) {
            seat_kind[si] = *k;
            free_seats[si] = false;
        }
    }

    // ------------------------------------------------ recocido por intercambios
    let local = |sk: &Vec<usize>, i: usize| -> f64 {
        let k = sk[i];
        if k == EMPTY {
            return 0.0;
        }
        let mut c = unary[i * nk + k];
        for &(j, d) in &nbr[i] {
            let kj = sk[j];
            if kj != EMPTY {
                c += pair_cost(k, kj, d, module_of[i] == module_of[j]);
            }
        }
        c
    };
    let mut rng = Rng(0x5EED_1234_ABCD | 1);
    let iters = (ns * 400).clamp(20_000, 600_000);
    let (t0, t1) = (2.0f64, 0.01f64);
    for it in 0..iters {
        let temp = t0 * (t1 / t0).powf(it as f64 / iters as f64);
        let a = rng.below(ns);
        // vecino o puesto aleatorio
        let b = if rng.f32() < 0.6 && !nbr[a].is_empty() { nbr[a][rng.below(nbr[a].len())].0 } else { rng.below(ns) };
        if a == b || seat_kind[a] == seat_kind[b] {
            continue;
        }
        let before = local(&seat_kind, a) + local(&seat_kind, b);
        seat_kind.swap(a, b);
        let after = local(&seat_kind, a) + local(&seat_kind, b);
        let delta = after - before;
        if delta > 0.0 && rng.f64() >= (-delta / temp).exp() {
            seat_kind.swap(a, b);
        }
    }

    // ------------------------------------------------ resultado
    let mut misplaced = 0u32;
    let mut same = 0usize;
    let mut diff = 0usize;
    for (i, s) in layout.seats.iter_mut().enumerate() {
        let k = seat_kind[i];
        if k == EMPTY {
            s.role_id.clear();
            s.client_id.clear();
            continue;
        }
        let kd = kinds[k];
        s.role_id = org.roles[kd.role as usize].id.clone();
        s.client_id = if kd.client >= 0 { org.clients[kd.client as usize].id.clone() } else { String::new() };
        let mut why = unary_why[i * nk + k].clone();
        if kd.client >= 0 && r2 > 0.0 {
            let mut has_mate = false;
            for &(j, d) in &nbr[i] {
                if d > r2 || seat_kind[j] == EMPTY {
                    continue;
                }
                let kc = kinds[seat_kind[j]].client;
                if kc < 0 {
                    continue;
                }
                if kc == kd.client {
                    has_mate = true;
                    same += 1;
                } else {
                    diff += 1;
                }
            }
            if !has_mate {
                why.push("Separado de su equipo".into());
            }
        }
        if !why.is_empty() {
            misplaced += 1;
            for w in why {
                if !s.flags.contains(&w) {
                    s.flags.push(w);
                }
            }
        }
    }
    layout.kpis.grouping_score = if same + diff > 0 { same as f64 / (same + diff) as f64 * 100.0 } else { 100.0 };
    layout.kpis.misplaced_people = misplaced;
    if people > ns {
        layout.violations.push(Violation { rule_id: "R".into(), message: format!("Hay {} personas sin puesto: se ubicaron {} de {}.", people - ns, ns, people), at: None, hard: false });
    }
    // reglas personalizadas por pares / grupos: solo informe
    let infos: Vec<SeatInfo> = layout
        .seats
        .iter()
        .map(|s| SeatInfo { pos: s.pos, facing: s.facing, type_id: &s.type_id, role_id: &s.role_id, client_id: &s.client_id })
        .collect();
    let active: Vec<usize> = (0..ns).filter(|&i| !layout.seats[i].role_id.is_empty()).collect();
    let mut extra = Vec::new();
    for r in &org_rules {
        if !(r.target.kind == TargetKind::Seats || matches!(r.predicate, Predicate::KeepTogether | Predicate::CountMax)) {
            continue;
        }
        let (bad, excess) = custom::eval_rule(ctx, None, r, &infos, &active);
        if !bad.is_empty() || excess > 0 {
            extra.push(Violation { rule_id: r.id.clone(), message: format!("{}: {} incumplimientos", r.name, bad.len() as u32 + excess), at: bad.first().map(|&i| layout.seats[i].pos), hard: r.hard });
        }
    }
    layout.violations.extend(extra);
}

/// Centro aproximado de un cliente (para etiquetas en la vista).
pub fn client_centers(layout: &Layout) -> Vec<(String, Pt)> {
    let mut acc: std::collections::BTreeMap<String, (Pt, usize)> = Default::default();
    for s in &layout.seats {
        if s.client_id.is_empty() {
            continue;
        }
        let e = acc.entry(s.client_id.clone()).or_insert((pt(0.0, 0.0), 0));
        e.0 = e.0 + s.pos;
        e.1 += 1;
    }
    acc.into_iter().map(|(k, (p, n))| (k, p * (1.0 / n as f64))).collect()
}
