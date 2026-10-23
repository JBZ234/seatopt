//! Geometría 2D básica.

use serde::{Deserialize, Serialize};
use std::ops::{Add, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

pub const fn pt(x: f64, y: f64) -> Pt {
    Pt { x, y }
}

impl Pt {
    pub fn dot(self, o: Pt) -> f64 {
        self.x * o.x + self.y * o.y
    }
    pub fn cross(self, o: Pt) -> f64 {
        self.x * o.y - self.y * o.x
    }
    pub fn len(self) -> f64 {
        self.x.hypot(self.y)
    }
    pub fn dist(self, o: Pt) -> f64 {
        (self - o).len()
    }
    pub fn norm(self) -> Pt {
        let l = self.len();
        if l < 1e-12 {
            pt(0.0, 0.0)
        } else {
            pt(self.x / l, self.y / l)
        }
    }
    pub fn perp(self) -> Pt {
        pt(-self.y, self.x)
    }
    pub fn lerp(self, o: Pt, t: f64) -> Pt {
        pt(self.x + (o.x - self.x) * t, self.y + (o.y - self.y) * t)
    }
    pub fn rot(self, ang: f64) -> Pt {
        let (s, c) = ang.sin_cos();
        pt(self.x * c - self.y * s, self.x * s + self.y * c)
    }
    pub fn from_angle(ang: f64) -> Pt {
        pt(ang.cos(), ang.sin())
    }
}

impl Add for Pt {
    type Output = Pt;
    fn add(self, o: Pt) -> Pt {
        pt(self.x + o.x, self.y + o.y)
    }
}
impl Sub for Pt {
    type Output = Pt;
    fn sub(self, o: Pt) -> Pt {
        pt(self.x - o.x, self.y - o.y)
    }
}
impl Mul<f64> for Pt {
    type Output = Pt;
    fn mul(self, k: f64) -> Pt {
        pt(self.x * k, self.y * k)
    }
}
impl Neg for Pt {
    type Output = Pt;
    fn neg(self) -> Pt {
        pt(-self.x, -self.y)
    }
}

/// Transformación afín 2D: [a c e; b d f]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Affine {
    pub const IDENTITY: Affine = Affine { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };
    pub fn new(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self {
        Affine { a, b, c, d, e, f }
    }
    pub fn translate(x: f64, y: f64) -> Self {
        Affine::new(1.0, 0.0, 0.0, 1.0, x, y)
    }
    pub fn scale(sx: f64, sy: f64) -> Self {
        Affine::new(sx, 0.0, 0.0, sy, 0.0, 0.0)
    }
    pub fn rotate(ang: f64) -> Self {
        let (s, c) = ang.sin_cos();
        Affine::new(c, s, -s, c, 0.0, 0.0)
    }
    /// self ∘ other : primero `other`, luego `self`.
    pub fn then(&self, outer: &Affine) -> Affine {
        // outer * self
        let m = self;
        let o = outer;
        Affine {
            a: o.a * m.a + o.c * m.b,
            b: o.b * m.a + o.d * m.b,
            c: o.a * m.c + o.c * m.d,
            d: o.b * m.c + o.d * m.d,
            e: o.a * m.e + o.c * m.f + o.e,
            f: o.b * m.e + o.d * m.f + o.f,
        }
    }
    pub fn apply(&self, p: Pt) -> Pt {
        pt(self.a * p.x + self.c * p.y + self.e, self.b * p.x + self.d * p.y + self.f)
    }
    pub fn scale_factor(&self) -> f64 {
        (self.a * self.d - self.b * self.c).abs().sqrt()
    }
    /// Rotation of the x axis, in degrees.
    pub fn rotation_deg(&self) -> f64 {
        self.b.atan2(self.a).to_degrees()
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct BBox {
    pub min: Pt,
    pub max: Pt,
}

impl BBox {
    pub fn empty() -> Self {
        BBox { min: pt(f64::INFINITY, f64::INFINITY), max: pt(f64::NEG_INFINITY, f64::NEG_INFINITY) }
    }
    pub fn add(&mut self, p: Pt) {
        self.min.x = self.min.x.min(p.x);
        self.min.y = self.min.y.min(p.y);
        self.max.x = self.max.x.max(p.x);
        self.max.y = self.max.y.max(p.y);
    }
    pub fn of(pts: impl IntoIterator<Item = Pt>) -> Self {
        let mut b = BBox::empty();
        for p in pts {
            b.add(p);
        }
        b
    }
    pub fn is_valid(&self) -> bool {
        self.min.x <= self.max.x && self.min.y <= self.max.y
    }
    pub fn w(&self) -> f64 {
        self.max.x - self.min.x
    }
    pub fn h(&self) -> f64 {
        self.max.y - self.min.y
    }
    pub fn expand(&self, m: f64) -> BBox {
        BBox { min: pt(self.min.x - m, self.min.y - m), max: pt(self.max.x + m, self.max.y + m) }
    }
}

pub fn polygon_area(poly: &[Pt]) -> f64 {
    let n = poly.len();
    if n < 3 {
        return 0.0;
    }
    let mut s = 0.0;
    for i in 0..n {
        s += poly[i].cross(poly[(i + 1) % n]);
    }
    s * 0.5
}

pub fn centroid(poly: &[Pt]) -> Pt {
    let a = polygon_area(poly);
    if a.abs() < 1e-9 {
        let n = poly.len().max(1) as f64;
        return poly.iter().fold(pt(0.0, 0.0), |s, &p| s + p) * (1.0 / n);
    }
    let n = poly.len();
    let mut cx = 0.0;
    let mut cy = 0.0;
    for i in 0..n {
        let p = poly[i];
        let q = poly[(i + 1) % n];
        let c = p.cross(q);
        cx += (p.x + q.x) * c;
        cy += (p.y + q.y) * c;
    }
    pt(cx / (6.0 * a), cy / (6.0 * a))
}

pub fn point_in_polygon(p: Pt, poly: &[Pt]) -> bool {
    let n = poly.len();
    let mut inside = false;
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let a = poly[i];
        let b = poly[j];
        if (a.y > p.y) != (b.y > p.y) {
            let x = (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x;
            if p.x < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Regla par-impar sobre varios contornos.
pub fn point_in_region(p: Pt, rings: &[Vec<Pt>]) -> bool {
    let mut c = 0;
    for r in rings {
        if point_in_polygon(p, r) {
            c += 1;
        }
    }
    c % 2 == 1
}

pub fn dist_point_segment(p: Pt, a: Pt, b: Pt) -> f64 {
    let ab = b - a;
    let l2 = ab.dot(ab);
    if l2 < 1e-12 {
        return p.dist(a);
    }
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    p.dist(a + ab * t)
}

pub fn closest_on_segment(p: Pt, a: Pt, b: Pt) -> Pt {
    let ab = b - a;
    let l2 = ab.dot(ab);
    if l2 < 1e-12 {
        return a;
    }
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    a + ab * t
}

pub fn dist_point_polygon_boundary(p: Pt, poly: &[Pt]) -> f64 {
    let n = poly.len();
    let mut d = f64::INFINITY;
    for i in 0..n {
        d = d.min(dist_point_segment(p, poly[i], poly[(i + 1) % n]));
    }
    d
}

/// Distancia de un punto a un polígono relleno (0 si está dentro).
pub fn dist_point_polygon(p: Pt, poly: &[Pt]) -> f64 {
    if point_in_polygon(p, poly) {
        0.0
    } else {
        dist_point_polygon_boundary(p, poly)
    }
}

/// Simplificación Douglas–Peucker (polilínea abierta).
pub fn simplify(pts: &[Pt], tol: f64) -> Vec<Pt> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0usize, pts.len() - 1)];
    while let Some((s, e)) = stack.pop() {
        let mut best = 0.0;
        let mut idx = s;
        for i in s + 1..e {
            let d = dist_point_segment(pts[i], pts[s], pts[e]);
            if d > best {
                best = d;
                idx = i;
            }
        }
        if best > tol && idx > s {
            keep[idx] = true;
            stack.push((s, idx));
            stack.push((idx, e));
        }
    }
    pts.iter().zip(keep).filter(|(_, k)| *k).map(|(p, _)| *p).collect()
}

/// Simplifica un anillo cerrado.
pub fn simplify_ring(ring: &[Pt], tol: f64) -> Vec<Pt> {
    if ring.len() < 4 {
        return ring.to_vec();
    }
    // Partir en el punto más lejano del primero para que DP funcione en anillos.
    let far = (0..ring.len()).max_by(|&i, &j| ring[0].dist(ring[i]).partial_cmp(&ring[0].dist(ring[j])).unwrap()).unwrap();
    let mut a: Vec<Pt> = ring[0..=far].to_vec();
    let mut b: Vec<Pt> = ring[far..].to_vec();
    b.push(ring[0]);
    a = simplify(&a, tol);
    b = simplify(&b, tol);
    a.pop();
    b.pop();
    a.extend(b);
    a
}

/// Rectángulo orientado: centro, eje (ángulo), medio largo, medio fondo.
pub fn oriented_rect(center: Pt, ang: f64, half_len: f64, half_dep: f64) -> Vec<Pt> {
    let u = Pt::from_angle(ang);
    let v = u.perp();
    vec![
        center - u * half_len - v * half_dep,
        center + u * half_len - v * half_dep,
        center + u * half_len + v * half_dep,
        center - u * half_len + v * half_dep,
    ]
}

/// Ángulo normalizado a [0, 180).
pub fn norm_angle_180(deg: f64) -> f64 {
    let mut a = deg % 180.0;
    if a < 0.0 {
        a += 180.0;
    }
    if a >= 180.0 - 1e-9 {
        a -= 180.0;
    }
    a
}

/// Intersección de dos segmentos (si se cruzan propiamente o se tocan).
pub fn seg_intersect(a: Pt, b: Pt, c: Pt, d: Pt) -> Option<Pt> {
    let r = b - a;
    let s = d - c;
    let den = r.cross(s);
    if den.abs() < 1e-12 {
        return None;
    }
    let t = (c - a).cross(s) / den;
    let u = (c - a).cross(r) / den;
    if (-1e-9..=1.0 + 1e-9).contains(&t) && (-1e-9..=1.0 + 1e-9).contains(&u) {
        Some(a + r * t)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn area_and_pip() {
        let sq = vec![pt(0., 0.), pt(10., 0.), pt(10., 10.), pt(0., 10.)];
        assert!((polygon_area(&sq) - 100.0).abs() < 1e-9);
        assert!(point_in_polygon(pt(5., 5.), &sq));
        assert!(!point_in_polygon(pt(15., 5.), &sq));
    }
    #[test]
    fn affine_compose() {
        let t = Affine::translate(10.0, 0.0);
        let r = Affine::rotate(std::f64::consts::FRAC_PI_2);
        // rotar y luego trasladar
        let m = r.then(&t);
        let p = m.apply(pt(1.0, 0.0));
        assert!((p.x - 10.0).abs() < 1e-9 && (p.y - 1.0).abs() < 1e-9);
    }
}
