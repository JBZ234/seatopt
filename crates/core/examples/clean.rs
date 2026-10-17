use seat_core::{analyze::*, import, pdf_out::*};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let d = import::import_bytes(&a[1], &std::fs::read(&a[1]).unwrap()).unwrap();
    let roles = suggest_roles(&d);
    let sg = guess_scale(&d, &roles);
    let exclude: Vec<String> = a[3].split(',').map(String::from).collect();
    let dxf = clean_dxf(&d, sg.mm_per_unit, 1.0, &exclude);
    std::fs::write(format!("{}.dxf", a[2]), dxf).unwrap();
    for paper in ["A3", "A1"] {
        let opts = PdfOpts { paper: paper.into(), scale: None, title: a[4].clone(), color_by: "type".into(), background: true, hidden_layers: exclude.clone(), clean: true, numbers: false };
        let pdf = render_pdf(None, None, None, Some((&d, sg.mm_per_unit)), &opts).unwrap();
        std::fs::write(format!("{}_{}.pdf", a[2], paper), pdf).unwrap();
    }
}
