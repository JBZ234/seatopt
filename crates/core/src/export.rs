//! Exportación: DXF (R12, lo abre cualquier versión de AutoCAD) y CSV de puestos.

use crate::geom::*;
use crate::model::*;
use std::fmt::Write;

struct Dxf {
    s: String,
    k: f64,
}

impl Dxf {
    fn pair(&mut self, code: i32, v: impl std::fmt::Display) {
        let _ = write!(self.s, "{code}\n{v}\n");
    }
    fn poly(&mut self, layer: &str, pts: &[Pt], closed: bool, color: Option<u8>) {
        if pts.len() < 2 {
            return;
        }
        self.pair(0, "POLYLINE");
        self.pair(8, layer);
        if let Some(c) = color {
            self.pair(62, c);
        }
        self.pair(66, 1);
        self.pair(10, 0.0);
        self.pair(20, 0.0);
        self.pair(30, 0.0);
        self.pair(70, if closed { 1 } else { 0 });
        for p in pts {
            self.pair(0, "VERTEX");
            self.pair(8, layer);
            self.pair(10, format!("{:.3}", p.x / self.k));
            self.pair(20, format!("{:.3}", p.y / self.k));
            self.pair(30, 0.0);
        }
        self.pair(0, "SEQEND");
        self.pair(8, layer);
    }
    fn line(&mut self, layer: &str, a: Pt, b: Pt) {
        self.pair(0, "LINE");
        self.pair(8, layer);
        self.pair(10, format!("{:.3}", a.x / self.k));
        self.pair(20, format!("{:.3}", a.y / self.k));
        self.pair(30, 0.0);
        self.pair(11, format!("{:.3}", b.x / self.k));
        self.pair(21, format!("{:.3}", b.y / self.k));
        self.pair(31, 0.0);
    }
    fn text(&mut self, layer: &str, p: Pt, h: f64, rot: f64, t: &str) {
        self.pair(0, "TEXT");
        self.pair(8, layer);
        self.pair(10, format!("{:.3}", p.x / self.k));
        self.pair(20, format!("{:.3}", p.y / self.k));
        self.pair(30, 0.0);
        self.pair(40, format!("{:.3}", h / self.k));
        self.pair(1, dxf_text(&t.replace('\n', " ")));
        self.pair(50, format!("{:.2}", rot));
        self.pair(72, 1);
        self.pair(73, 2);
        self.pair(11, format!("{:.3}", p.x / self.k));
        self.pair(21, format!("{:.3}", p.y / self.k));
        self.pair(31, 0.0);
    }
}

/// Texto para DXF R12: caracteres no ASCII como \U+XXXX (AutoCAD los interpreta).
pub fn dxf_text(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        if c.is_ascii() {
            o.push(c);
        } else {
            let _ = write!(o, "\\U+{:04X}", c as u32);
        }
    }
    o
}

fn layer_name(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c.to_ascii_uppercase() } else { '_' }).collect()
}

/// `mm_per_unit`: para volver a las unidades del dibujo original (y superponer sobre él).
pub fn to_dxf(plan: &Plan, layout: &Layout, cfg: &Config, mm_per_unit: f64, include_plan: bool) -> String {
    let mut d = Dxf { s: String::new(), k: mm_per_unit.max(1e-9) };
    let mut layers: Vec<(String, u8)> = vec![
        ("SO-ESCRITORIOS".into(), 5),
        ("SO-SILLAS".into(), 8),
        ("SO-NUMEROS".into(), 7),
        ("SO-PASILLO-PRINCIPAL".into(), 4),
        ("SO-DESPEJES".into(), 2),
        ("SO-RECINTO".into(), 7),
        ("SO-OBSTACULOS".into(), 1),
        ("SO-PUERTAS".into(), 3),
    ];
    for t in &cfg.catalog {
        layers.push((format!("SO-ESC-{}", layer_name(&t.id)), 5));
    }
    for r in &cfg.org.roles {
        layers.push((format!("SO-ROL-{}", layer_name(&r.name)), 3));
    }
    for c in &cfg.org.clients {
        layers.push((format!("SO-CLIENTE-{}", layer_name(&c.name)), 6));
    }
    // encabezado mínimo + tabla de capas
    d.pair(0, "SECTION");
    d.pair(2, "HEADER");
    d.pair(9, "$ACADVER");
    d.pair(1, "AC1009");
    d.pair(0, "ENDSEC");
    d.pair(0, "SECTION");
    d.pair(2, "TABLES");
    d.pair(0, "TABLE");
    d.pair(2, "LAYER");
    d.pair(70, layers.len());
    for (n, c) in &layers {
        d.pair(0, "LAYER");
        d.pair(2, n);
        d.pair(70, 0);
        d.pair(62, *c);
        d.pair(6, "CONTINUOUS");
    }
    d.pair(0, "ENDTAB");
    d.pair(0, "ENDSEC");
    d.pair(0, "SECTION");
    d.pair(2, "ENTITIES");
    if include_plan {
        for r in &plan.region {
            d.poly("SO-RECINTO", r, true, None);
        }
        for o in &plan.obstacles {
            d.poly("SO-OBSTACULOS", &o.poly, true, None);
        }
        for dr in &plan.doors {
            d.line("SO-PUERTAS", dr.a, dr.b);
        }
    }
    for c in &layout.corridors {
        d.poly("SO-PASILLO-PRINCIPAL", c, true, None);
    }
    for k in &layout.keepouts {
        d.poly("SO-DESPEJES", k, true, None);
    }
    for m in &layout.modules {
        d.poly(&format!("SO-ESC-{}", layer_name(&m.type_id)), &m.body, true, None);
    }
    for s in &layout.seats {
        let mut layer = "SO-SILLAS".to_string();
        if let Some(r) = cfg.org.roles.iter().find(|r| r.id == s.role_id) {
            layer = format!("SO-ROL-{}", layer_name(&r.name));
        }
        d.poly(&layer, &s.chair, true, None);
        if let Some(c) = cfg.org.clients.iter().find(|c| c.id == s.client_id) {
            let cl = format!("SO-CLIENTE-{}", layer_name(&c.name));
            d.text(&cl, s.pos - s.facing * 450.0, 120.0, 0.0, &c.name);
        }
        let ang = s.facing.y.atan2(s.facing.x).to_degrees() - 90.0;
        d.text("SO-NUMEROS", s.pos, 160.0, ang, &format!("{:03}", s.number));
    }
    d.pair(0, "ENDSEC");
    d.pair(0, "EOF");
    d.s
}

pub fn to_csv(layout: &Layout, cfg: &Config) -> String {
    let mut s = String::from("numero;tipo;rol;cliente;x_mm;y_mm;orientacion_grados;modulo;avisos\n");
    for st in &layout.seats {
        let tipo = cfg.catalog.iter().find(|t| t.id == st.type_id).map(|t| t.name.as_str()).unwrap_or(&st.type_id);
        let rol = cfg.org.roles.iter().find(|r| r.id == st.role_id).map(|r| r.name.as_str()).unwrap_or("");
        let cli = cfg.org.clients.iter().find(|c| c.id == st.client_id).map(|c| c.name.as_str()).unwrap_or("");
        let ang = st.facing.y.atan2(st.facing.x).to_degrees().round() + 0.0;
        let _ = writeln!(
            s,
            "{:03};{};{};{};{:.0};{:.0};{:.0};{};{}",
            st.number,
            tipo,
            rol,
            cli,
            st.pos.x,
            st.pos.y,
            ang,
            st.module_id,
            st.flags.join(" | ").replace(';', ",")
        );
    }
    s
}
