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
    cfg.demand = demand.iter().map(|(t, q)| Demand { type_id: t.to_string(), quantity: *q, max: false }).collect();
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
    cfg.demand = vec![Demand { type_id: "isla_8".into(), quantity: 2, max: false }, Demand { type_id: "pareja".into(), quantity: 6, max: false }];
    cfg.rules.iter_mut().find(|r| r.id == "E5").unwrap().enabled = false;
    cfg.settings.time_limit = 2.0;
    let l = search::optimize(&Problem { plan: r.plan, config: cfg }, &AtomicBool::new(false), &search::no_progress).unwrap();
    assert_eq!(l.kpis.seats_placed, 28);
}

#[test]
fn plano_simplificado_ida_y_vuelta() {
    use seat_core::{export, pdf_out};
    let path = format!("{}/../../samples/03_planta_L_oficinas.dxf", env!("CARGO_MANIFEST_DIR"));
    let d = import::import_bytes(&path, &std::fs::read(&path).unwrap()).unwrap();
    let roles = suggest_roles(&d);
    let r = analyze(&d, &AnalyzeOptions { mm_per_unit: 1.0, roles, seed: None, gap_close: 1200.0 }).unwrap();
    let count = |p: &Plan, k: DoorKind| p.doors.iter().filter(|x| x.kind == k).count();
    // DXF
    let dxf = export::plan_dxf(&r.plan, 1.0);
    let d2 = import::import_bytes("base.dxf", dxf.as_bytes()).unwrap();
    let ro2 = suggest_roles(&d2);
    let sg2 = guess_scale(&d2, &ro2);
    assert_eq!(sg2.mm_per_unit, 1.0);
    let r2 = analyze(&d2, &AnalyzeOptions { mm_per_unit: 1.0, roles: ro2, seed: None, gap_close: 1200.0 }).unwrap();
    assert!((r2.area_m2 - r.area_m2).abs() < r.area_m2 * 0.03, "área {} vs {}", r2.area_m2, r.area_m2);
    assert_eq!(count(&r2.plan, DoorKind::Main), 1);
    assert_eq!(count(&r2.plan, DoorKind::Exit), count(&r.plan, DoorKind::Exit));
    assert_eq!(r2.plan.doors.len(), r.plan.doors.len());
    // PDF (capas + escala guardada)
    let opts = pdf_out::PdfOpts { paper: "A3".into(), scale: None, title: "t".into(), color_by: "type".into(), background: false, hidden_layers: vec![], clean: true, numbers: false };
    let pdf = pdf_out::render_pdf(Some(&r.plan), None, None, None, &opts).unwrap();
    let d3 = import::import_bytes("base.pdf", &pdf).unwrap();
    let ro3 = suggest_roles(&d3);
    let sg3 = guess_scale(&d3, &ro3);
    assert!(sg3.reason.contains("guardada"));
    let r3 = analyze(&d3, &AnalyzeOptions { mm_per_unit: sg3.mm_per_unit, roles: ro3, seed: None, gap_close: 1200.0 }).unwrap();
    assert!((r3.area_m2 - r.area_m2).abs() < r.area_m2 * 0.03, "área {} vs {}", r3.area_m2, r.area_m2);
    assert_eq!(count(&r3.plan, DoorKind::Main), 1);
    assert_eq!(r3.plan.doors.len(), r.plan.doors.len());
}

#[test]
fn maximo_posible_supera_lo_fijo() {
    let path = format!("{}/../../samples/02_columnas_grilla.dxf", env!("CARGO_MANIFEST_DIR"));
    let d = import::import_bytes(&path, &std::fs::read(&path).unwrap()).unwrap();
    let roles = suggest_roles(&d);
    let sg = guess_scale(&d, &roles);
    let r = analyze(&d, &AnalyzeOptions { mm_per_unit: sg.mm_per_unit, roles, seed: None, gap_close: 1200.0 }).unwrap();
    let mut cfg = seat_core::defaults::default_config();
    cfg.demand = vec![
        Demand { type_id: "l_150".into(), quantity: 2, max: false },
        Demand { type_id: "isla_flex".into(), quantity: 0, max: true },
        Demand { type_id: "recto_120".into(), quantity: 0, max: true },
    ];
    cfg.settings.time_limit = 3.0;
    let l = seat_core::engine::search::optimize(&Problem { plan: r.plan, config: cfg }, &std::sync::atomic::AtomicBool::new(false), &seat_core::engine::search::no_progress).unwrap();
    assert!(l.kpis.max_mode);
    assert!(l.kpis.unplaced_by_type.is_empty(), "los fijos deben caber");
    assert_eq!(l.seats.iter().filter(|s| s.type_id == "l_150").count(), 2);
    assert!(l.kpis.max_seats >= 50, "máximo: {}", l.kpis.max_seats);
}

#[test]
fn lineas_simples_misma_area() {
    let path = format!("{}/../../samples/real_oficina401.pdf", env!("CARGO_MANIFEST_DIR"));
    let d = import::import_bytes(&path, &std::fs::read(&path).unwrap()).unwrap();
    let roles = suggest_roles(&d);
    let sg = guess_scale(&d, &roles);
    let opts = AnalyzeOptions { mm_per_unit: sg.mm_per_unit, roles, seed: None, gap_close: 1200.0 };
    let r = analyze(&d, &opts).unwrap();
    let sp = seat_core::simple::simple_plan(&d, &opts, &r.plan, &Default::default());
    let dxf = seat_core::simple::simple_dxf(&sp, 1.0);
    let d2 = import::import_bytes("x.dxf", dxf.as_bytes()).unwrap();
    let ro = suggest_roles(&d2);
    let r2 = analyze(&d2, &AnalyzeOptions { mm_per_unit: 1.0, roles: ro, seed: None, gap_close: 1200.0 }).unwrap();
    assert!((r2.area_m2 - r.area_m2).abs() / r.area_m2 < 0.05, "{} vs {}", r2.area_m2, r.area_m2);
    assert!(sp.walls.len() < 400);
}
