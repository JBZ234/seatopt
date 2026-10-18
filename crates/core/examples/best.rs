//! Busca la mejor distribución probando varias combinaciones de medidas y orden; guarda el mejor
//! resultado como proyecto (.puestos.json) y SVG.
//! uso: best <plano> <salida sin extensión> <tipo=max,...> [segundos por corrida]
use seat_core::{analyze::*, defaults::*, engine::search, import, model::*, render::*};
use std::sync::atomic::AtomicBool;

fn set(cfg: &mut Config, rid: &str, iid: &str, v: f64) {
    if let Some(r) = cfg.rules.iter_mut().find(|r| r.id == rid) {
        if let Some(i) = r.items.iter_mut().find(|i| i.id == iid) {
            i.value = Some(v);
        }
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (path, out) = (&a[1], &a[2]);
    let demand: Vec<Demand> = a[3].split(',').map(|s| { let (t, q) = s.split_once('=').unwrap(); Demand { type_id: t.into(), quantity: q.parse().unwrap_or(0), max: q == "max" } }).collect();
    let secs: f64 = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let d = import::import_bytes(path, &std::fs::read(path).unwrap()).unwrap();
    let roles = suggest_roles(&d);
    let sg = guess_scale(&d, &roles);
    let r = analyze(&d, &AnalyzeOptions { mm_per_unit: sg.mm_per_unit, roles: roles.clone(), seed: None, gap_close: 1200.0 }).unwrap();
    let plan = r.plan;
    // (nombre, E2 silla-silla, E2 silla-espalda, E1, E3 max, orden)
    let combos: &[(&str, f64, f64, f64, f64, f64)] = &[
        ("compacto 1,5 sin pasillo principal", 1500.0, 1000.0, 900.0, 8.0, 1.0),
        ("estándar 1,6 sin pasillo principal", 1600.0, 1100.0, 900.0, 8.0, 1.0),
    ];
    let mut best: Option<(u32, String, Layout, Config)> = None;
    for (seed, &(name, ss, se, e1, e3, order)) in combos.iter().enumerate() {
        for rep in 0..1u64 {
            let mut cfg = default_config();
            cfg.demand = demand.clone();
            set(&mut cfg, "E2", "silla_silla", ss);
            set(&mut cfg, "E2", "silla_espalda", se);
            set(&mut cfg, "E1", "muro", e1);
            set(&mut cfg, "E3", "max", e3);
            if ss < 1550.0 { set(&mut cfg, "C2", "paso_sillas", 500.0); }
            cfg.weights.order = order;
            if name.contains("sin pasillo") { if let Some(r) = cfg.rules.iter_mut().find(|r| r.id == "C1") { r.enabled = false; } }
            cfg.settings.time_limit = secs;
            cfg.settings.seed = 11 + seed as u64 * 7 + rep * 101;
            cfg.settings.alternatives = 3;
            let l = search::optimize(&Problem { plan: plan.clone(), config: cfg.clone() }, &AtomicBool::new(false), &search::no_progress).unwrap();
            let k = &l.kpis;
            println!("{name:<30} rep {rep}: {} puestos, {} escritorios, reglas duras {}", k.seats_placed, k.modules_placed, k.hard_violations);
            let ok = k.hard_violations == 0 && k.unplaced_by_type.is_empty();
            if ok && best.as_ref().map(|b| k.seats_placed > b.0).unwrap_or(true) {
                best = Some((k.seats_placed, format!("{name} (rep {rep})"), l, cfg));
            }
        }
    }
    let (n, name, mut l, cfg) = best.expect("sin resultado");
    println!("MEJOR: {n} puestos con «{name}»");
    l.alternatives.clear();
    let svg = render_svg(&plan, Some(&l), Some(&cfg), &RenderOpts { title: format!("{} — {} puestos", path, n), ..Default::default() });
    std::fs::write(format!("{out}.svg"), svg).unwrap();
    let roles_map: std::collections::HashMap<String, LayerRole> = roles;
    let proj = serde_json::json!({
        "version": 1, "drawingName": std::path::Path::new(path).file_name().unwrap().to_string_lossy(),
        "mmPerUnit": sg.mm_per_unit, "roles": roles_map, "plan": plan, "config": cfg, "layout": l, "background": null,
    });
    std::fs::write(format!("{out}.puestos.json"), serde_json::to_string(&proj).unwrap()).unwrap();
}
