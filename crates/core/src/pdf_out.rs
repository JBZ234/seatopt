//! Exportación a PDF vectorial a escala (layout final o plano limpio).

use crate::geom::*;
use crate::import::Drawing;
use crate::model::*;
use lopdf::{dictionary, Document, Object, Stream};
use serde::{Deserialize, Serialize};
use std::fmt::Write;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfOpts {
    /// "A4" | "A3" | "A2" | "A1" | "A0"
    #[serde(default = "a3")]
    pub paper: String,
    /// Escala 1:N; None = la estándar más grande que quepa.
    #[serde(default)]
    pub scale: Option<f64>,
    #[serde(default)]
    pub title: String,
    /// "type" | "role" | "client"
    #[serde(default = "ty")]
    pub color_by: String,
    /// Dibujar el plano de fondo (capas del archivo original).
    #[serde(default = "yes")]
    pub background: bool,
    /// Capas del fondo que no se dibujan.
    #[serde(default)]
    pub hidden_layers: Vec<String>,
    /// Plano limpio: solo el fondo en negro, sin recinto ni resultado.
    #[serde(default)]
    pub clean: bool,
    #[serde(default = "yes")]
    pub numbers: bool,
}
fn a3() -> String {
    "A3".into()
}
fn ty() -> String {
    "type".into()
}
fn yes() -> bool {
    true
}

const STD_SCALES: [f64; 15] = [20.0, 25.0, 50.0, 75.0, 100.0, 125.0, 150.0, 200.0, 250.0, 300.0, 400.0, 500.0, 750.0, 1000.0, 2000.0];

fn paper_size(p: &str) -> (f64, f64) {
    // apaisado, en puntos
    match p.to_uppercase().as_str() {
        "A4" => (841.89, 595.28),
        "A2" => (1683.78, 1190.55),
        "A1" => (2383.94, 1683.78),
        "A0" => (3370.39, 2383.94),
        _ => (1190.55, 841.89),
    }
}

fn hex_rgb(h: &str) -> (f64, f64, f64) {
    let h = h.trim_start_matches('#');
    if h.len() < 6 {
        return (0.5, 0.5, 0.5);
    }
    let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(128) as f64 / 255.0;
    (p(0), p(2), p(4))
}

/// Texto PDF literal en WinAnsi (soporta tildes y ñ).
fn pdf_str(s: &str) -> String {
    let mut o = String::from("(");
    for c in s.chars() {
        let b: u32 = match c {
            '–' => 0x96,
            '—' => 0x97,
            '·' => 0xB7,
            '•' => 0x95,
            '×' => 0xD7,
            '²' => 0xB2,
            '°' => 0xB0,
            c if (c as u32) < 256 => c as u32,
            _ => '?' as u32,
        };
        match b {
            0x28 | 0x29 | 0x5C => {
                o.push('\\');
                o.push(b as u8 as char);
            }
            0x20..=0x7E => o.push(b as u8 as char),
            _ => {
                let _ = write!(o, "\\{:03o}", b);
            }
        }
    }
    o.push(')');
    o
}

/// Ancho aproximado de un texto en Helvetica (em promedio).
fn text_w(s: &str, size: f64) -> f64 {
    s.chars().map(|c| if c.is_ascii_digit() { 0.556 } else if c == ' ' { 0.278 } else if c.is_uppercase() { 0.68 } else { 0.52 }).sum::<f64>() * size
}

struct Canvas {
    s: String,
    // mundo (mm) → página (pt)
    k: f64,
    ox: f64,
    oy: f64,
}

impl Canvas {
    fn p(&self, q: Pt) -> (f64, f64) {
        (self.ox + q.x * self.k, self.oy + q.y * self.k)
    }
    fn path(&mut self, pts: &[Pt], close: bool) {
        for (i, q) in pts.iter().enumerate() {
            let (x, y) = self.p(*q);
            let _ = write!(self.s, "{x:.2} {y:.2} {} ", if i == 0 { "m" } else { "l" });
        }
        if close {
            self.s.push_str("h ");
        }
    }
    fn fill_rgb(&mut self, c: (f64, f64, f64)) {
        let _ = write!(self.s, "{:.3} {:.3} {:.3} rg ", c.0, c.1, c.2);
    }
    fn stroke_rgb(&mut self, c: (f64, f64, f64)) {
        let _ = write!(self.s, "{:.3} {:.3} {:.3} RG ", c.0, c.1, c.2);
    }
    fn lw(&mut self, w: f64) {
        let _ = write!(self.s, "{w:.2} w ");
    }
    fn raw(&mut self, t: &str) {
        self.s.push_str(t);
        self.s.push('\n');
    }
    fn text(&mut self, x: f64, y: f64, size: f64, bold: bool, t: &str) {
        let _ = writeln!(self.s, "BT /{} {size:.2} Tf {x:.2} {y:.2} Td {} Tj ET", if bold { "F2" } else { "F1" }, pdf_str(t));
    }
    fn text_c(&mut self, x: f64, y: f64, size: f64, bold: bool, t: &str) {
        self.text(x - text_w(t, size) / 2.0, y - size * 0.35, size, bold, t);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render_pdf(plan: Option<&Plan>, layout: Option<&Layout>, cfg: Option<&Config>, bg: Option<(&Drawing, f64)>, opts: &PdfOpts) -> Result<Vec<u8>, String> {
    let (pw, ph) = paper_size(&opts.paper);
    let margin = 28.0;
    let side = if opts.clean { 0.0 } else { (pw * 0.2).clamp(170.0, 330.0) };
    let title_h = if opts.clean { 46.0 } else { 0.0 };
    let area = (margin, margin + title_h, pw - margin * 2.0 - side - if side > 0.0 { 12.0 } else { 0.0 }, ph - margin * 2.0 - title_h);

    // límites del dibujo
    let mut bb = BBox::empty();
    if let Some(p) = plan.filter(|_| !opts.clean) {
        for r in &p.region {
            for &q in r {
                bb.add(q);
            }
        }
    }
    if !bb.is_valid() {
        if let Some((d, s)) = bg {
            // capas visibles
            for p in &d.paths {
                if opts.hidden_layers.contains(&d.layers[p.layer as usize].name) {
                    continue;
                }
                for &q in &p.pts {
                    bb.add(q * s);
                }
            }
        }
    }
    if !bb.is_valid() {
        return Err("No hay nada que dibujar.".into());
    }
    let bb = bb.expand(800.0);
    let pt_per_mm = 72.0 / 25.4;
    let need = (bb.w() * pt_per_mm / area.2).max(bb.h() * pt_per_mm / area.3);
    let n = match opts.scale {
        Some(s) if s > 0.0 => s,
        _ => *STD_SCALES.iter().find(|&&s| s >= need).unwrap_or(&need.ceil()),
    };
    let k = pt_per_mm / n;
    let (dw, dh) = (bb.w() * k, bb.h() * k);
    let ox = area.0 + (area.2 - dw) / 2.0 - bb.min.x * k;
    let oy = area.1 + (area.3 - dh) / 2.0 - bb.min.y * k;
    let mut c = Canvas { s: String::new(), k, ox, oy };
    c.raw("1 J 1 j");

    // recorte al área de dibujo
    c.raw("q");
    let _ = writeln!(c.s, "{:.2} {:.2} {:.2} {:.2} re W n", area.0, area.1, area.2, area.3);

    // ------------------------------------------------ fondo
    if let Some((d, s)) = bg.filter(|_| opts.background || opts.clean) {
        let col = if opts.clean { (0.0, 0.0, 0.0) } else { (0.62, 0.65, 0.63) };
        c.stroke_rgb(col);
        c.lw(if opts.clean { 0.3 } else { 0.25 });
        for p in &d.paths {
            if opts.hidden_layers.contains(&d.layers[p.layer as usize].name) {
                continue;
            }
            let pts: Vec<Pt> = p.pts.iter().map(|q| *q * s).collect();
            c.path(&pts, p.closed);
            c.raw("S");
        }
        if opts.clean {
            c.fill_rgb((0.0, 0.0, 0.0));
            for t in &d.texts {
                if opts.hidden_layers.contains(&d.layers[t.layer as usize].name) || t.text.trim().is_empty() {
                    continue;
                }
                let (x, y) = c.p(t.pos * s);
                let size = (t.height * s * k).max(3.0);
                c.text(x, y, size, false, t.text.trim());
            }
        }
    }

    let catalog: Vec<DeskType> = cfg.map(|c| c.catalog.clone()).unwrap_or_default();
    let org = cfg.map(|c| c.org.clone()).unwrap_or_default();

    if !opts.clean {
        if let Some(plan) = plan {
            // recinto
            c.stroke_rgb((0.15, 0.19, 0.17));
            c.lw(0.9);
            for r in &plan.region {
                c.path(r, true);
                c.raw("S");
            }
            // pasillo principal (translúcido)
            if let Some(l) = layout {
                c.raw("q /GS1 gs");
                c.fill_rgb(hex_rgb("#6fa8c4"));
                for p in &l.corridors {
                    c.path(p, true);
                    c.raw("f");
                }
                c.raw("Q");
            }
            for o in &plan.obstacles {
                let col = match o.kind {
                    ObstacleKind::Column => "#c86b5c",
                    ObstacleKind::FixedOffice => "#c8d3e0",
                    ObstacleKind::Restricted => "#e3b3da",
                    ObstacleKind::Reserved => "#e2d99a",
                };
                c.fill_rgb(hex_rgb(col));
                c.stroke_rgb((0.33, 0.38, 0.36));
                c.lw(0.4);
                c.path(&o.poly, true);
                c.raw("B");
                if !o.label.is_empty() {
                    let (x, y) = c.p(centroid(&o.poly));
                    c.fill_rgb((0.15, 0.19, 0.17));
                    c.text_c(x, y, 6.0, false, &o.label);
                }
            }
            c.stroke_rgb(hex_rgb("#3aa7d9"));
            c.lw(2.0);
            for w in &plan.windows {
                c.path(&[w.a, w.b], false);
                c.raw("S");
            }
            for d in &plan.doors {
                let col = match d.kind {
                    DoorKind::Main => "#1a9c3b",
                    DoorKind::Exit => "#e07b12",
                    DoorKind::Office => "#2a8fb0",
                    DoorKind::Ignore => continue,
                };
                c.stroke_rgb(hex_rgb(col));
                c.lw(if d.kind == DoorKind::Main { 3.5 } else { 2.2 });
                c.path(&[d.a, d.b], false);
                c.raw("S");
            }
        }
        if let Some(l) = layout {
            let by_type = opts.color_by == "type";
            for m in &l.modules {
                let col = if by_type { catalog.iter().find(|t| t.id == m.type_id).map(|t| hex_rgb(&t.color)).unwrap_or((0.5, 0.5, 0.5)) } else { (0.78, 0.81, 0.79) };
                c.fill_rgb(col);
                c.stroke_rgb((0.15, 0.19, 0.17));
                c.lw(if m.pinned { 1.0 } else { 0.35 });
                c.path(&m.body, true);
                c.raw("B");
            }
            for s in &l.seats {
                let person_col = match opts.color_by.as_str() {
                    "role" => org.roles.iter().find(|r| r.id == s.role_id).map(|r| hex_rgb(&r.color)),
                    "client" => org.clients.iter().find(|r| r.id == s.client_id).map(|r| hex_rgb(&r.color)),
                    _ => None,
                };
                if let Some(pc) = person_col {
                    // porción de escritorio de la persona
                    let t = catalog.iter().find(|t| t.id == s.type_id);
                    let w = t.map(|t| t.seat_width).unwrap_or(1200.0) * 0.5;
                    let dep = t.map(|t| t.depth).unwrap_or(600.0);
                    let f = s.facing;
                    let u = f.perp();
                    let c0 = s.pos + f * 250.0;
                    let c1 = c0 + f * dep;
                    c.fill_rgb(pc);
                    c.path(&[c0 - u * w, c0 + u * w, c1 + u * w, c1 - u * w], true);
                    c.raw("f");
                }
                c.fill_rgb(person_col.unwrap_or((1.0, 1.0, 1.0)));
                c.stroke_rgb((0.33, 0.38, 0.36));
                c.lw(0.3);
                c.path(&s.chair, true);
                c.raw("B");
                if opts.numbers {
                    let (x, y) = c.p(s.pos);
                    let size = (380.0 * k).clamp(2.2, 7.0);
                    let light = person_col.map(|(r, g, b)| r * 0.3 + g * 0.59 + b * 0.11 < 0.5).unwrap_or(false);
                    c.fill_rgb(if light { (1.0, 1.0, 1.0) } else { (0.1, 0.1, 0.1) });
                    c.text_c(x, y, size, true, &format!("{}", s.number));
                }
            }
        }
    }
    c.raw("Q");

    // ------------------------------------------------ marco y rótulo
    c.stroke_rgb((0.15, 0.19, 0.17));
    c.lw(0.6);
    let _ = writeln!(c.s, "{:.2} {:.2} {:.2} {:.2} re S", margin - 6.0, margin - 6.0, pw - 2.0 * margin + 12.0, ph - 2.0 * margin + 12.0);
    let date = today();
    if opts.clean {
        c.fill_rgb((0.1, 0.1, 0.1));
        c.text(margin, margin + 22.0, 13.0, true, if opts.title.is_empty() { "Plano" } else { &opts.title });
        c.text(margin, margin + 8.0, 8.0, false, &format!("Plano sin mobiliario · Escala 1:{n:.0} en {} · {date}", opts.paper.to_uppercase()));
        scale_bar(&mut c, pw - margin - 160.0, margin + 10.0, k, n);
    } else {
        let x0 = pw - margin - side;
        let mut y = ph - margin - 8.0;
        c.fill_rgb((0.1, 0.1, 0.1));
        let title = if opts.title.is_empty() { "Distribución de puestos" } else { &opts.title };
        for line in wrap(title, side, 14.0) {
            c.text(x0, y - 12.0, 14.0, true, &line);
            y -= 17.0;
        }
        c.text(x0, y - 10.0, 7.5, false, &format!("Escala 1:{n:.0} en {} · {date}", opts.paper.to_uppercase()));
        y -= 26.0;
        if let Some(l) = layout {
            let kp = &l.kpis;
            c.text(x0, y - 22.0, 26.0, true, &format!("{}", kp.seats_placed));
            c.text(x0 + text_w(&format!("{}", kp.seats_placed), 26.0) + 4.0, y - 22.0, 12.0, false, &format!("/ {} puestos", kp.seats_requested));
            y -= 38.0;
            let rows: Vec<(String, String)> = vec![
                ("Área útil".into(), format!("{:.0} m²", kp.area_m2)),
                ("Área por puesto".into(), format!("{:.1} m²", kp.m2_per_seat)),
                ("Recorrido a salida".into(), format!("{:.1} m prom. · {:.1} máx.", kp.avg_travel_exit_m, kp.max_travel_exit_m)),
                ("Cerca de ventana".into(), format!("{:.0} %", kp.daylight_pct)),
                ("Accesibles PMR".into(), format!("{}", kp.accessible_pmr)),
            ];
            for (a, b) in rows {
                c.fill_rgb((0.33, 0.38, 0.36));
                c.text(x0, y, 7.5, false, &a);
                c.fill_rgb((0.1, 0.1, 0.1));
                c.text(x0 + side * 0.48, y, 7.5, true, &b);
                y -= 11.0;
            }
            y -= 8.0;
            // leyenda
            let mut legend: Vec<((f64, f64, f64), String)> = Vec::new();
            match opts.color_by.as_str() {
                "role" => {
                    for r in &org.roles {
                        let cnt = l.seats.iter().filter(|s| s.role_id == r.id).count();
                        legend.push((hex_rgb(&r.color), format!("{} ({cnt})", r.name)));
                    }
                }
                "client" => {
                    for r in &org.clients {
                        let cnt = l.seats.iter().filter(|s| s.client_id == r.id).count();
                        legend.push((hex_rgb(&r.color), format!("{} ({cnt})", r.name)));
                    }
                }
                _ => {
                    for t in &catalog {
                        let cnt = l.seats.iter().filter(|s| s.type_id == t.id).count();
                        if cnt > 0 {
                            legend.push((hex_rgb(&t.color), format!("{} ({cnt} puestos)", t.name)));
                        }
                    }
                }
            }
            let free = l.seats.iter().filter(|s| s.role_id.is_empty()).count();
            if opts.color_by != "type" && free > 0 {
                legend.push(((1.0, 1.0, 1.0), format!("Libres ({free})")));
            }
            c.fill_rgb((0.1, 0.1, 0.1));
            c.text(x0, y, 9.0, true, "Convenciones");
            y -= 13.0;
            for (col, name) in legend {
                if y < margin + 60.0 {
                    break;
                }
                c.fill_rgb(col);
                c.stroke_rgb((0.33, 0.38, 0.36));
                c.lw(0.3);
                let _ = writeln!(c.s, "{:.2} {:.2} 9 7 re B", x0, y - 1.0);
                c.fill_rgb((0.1, 0.1, 0.1));
                c.text(x0 + 14.0, y, 7.5, false, &name);
                y -= 11.0;
            }
            for (col, name) in [("#1a9c3b", "Puerta principal"), ("#e07b12", "Salida"), ("#6fa8c4", "Pasillo principal"), ("#3aa7d9", "Ventana")] {
                if y < margin + 60.0 {
                    break;
                }
                c.fill_rgb(hex_rgb(col));
                let _ = writeln!(c.s, "{:.2} {:.2} 9 3 re f", x0, y + 1.0);
                c.fill_rgb((0.1, 0.1, 0.1));
                c.text(x0 + 14.0, y, 7.5, false, name);
                y -= 11.0;
            }
            let hard: Vec<&Violation> = l.violations.iter().filter(|v| v.hard).collect();
            if !hard.is_empty() && y > margin + 80.0 {
                y -= 6.0;
                c.fill_rgb(hex_rgb("#c4472b"));
                c.text(x0, y, 8.0, true, &format!("Reglas incumplidas: {}", hard.len()));
                y -= 11.0;
                for v in hard.iter().take(4) {
                    for line in wrap(&v.message, side, 6.5) {
                        if y < margin + 60.0 {
                            break;
                        }
                        c.text(x0, y, 6.5, false, &line);
                        y -= 9.0;
                    }
                }
            }
        }
        scale_bar(&mut c, x0, margin + 12.0, k, n);
    }

    // ------------------------------------------------ documento
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let f1 = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding" });
    let f2 = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica-Bold", "Encoding" => "WinAnsiEncoding" });
    let gs = doc.add_object(dictionary! { "Type" => "ExtGState", "ca" => 0.35, "CA" => 0.35 });
    let resources = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => f1, "F2" => f2 }, "ExtGState" => dictionary! { "GS1" => gs } });
    let mut stream = Stream::new(dictionary! {}, c.s.into_bytes());
    let _ = stream.compress();
    let content = doc.add_object(stream);
    let page = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), Object::Real(pw as f32), Object::Real(ph as f32)],
        "Contents" => content,
        "Resources" => resources,
    });
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }));
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let info = doc.add_object(dictionary! { "Title" => Object::string_literal(opts.title.clone()), "Producer" => "Optimizador de Puestos" });
    doc.trailer.set("Info", info);
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

fn scale_bar(c: &mut Canvas, x: f64, y: f64, k: f64, n: f64) {
    // barra de 0 a 5 m (o 10 m si la escala es pequeña)
    let total_m = if n >= 300.0 { 20.0 } else if n >= 150.0 { 10.0 } else { 5.0 };
    let seg = total_m / 5.0;
    let w = seg * 1000.0 * k;
    c.lw(0.4);
    c.stroke_rgb((0.1, 0.1, 0.1));
    for i in 0..5 {
        if i % 2 == 0 {
            c.fill_rgb((0.1, 0.1, 0.1));
        } else {
            c.fill_rgb((1.0, 1.0, 1.0));
        }
        let _ = writeln!(c.s, "{:.2} {:.2} {:.2} 4 re B", x + i as f64 * w, y, w);
    }
    c.fill_rgb((0.1, 0.1, 0.1));
    c.text(x - 2.0, y + 7.0, 6.5, false, "0");
    c.text(x + 5.0 * w - 8.0, y + 7.0, 6.5, false, &format!("{total_m:.0} m"));
}

fn wrap(s: &str, width: f64, size: f64) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for w in s.split_whitespace() {
        let cand = if cur.is_empty() { w.to_string() } else { format!("{cur} {w}") };
        if text_w(&cand, size) > width && !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
            cur = w.to_string();
        } else {
            cur = cand;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

fn today() -> String {
    // fecha civil a partir del reloj del sistema (UTC)
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let days = secs.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{d:02}/{m:02}/{y}")
}

/// DXF R12 del dibujo original sin las capas indicadas (p. ej. mobiliario).
/// `out_mm`: mm por unidad de salida (1 = milímetros).
pub fn clean_dxf(d: &Drawing, mm_per_unit: f64, out_mm: f64, exclude: &[String]) -> String {
    let k = mm_per_unit / out_mm.max(1e-9);
    let name = |s: &str| -> String { s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '$' { c } else { '_' }).collect() };
    let mut s = String::new();
    let pair = |c: i32, v: &str, s: &mut String| {
        let _ = write!(s, "{c}\n{v}\n");
    };
    pair(0, "SECTION", &mut s);
    pair(2, "HEADER", &mut s);
    pair(9, "$ACADVER", &mut s);
    pair(1, "AC1009", &mut s);
    pair(0, "ENDSEC", &mut s);
    pair(0, "SECTION", &mut s);
    pair(2, "TABLES", &mut s);
    pair(0, "TABLE", &mut s);
    pair(2, "LAYER", &mut s);
    let layers: Vec<&crate::import::LayerInfo> = d.layers.iter().filter(|l| !exclude.contains(&l.name) && l.paths > 0).collect();
    pair(70, &layers.len().to_string(), &mut s);
    for (i, l) in layers.iter().enumerate() {
        pair(0, "LAYER", &mut s);
        pair(2, &name(&l.name), &mut s);
        pair(70, "0", &mut s);
        pair(62, &(1 + (i % 254)).to_string(), &mut s);
        pair(6, "CONTINUOUS", &mut s);
    }
    pair(0, "ENDTAB", &mut s);
    pair(0, "ENDSEC", &mut s);
    pair(0, "SECTION", &mut s);
    pair(2, "ENTITIES", &mut s);
    for p in &d.paths {
        let l = &d.layers[p.layer as usize].name;
        if exclude.contains(l) {
            continue;
        }
        let ln = name(l);
        if p.pts.len() == 2 {
            pair(0, "LINE", &mut s);
            pair(8, &ln, &mut s);
            for (c, v) in [(10, p.pts[0].x), (20, p.pts[0].y), (11, p.pts[1].x), (21, p.pts[1].y)] {
                pair(c, &format!("{:.3}", v * k), &mut s);
            }
            pair(30, "0", &mut s);
            pair(31, "0", &mut s);
            continue;
        }
        pair(0, "POLYLINE", &mut s);
        pair(8, &ln, &mut s);
        pair(66, "1", &mut s);
        pair(10, "0", &mut s);
        pair(20, "0", &mut s);
        pair(30, "0", &mut s);
        pair(70, if p.closed { "1" } else { "0" }, &mut s);
        for q in &p.pts {
            pair(0, "VERTEX", &mut s);
            pair(8, &ln, &mut s);
            pair(10, &format!("{:.3}", q.x * k), &mut s);
            pair(20, &format!("{:.3}", q.y * k), &mut s);
            pair(30, "0", &mut s);
        }
        pair(0, "SEQEND", &mut s);
        pair(8, &ln, &mut s);
    }
    for t in &d.texts {
        let l = &d.layers[t.layer as usize].name;
        if exclude.contains(l) || t.text.trim().is_empty() {
            continue;
        }
        pair(0, "TEXT", &mut s);
        pair(8, &name(l), &mut s);
        pair(10, &format!("{:.3}", t.pos.x * k), &mut s);
        pair(20, &format!("{:.3}", t.pos.y * k), &mut s);
        pair(30, "0", &mut s);
        pair(40, &format!("{:.3}", t.height * k), &mut s);
        pair(1, &crate::export::dxf_text(t.text.trim()), &mut s);
        pair(50, &format!("{:.2}", t.rot), &mut s);
    }
    pair(0, "ENDSEC", &mut s);
    pair(0, "EOF", &mut s);
    s
}
