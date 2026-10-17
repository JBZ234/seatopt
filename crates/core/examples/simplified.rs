//! Plano simplificado: importar → detectar → exportar DXF y PDF mínimos → reimportar y optimizar.
use seat_core::{analyze::*, defaults::*, engine::search, export, import, model::*, pdf_out::*};
use std::sync::atomic::AtomicBool;

fn load(path: &str, bytes: &[u8]) -> (import::Drawing, std::collections::HashMap<String, LayerRole>, ScaleGuess, AnalyzeResult) {
    let d = import::import_bytes(path, bytes).unwrap();
    let roles = suggest_roles(&d);
    let sg = guess_scale(&d, &roles);
    let r = analyze(&d, &AnalyzeOptions { mm_per_unit: sg.mm_per_unit, roles: roles.clone(), seed: None, gap_close: 1200.0 }).unwrap();
    (d, roles, sg, r)
}

fn summary(tag: &str, r: &AnalyzeResult) {
    let k = |kind: DoorKind| r.plan.doors.iter().filter(|d| d.kind == kind).count();
    let o = |kind: ObstacleKind| r.plan.obstacles.iter().filter(|x| x.kind == kind).count();
    println!("{tag}: área {:.0} m² · puertas ppal {} salida {} oficina {} · columnas {} oficinas {} · recintos {} · edificio {} pts",
        r.area_m2, k(DoorKind::Main), k(DoorKind::Exit), k(DoorKind::Office), o(ObstacleKind::Column), o(ObstacleKind::FixedOffice), r.plan.rooms.len(), r.plan.building.len());
}

fn optimize(plan: &Plan, demand: &[(&str, u32)]) -> Layout {
    let mut cfg = default_config();
    cfg.demand = demand.iter().map(|(t, q)| Demand { type_id: t.to_string(), quantity: *q }).collect();
    cfg.settings.time_limit = 8.0;
    search::optimize(&Problem { plan: plan.clone(), config: cfg }, &AtomicBool::new(false), &search::no_progress).unwrap()
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (src, out, title) = (&a[1], &a[2], &a[3]);
    // puerta principal: la más cercana a este punto (mm), si se da
    let main_at: Option<(f64, f64)> = a.get(4).filter(|s| !s.is_empty()).map(|s| { let (x, y) = s.split_once(',').unwrap(); (x.parse().unwrap(), y.parse().unwrap()) });
    let demand: Vec<(String, u32)> = a.get(5).map(|s| s.split(',').map(|kv| { let (t, q) = kv.split_once('=').unwrap(); (t.to_string(), q.parse().unwrap()) }).collect()).unwrap_or_default();
    let demand: Vec<(&str, u32)> = demand.iter().map(|(t, q)| (t.as_str(), *q)).collect();

    let (_d, _roles, sg, mut r) = load(src, &std::fs::read(src).unwrap());
    if let Some((x, y)) = main_at {
        let p = seat_core::geom::pt(x, y);
        let best = r.plan.doors.iter().enumerate().min_by(|a, b| a.1.mid().dist(p).partial_cmp(&b.1.mid().dist(p)).unwrap()).map(|x| x.0).unwrap();
        for (i, dr) in r.plan.doors.iter_mut().enumerate() {
            if dr.kind == DoorKind::Main { dr.kind = DoorKind::Office; }
            if i == best { dr.kind = DoorKind::Main; dr.label = "ACCESO".into(); }
        }
    }
    println!("escala original: {:.4} mm/u ({})", sg.mm_per_unit, sg.reason);
    summary("original  ", &r);

    // exportar
    let dxf = export::plan_dxf(&r.plan, 1.0);
    std::fs::write(format!("{out}.dxf"), &dxf).unwrap();
    let opts = PdfOpts { paper: "A3".into(), scale: None, title: title.clone(), color_by: "type".into(), background: false, hidden_layers: vec![], clean: true, numbers: false };
    let pdf = render_pdf(Some(&r.plan), None, None, None, &opts).unwrap();
    std::fs::write(format!("{out}.pdf"), &pdf).unwrap();

    // reimportar
    let (_, roles_dxf, sg_dxf, r_dxf) = load(&format!("{out}.dxf"), dxf.as_bytes());
    println!("DXF reimportado: {:.3} mm/u ({}), capas {:?}", sg_dxf.mm_per_unit, sg_dxf.reason, { let mut v: Vec<String> = roles_dxf.iter().map(|(k, v)| format!("{k}={v:?}")).collect(); v.sort(); v });
    summary("desde DXF ", &r_dxf);
    diff_doors(&r.plan, &r_dxf.plan);
    let (_, _, sg_pdf, r_pdf) = load(&format!("{out}.pdf"), &pdf);
    println!("PDF reimportado: {:.3} mm/u ({})", sg_pdf.mm_per_unit, sg_pdf.reason);
    summary("desde PDF ", &r_pdf);

    if !demand.is_empty() {
        for (tag, p) in [("original", &r.plan), ("desde DXF", &r_dxf.plan), ("desde PDF", &r_pdf.plan)] {
            let l = optimize(p, &demand);
            println!("  optimizar {tag:<10}: {}/{} puestos, duras {}", l.kpis.seats_placed, l.kpis.seats_requested, l.kpis.hard_violations);
        }
    }
}

#[allow(dead_code)]
pub fn diff_doors(a: &Plan, b: &Plan) {
    for d in &b.doors {
        let m = a.doors.iter().map(|x| x.mid().dist(d.mid())).fold(f64::INFINITY, f64::min);
        if m > 300.0 {
            println!("   puerta extra {:?} mid=({:.0},{:.0}) w={:.0} (más cercana original a {:.0} mm)", d.kind, d.mid().x, d.mid().y, d.width(), m);
        }
    }
    for d in &a.doors {
        let m = b.doors.iter().map(|x| x.mid().dist(d.mid())).fold(f64::INFINITY, f64::min);
        if m > 300.0 {
            println!("   puerta perdida {:?} mid=({:.0},{:.0}) w={:.0}", d.kind, d.mid().x, d.mid().y, d.width());
        }
    }
}
