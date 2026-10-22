//! Compares the default (tidy) scoring against pure seat count on the sample plans.
//! `cargo run --release -p seat_core --example tidy -- 8`
use seat_core::analyze::{analyze, guess_scale, suggest_roles, AnalyzeOptions};
use seat_core::import;
use seat_core::model::*;
use std::sync::atomic::AtomicBool;

fn main() {
    let secs: f64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(6.0);
    for name in ["01_rectangular_simple", "02_columnas_grilla", "03_planta_L_oficinas", "04_openplan_nucleo", "05_irregular_diagonal", "06_angosto_capacidad"] {
        let path = format!("samples/{name}.dxf");
        let d = import::import_bytes(&path, &std::fs::read(&path).unwrap()).unwrap();
        let roles = suggest_roles(&d);
        let sg = guess_scale(&d, &roles);
        let r = analyze(&d, &AnalyzeOptions { mm_per_unit: sg.mm_per_unit, roles, seed: None, seeds: vec![], gap_close: 1200.0 }).unwrap();
        let mut line = format!("{name:24}");
        for (paths, tidy) in [(false, 0.0), (true, 0.0), (true, 1.2)] {
            let mut cfg = seat_core::defaults::default_config();
            cfg.settings.time_limit = secs;
            cfg.weights.tidy = tidy;
            let mut plan = r.plan.clone();
            if paths {
                let pr = Problem { plan: plan.clone(), config: cfg.clone() };
                plan.paths = seat_core::engine::circulation::compute(&pr).unwrap_or_default();
            }
            let l = seat_core::engine::search::optimize(&Problem { plan: plan.clone(), config: cfg }, &AtomicBool::new(false), &seat_core::engine::search::no_progress).unwrap();
            let bb = |m: &Module| { let (mut a, mut b, mut c, mut d) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN); for q in &m.body { a = a.min(q.x); b = b.min(q.y); c = c.max(q.x); d = d.max(q.y); } (a, b, c, d) };
            let lone = l.modules.iter().filter(|m| { let x = bb(m); !l.modules.iter().any(|o| o.id != m.id && { let y = bb(o); x.0 <= y.2 + 150.0 && y.0 <= x.2 + 150.0 && x.1 <= y.3 + 150.0 && y.1 <= x.3 + 150.0 }) }).count();
            if paths && tidy > 0.0 {
                let mut svg = String::new();
                let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
                for q in plan.region.iter().flatten() { x0 = x0.min(q.x); y0 = y0.min(q.y); x1 = x1.max(q.x); y1 = y1.max(q.y); }
                let poly = |pts: &[seat_core::geom::Pt]| pts.iter().map(|q| format!("{:.0},{:.0}", q.x, -q.y)).collect::<Vec<_>>().join(" ");
                svg += &format!("<svg xmlns='http://www.w3.org/2000/svg' viewBox='{} {} {} {}' width='1400'>", x0 - 500.0, -y1 - 500.0, x1 - x0 + 1000.0, y1 - y0 + 1000.0);
                for r in &plan.region { svg += &format!("<polygon points='{}' fill='#f4f1ea' stroke='#333' stroke-width='60'/>", poly(r)); }
                for o in &plan.obstacles { svg += &format!("<polygon points='{}' fill='#9ab'/>", poly(&o.poly)); }
                for c in plan.paths.iter().filter(|c| c.keep) { svg += &format!("<polyline points='{}' fill='none' stroke='rgba(230,140,40,.35)' stroke-width='{}' stroke-linejoin='round' stroke-linecap='round'/><polyline points='{}' fill='none' stroke='#d07000' stroke-width='40'/>", poly(&c.pts), c.width.unwrap_or(1200.0), poly(&c.pts)); }
                for m in &l.modules { svg += &format!("<polygon points='{}' fill='#5b7fa8'/>", poly(&m.body)); }
                for st in &l.seats { svg += &format!("<polygon points='{}' fill='#ccc'/>", poly(&st.chair)); }
                for d in &plan.doors { svg += &format!("<line x1='{:.0}' y1='{:.0}' x2='{:.0}' y2='{:.0}' stroke='{}' stroke-width='120'/>", d.a.x, -d.a.y, d.b.x, -d.b.y, match d.kind { DoorKind::Main => "green", DoorKind::Exit => "red", _ => "purple" }); }
                svg += "</svg>";
                std::fs::write(format!("/tmp/pw/{name}.svg"), svg).unwrap();
            }
            line += &format!(" | paths {} tidy {tidy}: {} seats, {} lone", paths as u8, l.kpis.seats_placed, lone);
        }
        println!("{line}");
    }
}
