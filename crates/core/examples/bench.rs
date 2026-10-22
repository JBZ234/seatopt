use seat_core::{analyze::*, defaults::*, engine::{ctx::Ctx, decode::*, eval::evaluate}, import, model::*};
use std::time::Instant;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let d = import::import_bytes(&a[1], &std::fs::read(&a[1]).unwrap()).unwrap();
    let roles = suggest_roles(&d);
    let sg = guess_scale(&d, &roles);
    let r = analyze(&d, &AnalyzeOptions { mm_per_unit: sg.mm_per_unit, roles, seed: None, seeds: vec![], gap_close: 1200.0 }).unwrap();
    let mut cfg = default_config();
    cfg.demand = a[2].split(',').map(|s| { let (t, q) = s.split_once('=').unwrap(); Demand { type_id: t.into(), quantity: q.parse().unwrap(), max: false } }).collect();
    let pr = Problem { plan: r.plan, config: cfg.clone() };
    let t = Instant::now(); let ctx = Ctx::build(&pr).unwrap(); println!("ctx build {:?}  res {} cells {} zones {:?} angles {}", t.elapsed(), ctx.res, ctx.f.len(), ctx.variants.iter().map(|v| v.zones.len()).collect::<Vec<_>>(), ctx.angles.len());
    let mut rng = Rng(12345);
    let g = random_genome(&ctx, &mut rng);
    let t = Instant::now(); let mut dd = None; for _ in 0..30 { dd = Some(decode(&ctx, &g, true)); } println!("decode {:?} ({} modules)", t.elapsed() / 30, dd.as_ref().unwrap().modules.len());
    let dd = dd.unwrap();
    let t = Instant::now(); for _ in 0..30 { let _ = evaluate(&ctx, &dd, 0, &cfg.weights, false); } println!("eval {:?}", t.elapsed() / 30);
}
