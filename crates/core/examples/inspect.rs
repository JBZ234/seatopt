use seat_core::{analyze::*, import, render::*};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = &args[1];
    let out = &args[2];
    let bytes = std::fs::read(path).unwrap();
    let t = std::time::Instant::now();
    let d = import::import_bytes(path, &bytes).unwrap();
    println!("import {:?}: {} paths, {} curves, {} texts, {} layers", t.elapsed(), d.paths.len(), d.curves.len(), d.texts.len(), d.layers.len());
    let roles = suggest_roles(&d);
    for l in &d.layers { println!("  {:<22} {:>6}  -> {:?}", l.name, l.paths, roles[&l.name]); }
    let sg = guess_scale(&d, &roles);
    println!("scale: {} mm/u  {:?}  {}", sg.mm_per_unit, sg.paper_scale, sg.reason);
    let opts = AnalyzeOptions { mm_per_unit: sg.mm_per_unit, roles, seed: None, gap_close: 1200.0 };
    let t = std::time::Instant::now();
    let r = analyze(&d, &opts).unwrap();
    println!("analyze {:?}: area {:.1} m2, {:?}", t.elapsed(), r.area_m2, r.notes);
    for dr in &r.plan.doors { println!("  door {:?} w={:.0} {}", dr.kind, dr.width(), dr.label); }
    let svg = render_svg(&r.plan, None, None, &RenderOpts { background: Some((&d, sg.mm_per_unit)), ..Default::default() });
    std::fs::write(out, svg).unwrap();
}
