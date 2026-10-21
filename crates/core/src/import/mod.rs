//! Importación de planos: DXF, DWG (vía convertidor externo) y PDF vectorial con capas.

pub mod dwg;
pub mod dxf_in;
pub mod pdf_in;

use crate::geom::{BBox, Pt};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SourceKind {
    Dxf,
    Dwg,
    Pdf,
}

/// Trazo en unidades del dibujo original.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DPath {
    pub layer: u32,
    pub pts: Vec<Pt>,
    pub closed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DText {
    pub layer: u32,
    pub pos: Pt,
    pub height: f64,
    pub rot: f64,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerInfo {
    pub name: String,
    pub color: String,
    pub paths: usize,
}

/// Dibujo importado, tal cual viene del archivo (unidades del archivo).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Drawing {
    pub name: String,
    pub source: SourceKind,
    pub layers: Vec<LayerInfo>,
    pub paths: Vec<DPath>,
    /// Tramos curvos (arcos) por separado: sirven para detectar puertas.
    pub curves: Vec<DPath>,
    pub texts: Vec<DText>,
    pub bounds: BBox,
    /// mm por unidad del dibujo según el archivo (None si no se sabe).
    pub declared_unit_mm: Option<f64>,
    /// Nota legible sobre unidades / escala.
    pub unit_note: String,
}

impl Drawing {
    pub fn new(name: &str, source: SourceKind) -> Self {
        Drawing {
            name: name.into(),
            source,
            layers: vec![],
            paths: vec![],
            curves: vec![],
            texts: vec![],
            bounds: BBox::empty(),
            declared_unit_mm: None,
            unit_note: String::new(),
        }
    }
    pub fn layer_id(&mut self, name: &str, color: &str) -> u32 {
        if let Some(i) = self.layers.iter().position(|l| l.name == name) {
            return i as u32;
        }
        self.layers.push(LayerInfo { name: name.into(), color: color.into(), paths: 0 });
        (self.layers.len() - 1) as u32
    }
    pub fn add_path(&mut self, layer: u32, pts: Vec<Pt>, closed: bool) {
        if pts.len() < 2 {
            return;
        }
        if pts.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
            return;
        }
        for &p in &pts {
            self.bounds.add(p);
        }
        self.layers[layer as usize].paths += 1;
        self.paths.push(DPath { layer, pts, closed });
    }
    pub fn add_curve(&mut self, layer: u32, pts: Vec<Pt>) {
        if pts.len() >= 3 && pts.iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
            self.curves.push(DPath { layer, pts, closed: false });
        }
    }
    pub fn finalize(&mut self) {
        // Recalcular límites ignorando trazos lejanos aislados (p. ej. textos de rótulo muy lejos).
        self.bounds = BBox::empty();
        for p in &self.paths {
            for &q in &p.pts {
                self.bounds.add(q);
            }
        }
    }
}

/// Paleta AutoCAD (ACI) básica para mostrar capas.
pub fn aci_color(i: u8) -> String {
    let base = [
        "#000000", "#ff0000", "#ffff00", "#00ff00", "#00ffff", "#0000ff", "#ff00ff", "#000000", "#808080", "#c0c0c0",
    ];
    if (i as usize) < base.len() {
        return base[i as usize].into();
    }
    // aproximación del resto del círculo cromático ACI
    let hue = ((i as f64 - 10.0) / 240.0 * 360.0) % 360.0;
    let shade = 1.0 - ((i % 10) / 2) as f64 * 0.15;
    hsv_hex(hue, if i % 2 == 0 { 1.0 } else { 0.5 }, shade)
}

pub fn hsv_hex(h: f64, s: f64, v: f64) -> String {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    format!("#{:02x}{:02x}{:02x}", ((r + m) * 255.0) as u8, ((g + m) * 255.0) as u8, ((b + m) * 255.0) as u8)
}

/// Aplana un arco (centro, radio, ángulos en radianes, sentido antihorario de a0 a a1).
pub fn flatten_arc(c: Pt, r: f64, a0: f64, mut a1: f64) -> Vec<Pt> {
    while a1 <= a0 {
        a1 += std::f64::consts::TAU;
    }
    let sweep = a1 - a0;
    let n = ((sweep / (std::f64::consts::PI / 24.0)).ceil() as usize).clamp(4, 96);
    (0..=n).map(|i| {
        let a = a0 + sweep * i as f64 / n as f64;
        Pt { x: c.x + r * a.cos(), y: c.y + r * a.sin() }
    }).collect()
}

/// Aplana el tramo de polilínea con "bulge" entre p y q (sin incluir p).
pub fn flatten_bulge(p: Pt, q: Pt, bulge: f64) -> Vec<Pt> {
    if bulge.abs() < 1e-9 {
        return vec![q];
    }
    let theta = 4.0 * bulge.atan();
    let chord = q - p;
    let d = chord.len();
    if d < 1e-12 {
        return vec![q];
    }
    let r = d / (2.0 * (theta / 2.0).sin());
    let mid = p.lerp(q, 0.5);
    let h = r * (theta / 2.0).cos();
    let nrm = chord.perp().norm();
    let c = mid + nrm * h;
    let a0 = (p.y - c.y).atan2(p.x - c.x);
    let n = ((theta.abs() / (std::f64::consts::PI / 24.0)).ceil() as usize).clamp(2, 96);
    (1..=n).map(|i| {
        let a = a0 + theta * i as f64 / n as f64;
        Pt { x: c.x + r.abs() * a.cos(), y: c.y + r.abs() * a.sin() }
    }).collect()
}

/// Importa desde bytes según la extensión.
pub fn import_bytes(name: &str, bytes: &[u8]) -> Result<Drawing, String> {
    let lower = name.to_lowercase();
    if lower.ends_with(".pdf") {
        pdf_in::read_pdf(name, bytes)
    } else if lower.ends_with(".dxf") {
        dxf_in::read_dxf(name, bytes)
    } else if lower.ends_with(".dwg") {
        Err("DWG files are first converted to DXF with ODA File Converter or LibreDWG (see Settings).".into())
    } else {
        Err(format!("Formato no soportado: {name}"))
    }
}
