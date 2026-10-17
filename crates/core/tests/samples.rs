//! Pruebas de extremo a extremo con los planos de muestra.
use seat_core::{analyze::*, defaults::*, engine::search, import, model::*};
use std::sync::atomic::AtomicBool;

fn run(file: &str, demand: &[(&str, u32)], secs: f64) -> Layout {
    let path = format!("{}/../../samples/{file}", env!("CARGO_MANIFEST_DIR"));
    let d = import::import_bytes(&path, &std::fs::read(&path).unwrap()).unwrap();
    let roles = suggest_roles(&d);
    let sg = guess_scale(&d, &roles);
    let r = analyze(&d, &AnalyzeOptions { mm_per_unit: sg.mm_per_unit, roles, seed: None, gap_close: 1200.0 }).unwrap();
    let mut cfg = default_config();
    cfg.demand = demand.iter().map(|(t, q)| Demand { type_id: t.to_string(), quantity: *q }).collect();
    cfg.settings.time_limit = secs;
    search::optimize(&Problem { plan: r.plan, config: cfg }, &AtomicBool::new(false), &search::no_progress).unwrap()
}

#[test]
fn rectangular_ubica_todo() {
    let l = run("01_rectangular_simple.dxf", &[("isla_flex", 24)], 3.0);
    assert_eq!(l.kpis.seats_placed, 24);
    assert_eq!(l.kpis.hard_violations, 0);
}

#[test]
fn columnas_ubica_todo_sin_tocar_columnas() {
    let l = run("02_columnas_grilla.dxf", &[("isla_flex", 40), ("recto_120", 10)], 3.0);
    assert_eq!(l.kpis.seats_placed, 50);
}

#[test]
fn planta_l_con_oficinas() {
    let l = run("03_planta_L_oficinas.dxf", &[("isla_8", 4), ("l_150", 4), ("recto_120", 8)], 4.0);
    assert_eq!(l.kpis.seats_placed, 44);
    // ningún puesto sin acceso
    assert!(l.seats.iter().all(|s| !s.flags.iter().any(|f| f.starts_with("Sin acceso"))));
}

#[test]
fn angosto_reporta_faltantes() {
    let l = run("06_angosto_capacidad.dxf", &[("isla_flex", 30)], 2.0);
    assert!(l.kpis.seats_placed < 30);
    assert!(!l.kpis.unplaced_by_type.is_empty());
}

#[test]
fn pdf_real_con_capas() {
    let path = format!("{}/../../samples/real_oficina401.pdf", env!("CARGO_MANIFEST_DIR"));
    let d = import::import_bytes(&path, &std::fs::read(&path).unwrap()).unwrap();
    assert!(d.layers.iter().any(|l| l.name == "4-WALL"));
    let roles = suggest_roles(&d);
    let sg = guess_scale(&d, &roles);
    assert_eq!(sg.paper_scale, Some(75.0));
    let r = analyze(&d, &AnalyzeOptions { mm_per_unit: sg.mm_per_unit, roles, seed: None, gap_close: 1200.0 }).unwrap();
    assert!(r.area_m2 > 600.0 && r.area_m2 < 900.0);
    assert!(r.plan.doors.len() >= 10);
}

#[test]
fn e5_apagada_mezcla_tipos_en_la_fila() {
    let path = format!("{}/../../samples/01_rectangular_simple.dxf", env!("CARGO_MANIFEST_DIR"));
    let d = import::import_bytes(&path, &std::fs::read(&path).unwrap()).unwrap();
    let roles = suggest_roles(&d);
    let r = analyze(&d, &AnalyzeOptions { mm_per_unit: 1.0, roles, seed: None, gap_close: 1200.0 }).unwrap();
    let mut cfg = default_config();
    cfg.demand = vec![Demand { type_id: "isla_8".into(), quantity: 2 }, Demand { type_id: "pareja".into(), quantity: 6 }];
    cfg.rules.iter_mut().find(|r| r.id == "E5").unwrap().enabled = false;
    cfg.settings.time_limit = 2.0;
    let l = search::optimize(&Problem { plan: r.plan, config: cfg }, &AtomicBool::new(false), &search::no_progress).unwrap();
    assert_eq!(l.kpis.seats_placed, 28);
}
