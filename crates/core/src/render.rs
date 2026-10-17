//! Renderizado SVG del plano y del resultado (vista previa, exportación y pruebas).

use crate::geom::*;
use crate::import::Drawing;
use crate::model::*;
use std::fmt::Write;

pub struct RenderOpts<'a> {
    pub background: Option<(&'a Drawing, f64)>,
    pub color_by: ColorBy,
    pub show_keepouts: bool,
    pub px_width: f64,
    pub title: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ColorBy {
    Type,
    Role,
    Client,
}

impl Default for RenderOpts<'_> {
    fn default() -> Self {
        RenderOpts { background: None, color_by: ColorBy::Type, show_keepouts: true, px_width: 1600.0, title: String::new() }
    }
}

fn poly_path(pts: &[Pt]) -> String {
    let mut s = String::new();
    for (i, p) in pts.iter().enumerate() {
        let _ = write!(s, "{}{:.0},{:.0}", if i == 0 { "M" } else { "L" }, p.x, -p.y);
    }
    s.push('Z');
    s
}

pub fn render_svg(plan: &Plan, layout: Option<&Layout>, cfg: Option<&Config>, o: &RenderOpts) -> String {
    let mut bb = BBox::empty();
    for r in &plan.region {
        for &p in r {
            bb.add(p);
        }
    }
    if let Some((d, s)) = o.background {
        if !bb.is_valid() {
            bb = BBox { min: d.bounds.min * s, max: d.bounds.max * s };
        }
    }
    if !bb.is_valid() {
        bb = BBox { min: pt(0.0, 0.0), max: pt(1000.0, 1000.0) };
    }
    let bb = bb.expand(1500.0);
    let stroke = (bb.w().max(bb.h()) / 1600.0).max(10.0);
    let px_h = o.px_width * bb.h() / bb.w();
    let mut s = String::new();
    let _ = write!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{:.0}" height="{:.0}" viewBox="{:.0} {:.0} {:.0} {:.0}" font-family="sans-serif">"#,
        o.px_width, px_h, bb.min.x, -bb.max.y, bb.w(), bb.h()
    );
    let _ = write!(s, r#"<rect x="{:.0}" y="{:.0}" width="{:.0}" height="{:.0}" fill="white"/>"#, bb.min.x, -bb.max.y, bb.w(), bb.h());
    if let Some((d, k)) = o.background {
        let _ = write!(s, r##"<g stroke="#bbb" stroke-width="{:.1}" fill="none">"##, stroke * 0.6);
        for p in &d.paths {
            let pts: Vec<Pt> = p.pts.iter().map(|q| *q * k).collect();
            let mut path = poly_path(&pts);
            if !p.closed {
                path.pop();
            }
            let _ = write!(s, r#"<path d="{}"/>"#, path);
        }
        s.push_str("</g>");
    }
    // recinto
    let mut reg = String::new();
    for r in &plan.region {
        reg.push_str(&poly_path(r));
    }
    let _ = write!(s, r##"<path d="{}" fill="#f4f1ea" fill-rule="evenodd" stroke="#222" stroke-width="{:.0}"/>"##, reg, stroke * 2.0);
    if let Some(l) = layout {
        if o.show_keepouts {
            for k in &l.keepouts {
                let _ = write!(s, r##"<path d="{}" fill="#f2c94c" fill-opacity="0.25" stroke="none"/>"##, poly_path(k));
            }
        }
        for c in &l.corridors {
            let _ = write!(s, r##"<path d="{}" fill="#7fb3d5" fill-opacity="0.35" stroke="none"/>"##, poly_path(c));
        }
    }
    for ob in &plan.obstacles {
        let (fill, st) = match ob.kind {
            ObstacleKind::Column => ("#d9534f", "#a33"),
            ObstacleKind::FixedOffice => ("#8fa7c9", "#355"),
            ObstacleKind::Restricted => ("#e377c2", "#a3a"),
            ObstacleKind::Reserved => ("#bcbd22", "#776"),
        };
        let _ = write!(s, r#"<path d="{}" fill="{}" fill-opacity="0.5" stroke="{}" stroke-width="{:.0}"/>"#, poly_path(&ob.poly), fill, st, stroke);
        if !ob.label.is_empty() {
            let c = centroid(&ob.poly);
            let _ = write!(s, r##"<text x="{:.0}" y="{:.0}" font-size="{:.0}" text-anchor="middle" fill="#123">{}</text>"##, c.x, -c.y, stroke * 14.0, xml(&ob.label));
        }
    }
    for w in &plan.windows {
        let _ = write!(s, r##"<line x1="{:.0}" y1="{:.0}" x2="{:.0}" y2="{:.0}" stroke="#2bb5e8" stroke-width="{:.0}"/>"##, w.a.x, -w.a.y, w.b.x, -w.b.y, stroke * 5.0);
    }
    for d in &plan.doors {
        let col = match d.kind {
            DoorKind::Main => "#1a9c3b",
            DoorKind::Exit => "#f08c00",
            DoorKind::Office => "#17a2b8",
            DoorKind::Ignore => "#999",
        };
        let _ = write!(s, r#"<line x1="{:.0}" y1="{:.0}" x2="{:.0}" y2="{:.0}" stroke="{}" stroke-width="{:.0}"/>"#, d.a.x, -d.a.y, d.b.x, -d.b.y, col, stroke * 6.0);
    }
    if let Some(l) = layout {
        let cat: Vec<DeskType> = cfg.map(|c| c.catalog.clone()).unwrap_or_default();
        let org = cfg.map(|c| c.org.clone()).unwrap_or_default();
        for m in &l.modules {
            let col = cat.iter().find(|t| t.id == m.type_id).map(|t| t.color.clone()).unwrap_or("#666".into());
            let _ = write!(s, r##"<path d="{}" fill="{}" fill-opacity="0.85" stroke="#111" stroke-width="{:.0}"/>"##, poly_path(&m.body), col, stroke * 0.8);
        }
        for st in &l.seats {
            let col = match o.color_by {
                ColorBy::Role => org.roles.iter().find(|r| r.id == st.role_id).map(|r| r.color.clone()),
                ColorBy::Client => org.clients.iter().find(|r| r.id == st.client_id).map(|r| r.color.clone()),
                ColorBy::Type => None,
            }
            .unwrap_or_else(|| "#333".into());
            let fill = if o.color_by == ColorBy::Type { "#ffffff".to_string() } else { col.clone() };
            let _ = write!(s, r##"<path d="{}" fill="{}" stroke="#333" stroke-width="{:.0}"/>"##, poly_path(&st.chair), fill, stroke * 0.7);
            if st.flags.iter().any(|f| f.starts_with("Sin acceso") || f.starts_with("Recorrido") || f.starts_with("Regla")) {
                let _ = write!(s, r##"<circle cx="{:.0}" cy="{:.0}" r="{:.0}" fill="#e74c3c"/>"##, st.pos.x, -st.pos.y, stroke * 3.0);
            }
        }
        for v in &l.violations {
            if let Some(p) = v.at {
                let _ = write!(s, r##"<circle cx="{:.0}" cy="{:.0}" r="{:.0}" fill="none" stroke="#e00" stroke-width="{:.0}"/>"##, p.x, -p.y, stroke * 12.0, stroke * 2.0);
            }
        }
    }
    if !o.title.is_empty() {
        let _ = write!(s, r##"<text x="{:.0}" y="{:.0}" font-size="{:.0}" fill="#000">{}</text>"##, bb.min.x + stroke * 10.0, -bb.max.y + stroke * 30.0, stroke * 22.0, xml(&o.title));
    }
    s.push_str("</svg>");
    s
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
