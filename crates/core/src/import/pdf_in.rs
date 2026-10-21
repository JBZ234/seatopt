//! Lector de PDF vectorial. Interpreta el flujo de contenido (trayectos, CTM, formularios XObject)
//! y conserva las capas (Optional Content Groups) que AutoCAD exporta con el PDF.

use super::{Drawing, SourceKind};
use crate::geom::{pt, Affine, Pt};
use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId};

pub fn read_pdf(name: &str, bytes: &[u8]) -> Result<Drawing, String> {
    let doc = Document::load_mem(bytes).map_err(|e| format!("Could not open the PDF: {e}"))?;
    let pages = doc.get_pages();
    let (&_num, &page_id) = pages.iter().next().ok_or("The PDF has no pages")?;
    let mut d = Drawing::new(name, SourceKind::Pdf);
    d.unit_note = "PDF: units are paper points; the scale must be set (calibrate).".into();
    // escala guardada por esta app al exportar
    if let Ok(info) = doc.trailer.get(b"Info").and_then(|i| doc.dereference(i).map(|(_, o)| o)).and_then(|o| o.as_dict()) {
        if let Ok(v) = info.get(b"SeatoptMmPerPt") {
            let mm = num(v);
            if mm > 0.0 {
                d.declared_unit_mm = Some(mm);
                let sc = info.get(b"SeatoptScale").map(num).unwrap_or(mm / (25.4 / 72.0));
                d.unit_note = format!("Scale saved in the PDF: 1:{sc:.0}");
            }
        }
    }

    let content = doc.get_page_content(page_id).map_err(|e| format!("Contenido ilegible: {e}"))?;
    let (inline_res, res_ids) = doc.get_page_resources(page_id).map_err(|e| format!("{e}"))?;
    let mut res: Vec<Dictionary> = Vec::new();
    if let Some(r) = inline_res {
        res.push(r.clone());
    }
    for id in res_ids {
        if let Ok(r) = doc.get_dictionary(id) {
            res.push(r.clone());
        }
    }
    let mut st = Interp { doc: &doc, d: &mut d, depth: 0 };
    let default_layer = st.d.layer_id("(no layer)", "#555555");
    st.run(&content, &res, Affine::IDENTITY, default_layer);
    d.finalize();
    if d.paths.is_empty() {
        return Err("The PDF has no vector strokes (is it a scan?). Export the PDF from AutoCAD or use the DXF.".into());
    }
    // quitar la capa vacía por defecto si no se usó
    Ok(d)
}

struct Interp<'a> {
    doc: &'a Document,
    d: &'a mut Drawing,
    depth: usize,
}

fn num(o: &Object) -> f64 {
    match o {
        Object::Integer(i) => *i as f64,
        Object::Real(r) => *r as f64,
        _ => 0.0,
    }
}

fn decode_pdf_string(b: &[u8]) -> String {
    if b.len() >= 2 && b[0] == 0xFE && b[1] == 0xFF {
        let u: Vec<u16> = b[2..].chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    } else {
        b.iter().map(|&c| c as char).collect()
    }
}

impl<'a> Interp<'a> {
    fn deref<'b>(&'b self, o: &'b Object) -> &'b Object {
        self.doc.dereference(o).map(|(_, o)| o).unwrap_or(o)
    }

    fn lookup(&self, res: &[Dictionary], cat: &[u8], key: &[u8]) -> Option<Object> {
        for r in res {
            if let Ok(c) = r.get(cat) {
                if let Ok(dict) = self.deref(c).as_dict() {
                    if let Ok(v) = dict.get(key) {
                        return Some(v.clone());
                    }
                }
            }
        }
        None
    }

    /// Nombre de capa de un OCG u OCMD.
    fn ocg_name(&self, o: &Object) -> Option<String> {
        let dict = self.deref(o).as_dict().ok()?;
        if let Ok(n) = dict.get(b"Name") {
            if let Object::String(s, _) = self.deref(n) {
                return Some(decode_pdf_string(s));
            }
        }
        if let Ok(ocgs) = dict.get(b"OCGs") {
            let ocgs = self.deref(ocgs);
            if let Ok(arr) = ocgs.as_array() {
                if let Some(first) = arr.first() {
                    return self.ocg_name(first);
                }
            } else {
                return self.ocg_name(ocgs);
            }
        }
        None
    }

    fn layer_for(&mut self, name: &str) -> u32 {
        let color = super::hsv_hex((fxhash(name) % 360) as f64, 0.55, 0.55);
        self.d.layer_id(name, &color)
    }

    fn run(&mut self, content: &[u8], res: &[Dictionary], base: Affine, layer0: u32) {
        if self.depth > 8 {
            return;
        }
        let Ok(ops) = Content::decode(content) else { return };
        let mut ctm = base;
        let mut stack: Vec<Affine> = Vec::new();
        let mut layers: Vec<u32> = vec![layer0];
        // trayecto actual
        let mut subpaths: Vec<(Vec<Pt>, bool)> = Vec::new();
        let mut cur: Vec<Pt> = Vec::new();
        let mut curves: Vec<Vec<Pt>> = Vec::new();
        let mut curve: Vec<Pt> = Vec::new();
        let mut last = pt(0.0, 0.0);
        let mut start = pt(0.0, 0.0);

        macro_rules! flush_curve {
            () => {
                if curve.len() >= 3 {
                    curves.push(std::mem::take(&mut curve));
                } else {
                    curve.clear();
                }
            };
        }
        macro_rules! flush_sub {
            ($closed:expr) => {
                flush_curve!();
                if cur.len() >= 2 {
                    subpaths.push((std::mem::take(&mut cur), $closed));
                } else {
                    cur.clear();
                }
            };
        }

        for op in ops.operations {
            let o = &op.operands;
            let n = |i: usize| o.get(i).map(num).unwrap_or(0.0);
            match op.operator.as_str() {
                "q" => stack.push(ctm),
                "Q" => {
                    if let Some(m) = stack.pop() {
                        ctm = m;
                    }
                }
                "cm" => {
                    let m = Affine::new(n(0), n(1), n(2), n(3), n(4), n(5));
                    ctm = m.then(&ctm);
                }
                "BDC" => {
                    let mut l = *layers.last().unwrap();
                    if o.first().and_then(|x| x.as_name().ok()) == Some(b"OC") {
                        let prop = o.get(1);
                        let name = match prop {
                            Some(Object::Name(nm)) => self.lookup(res, b"Properties", nm).and_then(|p| self.ocg_name(&p)),
                            Some(other) => self.ocg_name(other),
                            None => None,
                        };
                        if let Some(name) = name {
                            l = self.layer_for(&name);
                        }
                    }
                    layers.push(l);
                }
                "BMC" => {
                    let l = *layers.last().unwrap();
                    layers.push(l);
                }
                "EMC" => {
                    if layers.len() > 1 {
                        layers.pop();
                    }
                }
                "m" => {
                    flush_sub!(false);
                    last = ctm.apply(pt(n(0), n(1)));
                    start = last;
                    cur.push(last);
                }
                "l" => {
                    flush_curve!();
                    last = ctm.apply(pt(n(0), n(1)));
                    cur.push(last);
                }
                "c" | "v" | "y" => {
                    let (p1, p2, p3) = match op.operator.as_str() {
                        "c" => (ctm.apply(pt(n(0), n(1))), ctm.apply(pt(n(2), n(3))), ctm.apply(pt(n(4), n(5)))),
                        "v" => (last, ctm.apply(pt(n(0), n(1))), ctm.apply(pt(n(2), n(3)))),
                        _ => {
                            let p1 = ctm.apply(pt(n(0), n(1)));
                            let p3 = ctm.apply(pt(n(2), n(3)));
                            (p1, p3, p3)
                        }
                    };
                    let p0 = last;
                    if curve.is_empty() {
                        curve.push(p0);
                    }
                    let segs = 8;
                    for i in 1..=segs {
                        let t = i as f64 / segs as f64;
                        let mt = 1.0 - t;
                        let p = p0 * (mt * mt * mt) + p1 * (3.0 * mt * mt * t) + p2 * (3.0 * mt * t * t) + p3 * (t * t * t);
                        cur.push(p);
                        curve.push(p);
                    }
                    last = p3;
                }
                "h" => {
                    if cur.len() >= 2 {
                        flush_sub!(true);
                    }
                    last = start;
                }
                "re" => {
                    flush_sub!(false);
                    let (x, y, w, h) = (n(0), n(1), n(2), n(3));
                    let r = vec![
                        ctm.apply(pt(x, y)),
                        ctm.apply(pt(x + w, y)),
                        ctm.apply(pt(x + w, y + h)),
                        ctm.apply(pt(x, y + h)),
                    ];
                    subpaths.push((r, true));
                }
                "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" => {
                    let closing = matches!(op.operator.as_str(), "s" | "b" | "b*");
                    flush_sub!(closing);
                    let l = *layers.last().unwrap();
                    for (pts, closed) in subpaths.drain(..) {
                        self.d.add_path(l, pts, closed);
                    }
                    for c in curves.drain(..) {
                        self.d.add_curve(l, c);
                    }
                }
                "n" => {
                    cur.clear();
                    curve.clear();
                    subpaths.clear();
                    curves.clear();
                }
                "Do" => {
                    let Some(Object::Name(nm)) = o.first() else { continue };
                    let Some(xo) = self.lookup(res, b"XObject", nm) else { continue };
                    let (xid, xobj): (Option<ObjectId>, &Object) = match self.doc.dereference(&xo) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    let _ = xid;
                    let Ok(stream) = xobj.as_stream() else { continue };
                    if stream.dict.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) != Some(b"Form") {
                        continue;
                    }
                    let mut m = Affine::IDENTITY;
                    if let Ok(arr) = stream.dict.get(b"Matrix").and_then(|a| a.as_array()) {
                        if arr.len() == 6 {
                            m = Affine::new(num(&arr[0]), num(&arr[1]), num(&arr[2]), num(&arr[3]), num(&arr[4]), num(&arr[5]));
                        }
                    }
                    let mut l = *layers.last().unwrap();
                    if let Ok(oc) = stream.dict.get(b"OC") {
                        if let Some(name) = self.ocg_name(oc) {
                            l = self.layer_for(&name);
                        }
                    }
                    let mut sub_res: Vec<Dictionary> = Vec::new();
                    if let Ok(r) = stream.dict.get(b"Resources") {
                        if let Ok(rd) = self.deref(r).as_dict() {
                            sub_res.push(rd.clone());
                        }
                    }
                    sub_res.extend(res.iter().cloned());
                    let data = stream.decompressed_content().unwrap_or_else(|_| stream.content.clone());
                    self.depth += 1;
                    self.run(&data, &sub_res, m.then(&ctm), l);
                    self.depth -= 1;
                }
                _ => {}
            }
        }
    }
}

fn fxhash(s: &str) -> u64 {
    let mut h: u64 = 1469598103934665603;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    h
}
