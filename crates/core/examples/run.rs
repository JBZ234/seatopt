use seat_core::{analyze::*, defaults::*, engine::search, import, model::*, render::*};
use std::sync::atomic::AtomicBool;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let path = &a[1];
    let out = &a[2];
    let demand: Vec<Demand> = a[3].split(',').map(|s| { let (t, q) = s.split_once('=').unwrap(); Demand { type_id: t.into(), quantity: q.parse().unwrap_or(0), max: q == "max" } }).collect();
    let time: f64 = a.get(4).map(|s| s.parse().unwrap()).unwrap_or(5.0);
    let d = import::import_bytes(path, &std::fs::read(path).unwrap()).unwrap();
    let roles = suggest_roles(&d);
    let sg = guess_scale(&d, &roles);
    let r = analyze(&d, &AnalyzeOptions { mm_per_unit: sg.mm_per_unit, roles, seed: None, gap_close: 1200.0 }).unwrap();
    let mut plan = r.plan;
    if std::env::var("DOORS").is_ok() { for (i, d) in plan.doors.iter().enumerate() { eprintln!("door {i}: {:?} mid=({:.0},{:.0}) w={:.0}", d.kind, d.mid().x, d.mid().y, d.width()); } }
    if let Some(m) = a.get(5) { // índice de puerta principal
        let (x, y) = m.split_once(',').unwrap();
        let p = seat_core::geom::pt(x.parse().unwrap(), y.parse().unwrap());
        let i = plan.doors.iter().enumerate().min_by(|a, b| a.1.mid().dist(p).partial_cmp(&b.1.mid().dist(p)).unwrap()).unwrap().0;
        for (j, dr) in plan.doors.iter_mut().enumerate() { if dr.kind == DoorKind::Main { dr.kind = DoorKind::Office; } if j == i { dr.kind = DoorKind::Main; } }
    }
    let mut cfg = default_config();
    cfg.demand = demand;
    cfg.settings.time_limit = time;
    let problem = Problem { plan: plan.clone(), config: cfg.clone() };
    let t = std::time::Instant::now();
    let cancel = AtomicBool::new(false);
    let layout = search::optimize(&problem, &cancel, &|p| eprintln!("  {} {:.0}% placed {}/{} it {}", p.phase, p.fraction*100.0, p.seats_placed, p.seats_requested, p.iterations)).unwrap();
    let k = &layout.kpis;
    println!("{:?} it={}: {}/{} puestos (máx {}), {} módulos, orient {}, luz {:.0}%, recorrido prom {:.1} m máx {:.1} m, reserva {}, m2/p {:.1}, hard {}, score {:.0}",
        t.elapsed(), 0, k.seats_placed, k.seats_requested, k.max_seats, k.modules_placed, k.orientations, k.daylight_pct, k.avg_travel_exit_m, k.max_travel_exit_m, k.reserve_seats, k.m2_per_seat, k.hard_violations, k.score);
    for v in &layout.violations { println!("  ! [{}] {}", v.rule_id, v.message); }
    println!("  alternativas: {:?}", layout.alternatives.iter().map(|l| l.kpis.seats_placed).collect::<Vec<_>>());
    println!("  sin ubicar: {:?}", k.unplaced_by_type);
    let svg = render_svg(&plan, Some(&layout), Some(&cfg), &RenderOpts { background: if path.ends_with(".pdf") { Some((&d, sg.mm_per_unit)) } else { None }, title: format!("{} — {}/{} puestos", path, k.seats_placed, k.seats_requested), ..Default::default() });
    std::fs::write(out, svg).unwrap();
}
