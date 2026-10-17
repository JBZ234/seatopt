//! Rejillas raster: rasterización de polígonos, transformada de distancia exacta,
//! BFS / distancias geodésicas, componentes conexas y contornos.

use crate::geom::{pt, BBox, Pt};
use std::collections::{BinaryHeap, VecDeque};

/// Marco de una rejilla en coordenadas del mundo (mm).
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub origin: Pt,
    pub res: f64,
    pub w: usize,
    pub h: usize,
}

impl Frame {
    pub fn covering(b: BBox, res: f64, margin: f64) -> Frame {
        let b = b.expand(margin);
        let w = ((b.w() / res).ceil() as usize).max(1);
        let h = ((b.h() / res).ceil() as usize).max(1);
        Frame { origin: b.min, res, w, h }
    }
    #[inline]
    pub fn len(&self) -> usize {
        self.w * self.h
    }
    #[inline]
    pub fn idx(&self, x: usize, y: usize) -> usize {
        y * self.w + x
    }
    #[inline]
    pub fn center(&self, x: usize, y: usize) -> Pt {
        pt(self.origin.x + (x as f64 + 0.5) * self.res, self.origin.y + (y as f64 + 0.5) * self.res)
    }
    #[inline]
    pub fn center_i(&self, i: usize) -> Pt {
        self.center(i % self.w, i / self.w)
    }
    /// Celda que contiene el punto (puede estar fuera).
    #[inline]
    pub fn cell_f(&self, p: Pt) -> (f64, f64) {
        ((p.x - self.origin.x) / self.res, (p.y - self.origin.y) / self.res)
    }
    pub fn cell(&self, p: Pt) -> Option<(usize, usize)> {
        let (fx, fy) = self.cell_f(p);
        if fx < 0.0 || fy < 0.0 {
            return None;
        }
        let (x, y) = (fx as usize, fy as usize);
        if x < self.w && y < self.h {
            Some((x, y))
        } else {
            None
        }
    }
    pub fn cell_idx(&self, p: Pt) -> Option<usize> {
        self.cell(p).map(|(x, y)| self.idx(x, y))
    }
}

pub type Mask = Vec<bool>;

/// Rellena (con `val`) las celdas cuyo centro cae dentro del polígono.
pub fn fill_polygon<T: Copy>(f: &Frame, data: &mut [T], poly: &[Pt], val: T) {
    fill_rings(f, data, std::slice::from_ref(&poly.to_vec()), val)
}

/// Relleno par-impar de varios anillos, por barrido de líneas en centros de celda.
pub fn fill_rings<T: Copy>(f: &Frame, data: &mut [T], rings: &[Vec<Pt>], val: T) {
    let mut bb = BBox::empty();
    for r in rings {
        for &p in r {
            bb.add(p);
        }
    }
    if !bb.is_valid() {
        return;
    }
    let y0 = (((bb.min.y - f.origin.y) / f.res - 0.5).floor().max(0.0)) as usize;
    let y1 = ((((bb.max.y - f.origin.y) / f.res - 0.5).ceil()).max(0.0) as usize).min(f.h.saturating_sub(1));
    let mut xs: Vec<f64> = Vec::new();
    for y in y0..=y1 {
        if y >= f.h {
            break;
        }
        let cy = f.origin.y + (y as f64 + 0.5) * f.res;
        xs.clear();
        for r in rings {
            let n = r.len();
            if n < 3 {
                continue;
            }
            for i in 0..n {
                let a = r[i];
                let b = r[(i + 1) % n];
                if (a.y > cy) != (b.y > cy) {
                    xs.push(a.x + (cy - a.y) * (b.x - a.x) / (b.y - a.y));
                }
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for pair in xs.chunks(2) {
            if pair.len() < 2 {
                break;
            }
            // celdas cuyo centro está en [x0, x1)
            let xa = ((pair[0] - f.origin.x) / f.res - 0.5).ceil().max(0.0) as i64;
            let xb = ((pair[1] - f.origin.x) / f.res - 0.5).ceil() as i64 - 1;
            let xb = xb.min(f.w as i64 - 1);
            if xb < xa {
                continue;
            }
            let base = y * f.w;
            for x in xa..=xb {
                data[base + x as usize] = val;
            }
        }
    }
}

/// Marca las celdas cuyo centro está a ≤ `half` de un segmento (trazo grueso).
pub fn stroke_segment<T: Copy>(f: &Frame, data: &mut [T], a: Pt, b: Pt, half: f64, val: T) {
    let half = half.max(f.res * 0.5);
    let bb = BBox::of([a, b]).expand(half + f.res);
    let (x0, y0) = f.cell_f(bb.min);
    let (x1, y1) = f.cell_f(bb.max);
    let x0 = x0.floor().max(0.0) as usize;
    let y0 = y0.floor().max(0.0) as usize;
    let x1 = (x1.ceil() as usize).min(f.w.saturating_sub(1));
    let y1 = (y1.ceil() as usize).min(f.h.saturating_sub(1));
    if x0 > x1 || y0 > y1 {
        return;
    }
    for y in y0..=y1 {
        for x in x0..=x1 {
            let c = f.center(x, y);
            if crate::geom::dist_point_segment(c, a, b) <= half {
                data[f.idx(x, y)] = val;
            }
        }
    }
}

/// Transformada de distancia euclidiana exacta (Felzenszwalb–Huttenlocher).
/// Devuelve, para cada celda, la distancia en mm a la celda fuente más cercana.
pub fn edt(f: &Frame, source: &[bool]) -> Vec<f32> {
    let (w, h) = (f.w, f.h);
    const INF: f64 = 1e20;
    let mut g = vec![0f64; w * h];
    for i in 0..w * h {
        g[i] = if source[i] { 0.0 } else { INF };
    }
    let n = w.max(h);
    let mut fbuf = vec![0f64; n];
    let mut d = vec![0f64; n];
    let mut v = vec![0usize; n];
    let mut z = vec![0f64; n + 1];
    // columnas
    for x in 0..w {
        for y in 0..h {
            fbuf[y] = g[y * w + x];
        }
        dt1d(&fbuf[..h], &mut d[..h], &mut v, &mut z);
        for y in 0..h {
            g[y * w + x] = d[y];
        }
    }
    // filas
    for y in 0..h {
        fbuf[..w].copy_from_slice(&g[y * w..y * w + w]);
        dt1d(&fbuf[..w], &mut d[..w], &mut v, &mut z);
        g[y * w..y * w + w].copy_from_slice(&d[..w]);
    }
    g.iter().map(|&s| (s.sqrt() * f.res) as f32).collect()
}

fn dt1d(f: &[f64], d: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        loop {
            let p = v[k];
            let s = ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * q as f64 - 2.0 * p as f64);
            if s <= z[k] {
                if k == 0 {
                    v[0] = q;
                    z[0] = f64::NEG_INFINITY;
                    z[1] = f64::INFINITY;
                    break;
                }
                k -= 1;
                continue;
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = f64::INFINITY;
            break;
        }
    }
    k = 0;
    for q in 0..n {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let p = v[k];
        let dq = q as f64 - p as f64;
        d[q] = dq * dq + f[p];
    }
}

/// Dilata una máscara un radio (mm).
pub fn dilate(f: &Frame, m: &[bool], r: f64) -> Mask {
    if r <= 0.0 {
        return m.to_vec();
    }
    let d = edt(f, m);
    d.iter().map(|&x| (x as f64) <= r + 1e-6).collect()
}

/// Erosiona una máscara un radio (mm).
pub fn erode(f: &Frame, m: &[bool], r: f64) -> Mask {
    if r <= 0.0 {
        return m.to_vec();
    }
    let inv: Mask = m.iter().map(|&b| !b).collect();
    let d = edt(f, &inv);
    d.iter().map(|&x| (x as f64) > r + 1e-6).collect()
}

const N8: [(i32, i32); 8] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];
const N4: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// Inundación (4-conexa) desde semillas sobre celdas transitables.
pub fn flood(f: &Frame, passable: &[bool], seeds: &[usize]) -> Mask {
    let mut out = vec![false; f.len()];
    let mut q = VecDeque::new();
    for &s in seeds {
        if s < out.len() && passable[s] && !out[s] {
            out[s] = true;
            q.push_back(s);
        }
    }
    while let Some(i) = q.pop_front() {
        let (x, y) = ((i % f.w) as i32, (i / f.w) as i32);
        for (dx, dy) in N4 {
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || ny < 0 || nx >= f.w as i32 || ny >= f.h as i32 {
                continue;
            }
            let j = ny as usize * f.w + nx as usize;
            if passable[j] && !out[j] {
                out[j] = true;
                q.push_back(j);
            }
        }
    }
    out
}

/// Distancia geodésica (8-conexa, mm) desde semillas; f32::INFINITY si no se alcanza.
/// Se detiene al superar `limit`.
pub fn geodesic(f: &Frame, passable: &[bool], seeds: &[usize], limit: f64) -> Vec<f32> {
    let mut dist = vec![f32::INFINITY; f.len()];
    #[derive(PartialEq)]
    struct St(f32, usize);
    impl Eq for St {}
    impl PartialOrd for St {
        fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(o))
        }
    }
    impl Ord for St {
        fn cmp(&self, o: &Self) -> std::cmp::Ordering {
            o.0.partial_cmp(&self.0).unwrap_or(std::cmp::Ordering::Equal)
        }
    }
    let mut heap = BinaryHeap::new();
    for &s in seeds {
        if s < dist.len() && passable[s] {
            dist[s] = 0.0;
            heap.push(St(0.0, s));
        }
    }
    let r = f.res as f32;
    let diag = r * std::f32::consts::SQRT_2;
    while let Some(St(d, i)) = heap.pop() {
        if d > dist[i] {
            continue;
        }
        if d as f64 > limit {
            break;
        }
        let (x, y) = ((i % f.w) as i32, (i / f.w) as i32);
        for (k, (dx, dy)) in N8.iter().enumerate() {
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || ny < 0 || nx >= f.w as i32 || ny >= f.h as i32 {
                continue;
            }
            let j = ny as usize * f.w + nx as usize;
            if !passable[j] {
                continue;
            }
            if k >= 4 {
                // evitar cortar esquinas
                let a = y as usize * f.w + nx as usize;
                let b = ny as usize * f.w + x as usize;
                if !passable[a] || !passable[b] {
                    continue;
                }
            }
            let nd = d + if k < 4 { r } else { diag };
            if nd < dist[j] {
                dist[j] = nd;
                heap.push(St(nd, j));
            }
        }
    }
    dist
}

/// Etiquetado de componentes 4-conexas. Devuelve (etiquetas [0 = fondo], tamaños por etiqueta).
pub fn components(f: &Frame, m: &[bool]) -> (Vec<u32>, Vec<usize>) {
    let mut lab = vec![0u32; f.len()];
    let mut sizes = vec![0usize];
    let mut q = VecDeque::new();
    for s in 0..f.len() {
        if !m[s] || lab[s] != 0 {
            continue;
        }
        let id = sizes.len() as u32;
        sizes.push(0);
        lab[s] = id;
        q.push_back(s);
        while let Some(i) = q.pop_front() {
            sizes[id as usize] += 1;
            let (x, y) = ((i % f.w) as i32, (i / f.w) as i32);
            for (dx, dy) in N4 {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= f.w as i32 || ny >= f.h as i32 {
                    continue;
                }
                let j = ny as usize * f.w + nx as usize;
                if m[j] && lab[j] == 0 {
                    lab[j] = id;
                    q.push_back(j);
                }
            }
        }
    }
    (lab, sizes)
}

/// Contornos de una máscara (bordes de celdas), como anillos cerrados en mm.
/// Los anillos externos y los huecos salen con orientación opuesta.
pub fn contours(f: &Frame, m: &[bool]) -> Vec<Vec<Pt>> {
    use std::collections::HashMap;
    let at = |x: i64, y: i64| -> bool {
        if x < 0 || y < 0 || x >= f.w as i64 || y >= f.h as i64 {
            false
        } else {
            m[y as usize * f.w + x as usize]
        }
    };
    // Aristas dirigidas con el interior a la izquierda, entre vértices de la malla (x,y) enteros.
    let mut next: HashMap<(i64, i64), Vec<(i64, i64)>> = HashMap::new();
    for y in 0..f.h as i64 {
        for x in 0..f.w as i64 {
            if !at(x, y) {
                continue;
            }
            if !at(x, y - 1) {
                next.entry((x, y)).or_default().push((x + 1, y));
            }
            if !at(x + 1, y) {
                next.entry((x + 1, y)).or_default().push((x + 1, y + 1));
            }
            if !at(x, y + 1) {
                next.entry((x + 1, y + 1)).or_default().push((x, y + 1));
            }
            if !at(x - 1, y) {
                next.entry((x, y + 1)).or_default().push((x, y));
            }
        }
    }
    let mut rings = Vec::new();
    let keys: Vec<(i64, i64)> = next.keys().cloned().collect();
    for k in keys {
        loop {
            let start = match next.get_mut(&k) {
                Some(v) if !v.is_empty() => k,
                _ => break,
            };
            let mut ring = vec![start];
            let mut cur = start;
            let mut prev_dir = (0i64, 0i64);
            loop {
                let v = next.get_mut(&cur).unwrap();
                // en vértices con dos salidas, girar a la izquierda primero (separa toques en diagonal)
                let pick = if v.len() > 1 {
                    let mut best = 0;
                    let mut bestv = i64::MIN;
                    for (i, n) in v.iter().enumerate() {
                        let d = (n.0 - cur.0, n.1 - cur.1);
                        let c = prev_dir.0 * d.1 - prev_dir.1 * d.0;
                        if c > bestv {
                            bestv = c;
                            best = i;
                        }
                    }
                    best
                } else {
                    0
                };
                let n = v.swap_remove(pick);
                prev_dir = (n.0 - cur.0, n.1 - cur.1);
                cur = n;
                if cur == start {
                    break;
                }
                ring.push(cur);
            }
            // quitar vértices colineales
            let mut out: Vec<Pt> = Vec::new();
            let len = ring.len();
            for i in 0..len {
                let a = ring[(i + len - 1) % len];
                let b = ring[i];
                let c = ring[(i + 1) % len];
                if (b.0 - a.0) * (c.1 - b.1) - (b.1 - a.1) * (c.0 - b.0) != 0 {
                    out.push(pt(f.origin.x + b.0 as f64 * f.res, f.origin.y + b.1 as f64 * f.res));
                }
            }
            if out.len() >= 3 {
                rings.push(out);
            }
        }
    }
    rings
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edt_basic() {
        let f = Frame { origin: pt(0.0, 0.0), res: 10.0, w: 10, h: 1 };
        let mut s = vec![false; 10];
        s[0] = true;
        let d = edt(&f, &s);
        assert!((d[5] - 50.0).abs() < 1e-3);
    }
    #[test]
    fn fill_and_contour() {
        let f = Frame { origin: pt(0.0, 0.0), res: 100.0, w: 20, h: 20 };
        let mut m = vec![false; f.len()];
        fill_polygon(&f, &mut m, &[pt(250.0, 250.0), pt(1250.0, 250.0), pt(1250.0, 850.0), pt(250.0, 850.0)], true);
        assert_eq!(m.iter().filter(|&&b| b).count(), 10 * 6);
        let c = contours(&f, &m);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].len(), 4);
    }
}
