//! Plano de líneas simples del original: muros a eje, columnas, puertas y oficinas.
//! uso: simple_lines <entrada> <salida sin extensión> <título> [x,y puerta principal mm]
use seat_core::{analyze::*, import, model::*, pdf_out::*, simple::*};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (src, out, title) = (&a[1], &a[2], &a[3]);
    let d = import::import_bytes(src, &std::fs::read(src).unwrap()).unwrap();
    let roles = suggest_roles(&d);
    let sg = guess_scale(&d, &roles);
    let opts = AnalyzeOptions { mm_per_unit: sg.mm_per_unit, roles, seed: None, seeds: vec![], gap_close: 1200.0 };
    let mut r = analyze(&d, &opts).unwrap();
    if let Some(s) = a.get(4) {
        let (x, y) = s.split_once(',').unwrap();
        let p = seat_core::geom::pt(x.parse().unwrap(), y.parse().unwrap());
        let best = r.plan.doors.iter().enumerate().min_by(|a, b| a.1.mid().dist(p).partial_cmp(&b.1.mid().dist(p)).unwrap()).map(|x| x.0).unwrap();
        for (i, dr) in r.plan.doors.iter_mut().enumerate() {
            if dr.kind == DoorKind::Main { dr.kind = DoorKind::Office; }
            if i == best { dr.kind = DoorKind::Main; dr.label = "ACCESO".into(); }
        }
    }
    let t = std::time::Instant::now();
    let sp = simple_plan(&d, &opts, &r.plan, &SimpleOpts::default());
    let segs: usize = sp.walls.iter().map(|w| w.len() - 1).sum();
    println!("{:?}: {} polilíneas de muro ({} tramos), {} columnas, {} puertas, {} rótulos", t.elapsed(), sp.walls.len(), segs, sp.columns.len(), sp.doors.len(), sp.labels.len());
    std::fs::write(format!("{out}.dxf"), simple_dxf(&sp, 1.0)).unwrap();
    let dd = simple_drawing(&sp, title);
    for paper in ["A3", "A1"] {
        let o = PdfOpts { paper: paper.into(), scale: None, title: title.clone(), color_by: "type".into(), background: true, hidden_layers: vec![], clean: true, numbers: false };
        std::fs::write(format!("{out}_{paper}.pdf"), render_pdf(None, None, None, Some((&dd, 1.0)), &o).unwrap()).unwrap();
    }
    if std::env::var("OVERLAY").is_ok() {
        // original en gris + líneas simples encima, para revisar
        let mut ov = simple_drawing(&sp, "overlay");
        let g = ov.layer_id("ORIGINAL", "#bbbbbb");
        for p in &d.paths {
            let role = opts.roles.get(&d.layers[p.layer as usize].name).copied().unwrap_or(LayerRole::Ignore);
            if matches!(role, LayerRole::Wall | LayerRole::Outline | LayerRole::Window | LayerRole::Column) {
                ov.add_path(g, p.pts.iter().map(|&q| q * sg.mm_per_unit).collect(), p.closed);
            }
        }
        ov.layers[0].color = "#e02020".into();
        let o = PdfOpts { paper: "A1".into(), scale: None, title: "overlay".into(), color_by: "type".into(), background: true, hidden_layers: vec![], clean: false, numbers: false };
        std::fs::write(format!("{out}_overlay.pdf"), render_pdf(None, None, None, Some((&ov, 1.0)), &o).unwrap()).unwrap();
    }
    for f in [format!("{out}.dxf"), format!("{out}_A3.pdf"), format!("{out}_A1.pdf")] {
        let d2 = import::import_bytes(&f, &std::fs::read(&f).unwrap()).unwrap();
        let ro = suggest_roles(&d2);
        let sg2 = guess_scale(&d2, &ro);
        let r2 = analyze(&d2, &AnalyzeOptions { mm_per_unit: sg2.mm_per_unit, roles: ro, seed: None, seeds: vec![], gap_close: 1200.0 }).unwrap();
        let m = r2.plan.doors.iter().filter(|x| x.kind == DoorKind::Main).count();
        println!("{f}: {:.3} mm/u · {:.0} m² · {} puertas ({} ppal) · {} obstáculos", sg2.mm_per_unit, r2.area_m2, r2.plan.doors.len(), m, r2.plan.obstacles.len());
    }
}
