//! Distancias geodésicas rápidas (Dijkstra con cubetas, costos 10/14 ≈ 1/√2).

use crate::raster::Frame;

pub const UNREACH: u32 = u32::MAX;

/// Distancia (en décimas de celda: 10 = una celda ortogonal) desde las semillas,
/// moviéndose solo por celdas transitables. Se detiene en `limit` (mismas unidades).
pub fn dial(f: &Frame, passable: &[bool], seeds: &[usize], limit: u32) -> Vec<u32> {
    let n = f.len();
    let mut dist = vec![UNREACH; n];
    const B: usize = 16;
    let mut buckets: Vec<Vec<usize>> = (0..B).map(|_| Vec::new()).collect();
    let mut count = 0usize;
    for &s in seeds {
        if s < n && passable[s] && dist[s] != 0 {
            dist[s] = 0;
            buckets[0].push(s);
            count += 1;
        }
    }
    let w = f.w as isize;
    let h = f.h as isize;
    let mut cur: u32 = 0;
    while count > 0 {
        let b = (cur as usize) % B;
        while let Some(i) = buckets[b].pop() {
            count -= 1;
            if dist[i] != cur {
                continue;
            }
            if cur > limit {
                continue;
            }
            let x = (i as isize) % w;
            let y = (i as isize) / w;
            for (dx, dy, c) in [(1, 0, 10u32), (-1, 0, 10), (0, 1, 10), (0, -1, 10), (1, 1, 14), (1, -1, 14), (-1, 1, 14), (-1, -1, 14)] {
                let nx = x + dx;
                let ny = y + dy;
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let j = (ny * w + nx) as usize;
                if !passable[j] {
                    continue;
                }
                if c == 14 && (!passable[(y * w + nx) as usize] || !passable[(ny * w + x) as usize]) {
                    continue;
                }
                let nd = cur + c;
                if nd < dist[j] {
                    dist[j] = nd;
                    buckets[(nd as usize) % B].push(j);
                    count += 1;
                }
            }
        }
        cur += 1;
    }
    dist
}

/// Convierte unidades de `dial` a mm.
#[inline]
pub fn to_mm(d: u32, res: f64) -> f64 {
    if d == UNREACH {
        f64::INFINITY
    } else {
        d as f64 * res / 10.0
    }
}

#[inline]
pub fn from_mm(mm: f64, res: f64) -> u32 {
    if !mm.is_finite() {
        u32::MAX - 1
    } else {
        (mm * 10.0 / res).round().max(0.0) as u32
    }
}
