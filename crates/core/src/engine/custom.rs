//! Reglas personalizadas: sujeto (selector de puestos) + predicado + objetivo + valor.

use super::ctx::{Ctx, Variant};
use super::eval::{WModule, WSeat};
use crate::geom::*;
use crate::model::*;

pub struct SeatInfo<'a> {
    pub pos: Pt,
    pub facing: Pt,
    pub type_id: &'a str,
    pub role_id: &'a str,
    pub client_id: &'a str,
}

pub fn matches(sel: &SeatSelector, s: &SeatInfo) -> bool {
    match sel.kind {
        SelectorKind::Any => true,
        SelectorKind::DeskType => s.type_id == sel.value,
        SelectorKind::Role => s.role_id == sel.value,
        SelectorKind::Client => s.client_id == sel.value,
    }
}

/// ¿La regla necesita roles/clientes? (entonces se evalúa en la asignación)
pub fn is_org_rule(r: &CustomRule) -> bool {
    let org = |s: &SeatSelector| matches!(s.kind, SelectorKind::Role | SelectorKind::Client);
    org(&r.subject) || r.target.seats.as_ref().map(org).unwrap_or(false)
}

/// Distancia y punto más cercano de un objetivo.
pub fn target_nearest(ctx: &Ctx, var: Option<&Variant>, t: &Target, p: Pt) -> Option<(f64, Pt)> {
    let mut best: Option<(f64, Pt)> = None;
    let mut consider = |d: f64, q: Pt| {
        if best.map(|b| d < b.0).unwrap_or(true) {
            best = Some((d, q));
        }
    };
    match t.kind {
        TargetKind::MainDoor | TargetKind::Exits | TargetKind::OfficeDoors => {
            for d in &ctx.plan.doors {
                let ok = match t.kind {
                    TargetKind::MainDoor => d.kind == DoorKind::Main,
                    TargetKind::Exits => matches!(d.kind, DoorKind::Exit | DoorKind::Main),
                    _ => d.kind == DoorKind::Office,
                };
                if ok {
                    let q = closest_on_segment(p, d.a, d.b);
                    consider(q.dist(p), q);
                }
            }
        }
        TargetKind::Windows => {
            for w in &ctx.plan.windows {
                let q = closest_on_segment(p, w.a, w.b);
                consider(q.dist(p), q);
            }
        }
        TargetKind::Columns | TargetKind::ObstacleLabel => {
            let needle = t.value.to_lowercase();
            for o in &ctx.plan.obstacles {
                let ok = match t.kind {
                    TargetKind::Columns => o.kind == ObstacleKind::Column,
                    _ => !needle.is_empty() && o.label.to_lowercase().contains(&needle),
                };
                if ok {
                    let d = dist_point_polygon(p, &o.poly);
                    consider(d, centroid(&o.poly));
                }
            }
        }
        TargetKind::MainCorridor => {
            if let Some(v) = var {
                if let Some(i) = ctx.f.cell_idx(p) {
                    let d = v.corridor_dist[i] as f64;
                    if d.is_finite() {
                        consider(d, p);
                    }
                }
            }
        }
        TargetKind::Seats => {}
    }
    best
}

/// Evalúa una regla; devuelve índices (en `infos`) de puestos que la incumplen y un exceso de conteo.
pub fn eval_rule(ctx: &Ctx, var: Option<&Variant>, r: &CustomRule, infos: &[SeatInfo], active: &[usize]) -> (Vec<usize>, u32) {
    let subj: Vec<usize> = active.iter().copied().filter(|&i| matches(&r.subject, &infos[i])).collect();
    let mut bad = Vec::new();
    match r.predicate {
        Predicate::CountMax => {
            let excess = (subj.len() as f64 - r.value).max(0.0) as u32;
            return (vec![], excess);
        }
        Predicate::KeepTogether => {
            for &i in &subj {
                let near = subj.iter().any(|&j| j != i && infos[i].pos.dist(infos[j].pos) <= r.value);
                if !near && subj.len() > 1 {
                    bad.push(i);
                }
            }
            return (bad, 0);
        }
        _ => {}
    }
    let tgt_seats: Vec<usize> = if r.target.kind == TargetKind::Seats {
        let sel = r.target.seats.clone().unwrap_or(SeatSelector { kind: SelectorKind::Any, value: String::new() });
        active.iter().copied().filter(|&i| matches(&sel, &infos[i])).collect()
    } else {
        vec![]
    };
    for &i in &subj {
        let s = &infos[i];
        let near = if r.target.kind == TargetKind::Seats {
            tgt_seats.iter().filter(|&&j| j != i).map(|&j| (s.pos.dist(infos[j].pos), infos[j].pos)).min_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
        } else {
            target_nearest(ctx, var, &r.target, s.pos)
        };
        let Some((d, q)) = near else { continue };
        let fail = match r.predicate {
            Predicate::DistMax => d > r.value,
            Predicate::DistMin => d < r.value,
            Predicate::FacingToward => {
                let dir = (q - s.pos).norm();
                d > 1.0 && s.facing.dot(dir) < 0.5
            }
            Predicate::NotBackTo => {
                let dir = (q - s.pos).norm();
                d <= r.value && s.facing.dot(dir) < -0.5
            }
            _ => false,
        };
        if fail {
            bad.push(i);
        }
    }
    (bad, 0)
}

#[allow(clippy::too_many_arguments)]
pub fn layout_penalty(
    ctx: &Ctx,
    mods: &[WModule],
    seats: &[WSeat],
    valid: &[usize],
    _module_ok: &[bool],
    var: &Variant,
    flags: &mut [Vec<String>],
    violations: &mut Vec<Violation>,
    hard: &mut u32,
) -> f64 {
    let rules: Vec<&CustomRule> = ctx.config.custom_rules.iter().filter(|r| r.enabled && !is_org_rule(r)).collect();
    if rules.is_empty() {
        return 0.0;
    }
    let infos: Vec<SeatInfo> = seats
        .iter()
        .map(|s| SeatInfo { pos: s.pos, facing: s.facing, type_id: &ctx.catalog[mods[s.module].pm.t].id, role_id: "", client_id: "" })
        .collect();
    let mut pen = 0.0;
    let vs = valid.len().max(1) as f64;
    for r in rules {
        let (bad, excess) = eval_rule(ctx, Some(var), r, &infos, valid);
        let cnt = bad.len() as u32 + excess;
        if cnt == 0 {
            continue;
        }
        for &i in &bad {
            flags[i].push(format!("Regla: {}", r.name));
        }
        violations.push(Violation { rule_id: r.id.clone(), message: format!("{}: {} incumplimientos", r.name, cnt), at: bad.first().map(|&i| infos[i].pos), hard: r.hard });
        if r.hard {
            *hard += cnt;
        } else {
            pen += r.weight * cnt as f64 / vs * 3.0;
        }
    }
    pen
}
