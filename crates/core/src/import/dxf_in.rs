//! Lector DXF: expande bloques (INSERT) con sus transformaciones, aplana arcos, elipses,
//! polilíneas con "bulge" y splines, y conserva textos y capas.

use super::{aci_color, flatten_arc, flatten_bulge, Drawing, SourceKind};
use crate::geom::{pt, Affine, Pt};
use dxf::entities::{Entity, EntityType};
use dxf::enums::Units;
use std::collections::HashMap;

pub fn read_dxf(name: &str, bytes: &[u8]) -> Result<Drawing, String> {
    let mut cur = std::io::Cursor::new(bytes);
    let dx = dxf::Drawing::load(&mut cur).map_err(|e| format!("Could not read the DXF: {e}"))?;
    let mut d = Drawing::new(name, SourceKind::Dxf);

    let layer_colors: HashMap<String, String> = dx
        .layers()
        .map(|l| (l.name.clone(), l.color.index().map(aci_color).unwrap_or_else(|| "#444444".into())))
        .collect();
    let blocks: HashMap<String, &dxf::Block> = dx.blocks().map(|b| (b.name.clone(), b)).collect();

    let (unit, note) = match dx.header.default_drawing_units {
        Units::Millimeters => (Some(1.0), "File units: millimeters"),
        Units::Centimeters => (Some(10.0), "File units: centimeters"),
        Units::Meters => (Some(1000.0), "File units: meters"),
        Units::Inches => (Some(25.4), "File units: inches"),
        Units::Feet => (Some(304.8), "File units: feet"),
        Units::Decimeters => (Some(100.0), "File units: decimeters"),
        _ => (None, "The file does not declare units: they are estimated"),
    };
    d.declared_unit_mm = unit;
    d.unit_note = note.into();

    let mut ctx = Ctx { d: &mut d, blocks: &blocks, colors: &layer_colors };
    for e in dx.entities() {
        if e.common.is_in_paper_space {
            continue;
        }
        ctx.entity(e, &Affine::IDENTITY, None, 0);
    }
    d.finalize();
    if d.paths.is_empty() {
        return Err("The DXF has no geometry in model space.".into());
    }
    Ok(d)
}

struct Ctx<'a, 'b> {
    d: &'a mut Drawing,
    blocks: &'a HashMap<String, &'b dxf::Block>,
    colors: &'a HashMap<String, String>,
}

impl<'a, 'b> Ctx<'a, 'b> {
    fn layer(&mut self, e: &Entity, parent: Option<&str>) -> u32 {
        let name = if (e.common.layer == "0" || e.common.layer.is_empty()) && parent.is_some() {
            parent.unwrap().to_string()
        } else {
            e.common.layer.clone()
        };
        let color = self.colors.get(&name).cloned().unwrap_or_else(|| "#444444".into());
        self.d.layer_id(&name, &color)
    }

    fn entity(&mut self, e: &Entity, m: &Affine, parent: Option<&str>, depth: usize) {
        if depth > 10 {
            return;
        }
        let tp = |p: &dxf::Point| m.apply(pt(p.x, p.y));
        match &e.specific {
            EntityType::Line(l) => {
                let li = self.layer(e, parent);
                self.d.add_path(li, vec![tp(&l.p1), tp(&l.p2)], false);
            }
            EntityType::LwPolyline(pl) => {
                let li = self.layer(e, parent);
                let ocs = if pl.extrusion_direction.z < 0.0 { Affine::scale(-1.0, 1.0).then(m) } else { *m };
                let m = &ocs;
                let vs = &pl.vertices;
                if vs.is_empty() {
                    return;
                }
                let closed = pl.is_closed();
                let mut pts = vec![pt(vs[0].x, vs[0].y)];
                let n = vs.len();
                let segs = if closed { n } else { n - 1 };
                for i in 0..segs {
                    let a = pt(vs[i].x, vs[i].y);
                    let b = pt(vs[(i + 1) % n].x, vs[(i + 1) % n].y);
                    if vs[i].bulge.abs() > 1e-9 {
                        let mut arc = vec![a];
                        arc.extend(flatten_bulge(a, b, vs[i].bulge));
                        let arc_w: Vec<Pt> = arc.iter().map(|&p| m.apply(p)).collect();
                        self.d.add_curve(li, arc_w);
                    }
                    pts.extend(flatten_bulge(a, b, vs[i].bulge));
                }
                if closed && pts.len() > 1 && pts[0].dist(*pts.last().unwrap()) < 1e-9 {
                    pts.pop();
                }
                let pts: Vec<Pt> = pts.into_iter().map(|p| m.apply(p)).collect();
                self.d.add_path(li, pts, closed);
            }
            EntityType::Polyline(pl) => {
                let li = self.layer(e, parent);
                let vs: Vec<&dxf::entities::Vertex> = pl.vertices().collect();
                if vs.is_empty() {
                    return;
                }
                let closed = pl.is_closed();
                let n = vs.len();
                let mut pts = vec![pt(vs[0].location.x, vs[0].location.y)];
                let segs = if closed { n } else { n - 1 };
                for i in 0..segs {
                    let a = pt(vs[i].location.x, vs[i].location.y);
                    let b = pt(vs[(i + 1) % n].location.x, vs[(i + 1) % n].location.y);
                    pts.extend(flatten_bulge(a, b, vs[i].bulge));
                }
                if closed && pts.len() > 1 && pts[0].dist(*pts.last().unwrap()) < 1e-9 {
                    pts.pop();
                }
                let pts: Vec<Pt> = pts.into_iter().map(|p| m.apply(p)).collect();
                self.d.add_path(li, pts, closed);
            }
            EntityType::Arc(a) => {
                let li = self.layer(e, parent);
                let mirror = a.normal.z < 0.0;
                let c = pt(if mirror { -a.center.x } else { a.center.x }, a.center.y);
                let (s, en) = if mirror {
                    (180.0 - a.end_angle, 180.0 - a.start_angle)
                } else {
                    (a.start_angle, a.end_angle)
                };
                let pts = flatten_arc(c, a.radius, s.to_radians(), en.to_radians());
                let pts: Vec<Pt> = pts.into_iter().map(|p| m.apply(p)).collect();
                self.d.add_curve(li, pts.clone());
                self.d.add_path(li, pts, false);
            }
            EntityType::Circle(c) => {
                let li = self.layer(e, parent);
                let ocs = if c.normal.z < 0.0 { Affine::scale(-1.0, 1.0).then(m) } else { *m };
                let m = &ocs;
                let mut pts = flatten_arc(pt(c.center.x, c.center.y), c.radius, 0.0, std::f64::consts::TAU);
                pts.pop();
                let pts: Vec<Pt> = pts.into_iter().map(|p| m.apply(p)).collect();
                self.d.add_path(li, pts, true);
            }
            EntityType::Ellipse(el) => {
                let li = self.layer(e, parent);
                let c = pt(el.center.x, el.center.y);
                let maj = pt(el.major_axis.x, el.major_axis.y);
                let min = maj.perp() * el.minor_axis_ratio;
                let (t0, mut t1) = (el.start_parameter, el.end_parameter);
                while t1 <= t0 {
                    t1 += std::f64::consts::TAU;
                }
                let n = 48;
                let pts: Vec<Pt> = (0..=n)
                    .map(|i| {
                        let t = t0 + (t1 - t0) * i as f64 / n as f64;
                        m.apply(c + maj * t.cos() + min * t.sin())
                    })
                    .collect();
                let closed = (t1 - t0 - std::f64::consts::TAU).abs() < 1e-6;
                self.d.add_path(li, pts, closed);
            }
            EntityType::Spline(sp) => {
                let li = self.layer(e, parent);
                let src = if sp.fit_points.len() >= 2 { &sp.fit_points } else { &sp.control_points };
                let pts: Vec<Pt> = src.iter().map(|p| tp(p)).collect();
                self.d.add_path(li, pts, sp.is_closed());
            }
            EntityType::Solid(s) => {
                let li = self.layer(e, parent);
                // orden de vértices de SOLID: 1,2,4,3
                let pts = vec![tp(&s.first_corner), tp(&s.second_corner), tp(&s.fourth_corner), tp(&s.third_corner)];
                self.d.add_path(li, pts, true);
            }
            EntityType::Face3D(s) => {
                let li = self.layer(e, parent);
                let pts = vec![tp(&s.first_corner), tp(&s.second_corner), tp(&s.third_corner), tp(&s.fourth_corner)];
                self.d.add_path(li, pts, true);
            }
            EntityType::Text(t) => {
                let li = self.layer(e, parent);
                let p = tp(&t.location);
                self.d.texts.push(super::DText { layer: li, pos: p, height: t.text_height * m.scale_factor(), rot: t.rotation, text: t.value.clone() });
            }
            EntityType::MText(t) => {
                let li = self.layer(e, parent);
                let p = tp(&t.insertion_point);
                let mut s = t.extended_text.join("");
                s.push_str(&t.text);
                self.d.texts.push(super::DText { layer: li, pos: p, height: t.initial_text_height * m.scale_factor(), rot: t.rotation_angle, text: clean_mtext(&s) });
            }
            EntityType::Insert(ins) => {
                let Some(block) = self.blocks.get(&ins.name) else { return };
                let block_layer = if e.common.layer == "0" && parent.is_some() { parent.unwrap().to_string() } else { e.common.layer.clone() };
                let cols = ins.column_count.max(1) as i32;
                let rows = ins.row_count.max(1) as i32;
                let mirror = ins.extrusion_direction.z < 0.0;
                for r in 0..rows {
                    for c in 0..cols {
                        let off = pt(c as f64 * ins.column_spacing, r as f64 * ins.row_spacing);
                        let base = Affine::translate(-block.base_point.x, -block.base_point.y);
                        let sc = Affine::scale(ins.x_scale_factor, ins.y_scale_factor);
                        let rot = Affine::rotate(ins.rotation.to_radians());
                        let tr = Affine::translate(ins.location.x, ins.location.y);
                        let arr = Affine::translate(off.x, off.y);
                        let mut local = base.then(&sc).then(&arr).then(&rot).then(&tr);
                        if mirror {
                            // con extrusión (0,0,-1) el sistema OCS refleja el eje X
                            local = local.then(&Affine::scale(-1.0, 1.0));
                        }
                        let world = local.then(m);
                        for be in &block.entities {
                            self.entity(be, &world, Some(&block_layer), depth + 1);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn clean_mtext(s: &str) -> String {
    // elimina códigos de formato frecuentes de MTEXT
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('P') | Some('X') => out.push(' '),
                Some('f') | Some('F') | Some('H') | Some('W') | Some('C') | Some('c') | Some('T') | Some('Q') | Some('A') | Some('p') => {
                    for d in chars.by_ref() {
                        if d == ';' {
                            break;
                        }
                    }
                }
                Some(o) => out.push(o),
                None => {}
            },
            '{' | '}' => {}
            _ => out.push(c),
        }
    }
    out.trim().to_string()
}
