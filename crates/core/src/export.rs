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

impl Dxf {
    fn arc(&mut self, layer: &str, c: Pt, r: f64, a0: f64, a1: f64) {
        self.pair(0, "ARC");
        self.pair(8, layer);
        self.pair(10, format!("{:.3}", c.x / self.k));
        self.pair(20, format!("{:.3}", c.y / self.k));
        self.pair(30, 0.0);
        self.pair(40, format!("{:.3}", r / self.k));
        self.pair(50, format!("{:.4}", a0));
        self.pair(51, format!("{:.4}", a1));
    }
}

/// Plano mínimo para optimizar (contorno, área amoblable, oficinas, columnas y puertas),
/// con las capas estándar de la app: se puede volver a importar tal cual.
pub fn plan_dxf(plan: &Plan, out_mm: f64) -> String {
    let mut d = Dxf { s: String::new(), k: out_mm.max(1e-9) };
    let layers: [(&str, u8); 12] = [
        ("A-EDIFICIO", 7),
        ("A-MURO-EXT", 7),
        ("A-RECINTOS", 8),
        ("A-OFICINA-FIJA", 5),
        ("A-COLUMNA", 1),
        ("A-ZONA-RESTRINGIDA", 6),
        ("A-ZONA-RESERVADA", 2),
        ("A-PUERTA-PPAL", 3),
        ("A-PUERTA-SALIDA", 30),
        ("A-PUERTA-OFICINA", 4),
        ("A-PUERTA-VANO", 3),
        ("A-TEXTO", 8),
    ];
    d.pair(0, "SECTION");
    d.pair(2, "HEADER");
    d.pair(9, "$ACADVER");
    d.pair(1, "AC1009");
    d.pair(9, "$INSUNITS");
    d.pair(70, insunits_code(d.k));
    d.pair(0, "ENDSEC");
    d.pair(0, "SECTION");
    d.pair(2, "TABLES");
    d.pair(0, "TABLE");
    d.pair(2, "LAYER");
    d.pair(70, layers.len());
    for (n, c) in layers {
        d.pair(0, "LAYER");
        d.pair(2, n);
        d.pair(70, 0);
        d.pair(62, c);
        d.pair(6, "CONTINUOUS");
    }
    d.pair(0, "ENDTAB");
    d.pair(0, "ENDSEC");
    d.pair(0, "SECTION");
    d.pair(2, "ENTITIES");
    if plan.building.len() >= 3 {
        d.poly("A-EDIFICIO", &plan.building, true, None);
    }
    for r in &plan.region {
        d.poly("A-MURO-EXT", r, true, None);
    }
    for rm in &plan.rooms {
        d.poly("A-RECINTOS", &rm.poly, true, None);
        if !rm.label.is_empty() {
            d.text("A-TEXTO", centroid(&rm.poly), 250.0, 0.0, &rm.label);
        }
    }
    for o in &plan.obstacles {
        let l = match o.kind {
            ObstacleKind::Column => "A-COLUMNA",
            ObstacleKind::FixedOffice => "A-OFICINA-FIJA",
            ObstacleKind::Restricted => "A-ZONA-RESTRINGIDA",
            ObstacleKind::Reserved => "A-ZONA-RESERVADA",
        };
        d.poly(l, &o.poly, true, None);
        if !o.label.is_empty() {
            d.text("A-TEXTO", centroid(&o.poly), 250.0, 0.0, &o.label);
        }
    }
    for dr in &plan.doors {
        let l = match dr.kind {
            DoorKind::Main => "A-PUERTA-PPAL",
            DoorKind::Exit => "A-PUERTA-SALIDA",
            DoorKind::Office => "A-PUERTA-OFICINA",
            DoorKind::Ignore => continue,
        };
        let w = dr.width();
        let u = (dr.b - dr.a).norm();
        let n = u.perp();
        let m = dr.mid();
        let inward = if point_in_region(m + n * 300.0, &plan.region) { n } else { -n };
        // convención de AutoCAD: hoja + arco en la capa de la puerta; el vano, aparte
        d.line("A-PUERTA-VANO", dr.a, dr.b);
        d.line(l, dr.a, dr.a + inward * w);
        let a0 = u.y.atan2(u.x).to_degrees();
        let a1 = inward.y.atan2(inward.x).to_degrees();
        let mut da = a1 - a0;
        while da > 180.0 {
            da -= 360.0;
        }
        while da < -180.0 {
            da += 360.0;
        }
        // ARC va en sentido antihorario de 50 a 51
        let (s, e) = if da >= 0.0 { (a0, a0 + da) } else { (a0 + da, a0) };
        d.arc(l, dr.a, w, s.rem_euclid(360.0), e.rem_euclid(360.0));
        if !dr.label.is_empty() {
            d.text("A-TEXTO", m + inward * (w + 300.0), 200.0, 0.0, &dr.label);
        }
    }
    d.pair(0, "ENDSEC");
    d.pair(0, "EOF");
    d.s
}

/// Código $INSUNITS de AutoCAD para «mm por unidad».
pub fn insunits_code(mm: f64) -> i32 {
    [(1.0, 4), (10.0, 5), (1000.0, 6), (25.4, 1), (304.8, 2)].iter().find(|(m, _)| (m - mm).abs() < 1e-6).map(|x| x.1).unwrap_or(0)
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
    d.pair(9, "$INSUNITS");
    d.pair(70, insunits_code(d.k));
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
