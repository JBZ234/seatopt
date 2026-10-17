//! Plano «de líneas simples»: a partir del dibujo original deja solo los muros como una línea
//! (eje del muro), las columnas, las puertas (hoja + arco) y el nombre de las oficinas.
//!
//! Los muros dobles, fachadas y vidrios se rasterizan, se funden con una dilatación y se
//! adelgazan (Zhang–Suen) hasta quedar en una línea de una celda; esa línea se recorre como
//! polilíneas, se simplifica (Douglas–Peucker) y se podan las ramitas cortas.

use crate::analyze::{to_mm, AnalyzeOptions, LayerRole};
use crate::export::{dxf_text, insunits_code};
use crate::geom::*;
use crate::import::{Drawing, SourceKind};
use crate::model::*;
use crate::raster::{dilate, erode, stroke_segment, Frame};
use std::collections::HashMap;
use std::fmt::Write;

pub struct SimplePlan {
    /// Ejes de muros (mm).
    pub walls: Vec<Vec<Pt>>,
    pub columns: Vec<Vec<Pt>>,
    /// Escaleras y ascensores como un solo bloque.
    pub cores: Vec<Vec<Pt>>,
    pub doors: Vec<Door>,
    /// Puntos interiores del área amoblable (para orientar el barrido de las puertas).
    pub region: Vec<Vec<Pt>>,
    pub labels: Vec<(Pt, String)>,
}

pub struct SimpleOpts {
    /// Tamaño de celda (mm).
    pub res: f64,
    /// Cuánto se engordan los trazos para fundir líneas dobles (mm).
    pub merge: f64,
    /// Tolerancia de simplificación (mm).
    pub tol: f64,
    /// Ramas sueltas más cortas que esto se borran (mm).
    pub spur: f64,
}

impl Default for SimpleOpts {
    fn default() -> Self {
        SimpleOpts { res: 40.0, merge: 170.0, tol: 100.0, spur: 700.0 }
    }
}

fn skip_layer(name: &str) -> bool {
    let n = name.to_uppercase();
    ["STAIR", "ESCALER", "FIXT", "SANIT", "ELEV", "ASCENS"].iter().any(|k| n.contains(k))
}

pub fn simple_plan(d: &Drawing, opts: &AnalyzeOptions, plan: &Plan, so: &SimpleOpts) -> SimplePlan {
    // Zona de interés: edificio (o área amoblable) con margen.
    let pts: Vec<Pt> = if plan.building.len() >= 3 { plan.building.clone() } else { plan.region.iter().flatten().copied().collect() };
    let zone = BBox::of(pts.iter().copied()).expand(1500.0);

    // Capas de muro sin escaleras, sanitarios ni ascensores (esos meten manchas).
    let mut ro = opts.clone();
    for (k, v) in ro.roles.iter_mut() {
        if skip_layer(k) && *v == LayerRole::Wall {
            *v = LayerRole::Ignore;
        }
    }
    let geo = to_mm(d, &ro);
    let inside = |p: &[Pt]| p.iter().any(|q| q.x >= zone.min.x && q.x <= zone.max.x && q.y >= zone.min.y && q.y <= zone.max.y);

    let f = Frame::covering(zone, so.res, so.merge * 2.0 + so.res * 4.0);
    let mut m = vec![false; f.len()];
    let mut columns: Vec<Vec<Pt>> = vec![];
    for (role, p, closed) in &geo.paths {
        if !inside(p) {
            continue;
        }
        match role {
            LayerRole::Wall | LayerRole::Outline | LayerRole::Window => {
                let n = p.len();
                let segs = if *closed { n } else { n.saturating_sub(1) };
                for i in 0..segs {
                    stroke_segment(&f, &mut m, p[i], p[(i + 1) % n], so.res * 0.6, true);
                }
            }
            LayerRole::Column if *closed || p.first().zip(p.last()).map_or(false, |(a, b)| a.dist(*b) < 20.0) => {
                let a = polygon_area(p).abs();
                if a > 0.015e6 && a < 4.0e6 {
                    columns.push(simplify_ring(p, 15.0));
                }
            }
            _ => {}
        }
    }

    // Columnas: quedarse con la exterior cuando hay varias anidadas (contorno + achurado).
    columns.sort_by(|a, b| polygon_area(b).abs().partial_cmp(&polygon_area(a).abs()).unwrap());
    let mut cols: Vec<Vec<Pt>> = vec![];
    for c in columns {
        let cc = centroid(&c);
        if cols.iter().any(|k| point_in_polygon(cc, k) || centroid(k).dist(cc) < 150.0) {
            continue;
        }
        cols.push(c);
    }
    // Las columnas no deben verse como muro.
    for c in &cols {
        crate::raster::fill_polygon(&f, &mut m, &c.iter().map(|&q| q).collect::<Vec<_>>(), false);
    }

    // Escaleras y ascensores: un bloque por grupo.
    let mut cm = vec![false; f.len()];
    for p in &d.paths {
        let n = d.layers[p.layer as usize].name.to_uppercase();
        if !["STAIR", "ESCALER", "ELEV", "ASCENS"].iter().any(|k| n.contains(k)) {
            continue;
        }
        let q: Vec<Pt> = p.pts.iter().map(|&q| q * opts.mm_per_unit).collect();
        if !inside(&q) {
            continue;
        }
        let segs = if p.closed { q.len() } else { q.len() - 1 };
        for i in 0..segs {
            stroke_segment(&f, &mut cm, q[i], q[(i + 1) % q.len()], so.res * 0.6, true);
        }
    }
    let cm = erode(&f, &dilate(&f, &cm, 350.0), 350.0);
    let rings = crate::raster::contours(&f, &cm);
    let mut cores: Vec<Vec<Pt>> = vec![];
    for r in &rings {
        if polygon_area(r).abs() < 4.0e6 {
            continue;
        }
        if rings.iter().any(|o| !std::ptr::eq(o, r) && polygon_area(o).abs() > polygon_area(r).abs() && point_in_polygon(r[0], o)) {
            continue;
        }
        cores.push(simplify_ring(r, 120.0));
    }
    for c in cores.iter_mut() {
        straighten_ring(c);
    }

    // Fundir líneas dobles: cierre morfológico.
    let grown = dilate(&f, &m, so.merge);
    let closed = erode(&f, &grown, so.merge * 0.55);
    let sk = thin(&f, closed);
    let mut walls = trace(&f, &sk);
    for w in walls.iter_mut() {
        *w = simplify(w, so.tol);
    }
    prune(&mut walls, so.spur);
    walls = merge_chains(walls);
    for w in walls.iter_mut() {
        *w = simplify(w, so.tol);
        straighten(w);
    }

    let doors: Vec<Door> = plan.doors.iter().filter(|d| d.kind != DoorKind::Ignore).cloned().collect();
    // Extremos de muro sueltos cerca de una jamba: llevarlos a la jamba.
    for w in walls.iter_mut() {
        for end in [0usize, 1] {
            let i = if end == 0 { 0 } else { w.len() - 1 };
            let p = w[i];
            let mut best: Option<(f64, Pt)> = None;
            for d in &doors {
                for q in [d.a, d.b] {
                    let dd = p.dist(q);
                    if dd < 420.0 && best.map_or(true, |b| dd < b.0) {
                        best = Some((dd, q));
                    }
                }
            }
            if let Some((_, q)) = best {
                w[i] = q;
            }
        }
    }
    walls.retain(|w| path_len(w) > 1.0);
    close_gaps(&f, &mut walls, &cols, &cores, &doors, &plan.region);

    let mut labels = vec![];
    for r in &plan.rooms {
        let a = polygon_area(&r.poly).abs() / 1e6;
        if (6.0..60.0).contains(&a) {
            let l = if r.label.is_empty() { "Oficina".to_string() } else { r.label.clone() };
            labels.push((centroid(&r.poly), l));
        }
    }
    for o in &plan.obstacles {
        if o.kind == ObstacleKind::FixedOffice {
            labels.push((centroid(&o.poly), if o.label.is_empty() { "Oficina".into() } else { o.label.clone() }));
        }
    }
    SimplePlan { walls, columns: cols, cores, doors, region: plan.region.clone(), labels }
}

/// Donde el borde del área amoblable detectada no tiene muro cerca (un vano grande que el
/// análisis cerró, un muro en una capa omitida) se agrega ese tramo del borde como muro,
/// para que el plano simple delimite exactamente la misma área.
fn close_gaps(f: &Frame, walls: &mut Vec<Vec<Pt>>, cols: &[Vec<Pt>], cores: &[Vec<Pt>], doors: &[Door], region: &[Vec<Pt>]) {
    let mut m = vec![false; f.len()];
    for w in walls.iter() {
        for s in w.windows(2) {
            stroke_segment(f, &mut m, s[0], s[1], f.res, true);
        }
    }
    for r in cols.iter().chain(cores.iter()) {
        for i in 0..r.len() {
            stroke_segment(f, &mut m, r[i], r[(i + 1) % r.len()], f.res, true);
        }
    }
    for d in doors {
        stroke_segment(f, &mut m, d.a, d.b, f.res, true);
    }
    let dist = crate::raster::edt(f, &m);
    let far = |p: Pt| f.cell_idx(p).map_or(false, |i| dist[i] > 320.0);
    for ring in region {
        let n = ring.len();
        if n < 3 {
            continue;
        }
        // densificar
        let mut pts: Vec<Pt> = vec![];
        for i in 0..n {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            let k = ((a.dist(b) / 100.0).ceil() as usize).max(1);
            for j in 0..k {
                pts.push(a.lerp(b, j as f64 / k as f64));
            }
        }
        let flags: Vec<bool> = pts.iter().map(|&p| far(p)).collect();
        if flags.iter().all(|&x| x) || !flags.iter().any(|&x| x) {
            continue;
        }
        // arrancar en un punto cubierto para no partir una corrida
        let start = flags.iter().position(|&x| !x).unwrap();
        let len = pts.len();
        let mut run: Vec<Pt> = vec![];
        for t in 0..=len {
            let i = (start + t) % len;
            if flags[i] && t < len {
                if run.is_empty() {
                    run.push(pts[(i + len - 1) % len]);
                }
                run.push(pts[i]);
            } else if !run.is_empty() {
                run.push(pts[i]);
                let r = simplify(&run, 60.0);
                if path_len(&r) > 250.0 {
                    walls.push(r);
                }
                run.clear();
            }
        }
    }
}

fn path_len(p: &[Pt]) -> f64 {
    p.windows(2).map(|w| w[0].dist(w[1])).sum()
}

/// Lleva a horizontal/vertical los tramos que casi lo son (errores de la grilla).
fn straighten(p: &mut [Pt]) {
    for i in 0..p.len().saturating_sub(1) {
        let (a, b) = (p[i], p[i + 1]);
        let d = b - a;
        if d.len() < 1.0 {
            continue;
        }
        let ang = d.y.atan2(d.x).to_degrees().rem_euclid(180.0);
        if ang < 4.0 || ang > 176.0 {
            let y = (a.y + b.y) * 0.5;
            p[i].y = y;
            p[i + 1].y = y;
        } else if (ang - 90.0).abs() < 4.0 {
            let x = (a.x + b.x) * 0.5;
            p[i].x = x;
            p[i + 1].x = x;
        }
    }
}

fn straighten_ring(r: &mut Vec<Pt>) {
    if r.len() < 3 {
        return;
    }
    let first = r[0];
    r.push(first);
    straighten(r);
    let last = r.pop().unwrap();
    r[0] = last;
}

/// Adelgazamiento de Zhang–Suen.
fn thin(f: &Frame, mut m: Vec<bool>) -> Vec<bool> {
    let (w, h) = (f.w as i64, f.h as i64);
    let at = |m: &[bool], x: i64, y: i64| x >= 0 && y >= 0 && x < w && y < h && m[(y * w + x) as usize];
    loop {
        let mut changed = false;
        for pass in 0..2 {
            let mut del = vec![];
            for y in 0..h {
                for x in 0..w {
                    if !m[(y * w + x) as usize] {
                        continue;
                    }
                    // P2..P9 en sentido horario desde arriba
                    let n = [
                        at(&m, x, y - 1),
                        at(&m, x + 1, y - 1),
                        at(&m, x + 1, y),
                        at(&m, x + 1, y + 1),
                        at(&m, x, y + 1),
                        at(&m, x - 1, y + 1),
                        at(&m, x - 1, y),
                        at(&m, x - 1, y - 1),
                    ];
                    let b = n.iter().filter(|&&v| v).count();
                    if !(2..=6).contains(&b) {
                        continue;
                    }
                    let a = (0..8).filter(|&i| !n[i] && n[(i + 1) % 8]).count();
                    if a != 1 {
                        continue;
                    }
                    let (p2, p4, p6, p8) = (n[0], n[2], n[4], n[6]);
                    let ok = if pass == 0 { !(p2 && p4 && p6) && !(p4 && p6 && p8) } else { !(p2 && p4 && p8) && !(p2 && p6 && p8) };
                    if ok {
                        del.push((y * w + x) as usize);
                    }
                }
            }
            if !del.is_empty() {
                changed = true;
                for i in del {
                    m[i] = false;
                }
            }
        }
        if !changed {
            return m;
        }
    }
}

/// Recorre el esqueleto: polilíneas entre nodos (extremos y cruces) y lazos cerrados.
fn trace(f: &Frame, sk: &[bool]) -> Vec<Vec<Pt>> {
    let (w, h) = (f.w as i64, f.h as i64);
    let on = |x: i64, y: i64| x >= 0 && y >= 0 && x < w && y < h && sk[(y * w + x) as usize];
    let nb = |i: usize| -> Vec<usize> {
        let (x, y) = ((i as i64) % w, (i as i64) / w);
        let mut v = vec![];
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            if on(x + dx, y + dy) {
                v.push(((y + dy) * w + x + dx) as usize);
            }
        }
        v
    };
    let deg: HashMap<usize, usize> = (0..sk.len()).filter(|&i| sk[i]).map(|i| (i, nb(i).len())).collect();
    let is_node = |i: usize| deg.get(&i).map_or(false, |&d| d != 2);
    // Nodos vecinos forman un solo cruce (los esqueletos dejan grumos de 2×2).
    let mut cluster: HashMap<usize, usize> = HashMap::new();
    let mut centers: Vec<Pt> = vec![];
    let mut nodes: Vec<usize> = deg.keys().copied().filter(|&i| is_node(i)).collect();
    nodes.sort();
    for &s in &nodes {
        if cluster.contains_key(&s) {
            continue;
        }
        let id = centers.len();
        let mut stack = vec![s];
        let mut acc = Pt { x: 0.0, y: 0.0 };
        let mut n = 0.0;
        cluster.insert(s, id);
        while let Some(c) = stack.pop() {
            acc = acc + f.center_i(c);
            n += 1.0;
            for q in nb(c) {
                if is_node(q) && !cluster.contains_key(&q) {
                    cluster.insert(q, id);
                    stack.push(q);
                }
            }
        }
        centers.push(acc * (1.0 / n));
    }
    let key = |a: usize, b: usize| if a < b { (a, b) } else { (b, a) };
    let mut used: std::collections::HashSet<(usize, usize)> = Default::default();
    let mut out: Vec<Vec<Pt>> = vec![];
    let walk = |start: usize, first: usize, used: &mut std::collections::HashSet<(usize, usize)>| -> Vec<usize> {
        let mut chain = vec![start, first];
        used.insert(key(start, first));
        let (mut prev, mut cur) = (start, first);
        while !is_node(cur) && cur != start {
            let next = nb(cur).into_iter().find(|&n| n != prev && !used.contains(&key(cur, n)));
            match next {
                Some(n) => {
                    used.insert(key(cur, n));
                    chain.push(n);
                    prev = cur;
                    cur = n;
                }
                None => break,
            }
        }
        chain
    };
    let to_pts = |c: Vec<usize>| -> Vec<Pt> {
        let mut v: Vec<Pt> = c.iter().map(|&i| f.center_i(i)).collect();
        if let Some(&k) = cluster.get(&c[0]) {
            v[0] = centers[k];
        }
        if let Some(&k) = cluster.get(c.last().unwrap()) {
            let l = v.len() - 1;
            v[l] = centers[k];
        }
        v
    };
    for &s in &nodes {
        for n in nb(s) {
            if is_node(n) || used.contains(&key(s, n)) {
                continue;
            }
            let c = walk(s, n, &mut used);
            out.push(to_pts(c));
        }
    }
    // lazos sin nodos
    let mut rest: Vec<usize> = deg.keys().copied().filter(|&i| !is_node(i)).collect();
    rest.sort();
    for s in rest {
        for n in nb(s) {
            if !used.contains(&key(s, n)) {
                let c = walk(s, n, &mut used);
                out.push(to_pts(c));
            }
        }
    }
    out.into_iter().filter(|c| c.len() >= 2 && path_len(c) > 1.0).collect()
}

fn pkey(p: Pt) -> (i64, i64) {
    ((p.x / 30.0).round() as i64, (p.y / 30.0).round() as i64)
}

fn degrees(ws: &[Vec<Pt>]) -> HashMap<(i64, i64), usize> {
    let mut d = HashMap::new();
    for w in ws {
        *d.entry(pkey(w[0])).or_insert(0) += 1;
        *d.entry(pkey(*w.last().unwrap())).or_insert(0) += 1;
    }
    d
}

/// Borra ramitas cortas con un extremo suelto (barbas del esqueleto).
fn prune(ws: &mut Vec<Vec<Pt>>, spur: f64) {
    for _ in 0..3 {
        let d = degrees(ws);
        let before = ws.len();
        ws.retain(|w| {
            let (a, b) = (d[&pkey(w[0])], d[&pkey(*w.last().unwrap())]);
            let loose = (a == 1) as u8 + (b == 1) as u8;
            let l = path_len(w);
            !(loose == 2 && l < spur * 1.5 || loose == 1 && l < spur && (a > 2 || b > 2))
        });
        if ws.len() == before {
            break;
        }
    }
}

/// Une tramos que se tocan en un punto por el que pasan exactamente dos.
fn merge_chains(mut ws: Vec<Vec<Pt>>) -> Vec<Vec<Pt>> {
    loop {
        let d = degrees(&ws);
        let mut ends: HashMap<(i64, i64), Vec<(usize, bool)>> = HashMap::new();
        for (i, w) in ws.iter().enumerate() {
            ends.entry(pkey(w[0])).or_default().push((i, false));
            ends.entry(pkey(*w.last().unwrap())).or_default().push((i, true));
        }
        let mut done = false;
        let mut keys: Vec<_> = ends.keys().copied().collect();
        keys.sort();
        for k in keys {
            let e = &ends[&k];
            if d[&k] != 2 || e.len() != 2 || e[0].0 == e[1].0 {
                continue;
            }
            let (i, ie) = e[0];
            let (j, je) = e[1];
            let mut a = ws[i].clone();
            let mut b = ws[j].clone();
            if !ie {
                a.reverse();
            }
            if je {
                b.reverse();
            }
            a.extend(b.into_iter().skip(1));
            let (lo, hi) = if i < j { (i, j) } else { (j, i) };
            ws.remove(hi);
            ws.remove(lo);
            ws.push(a);
            done = true;
            break;
        }
        if !done {
            return ws;
        }
    }
}

// ---------------------------------------------------------------------------
// Salidas

const LAYERS: [(&str, u8); 6] = [("MUROS", 7), ("COLUMNAS", 1), ("NUCLEO", 8), ("PUERTAS", 4), ("A-PUERTA-PPAL", 3), ("TEXTO", 8)];

struct DoorGeo {
    layer: &'static str,
    leaf: (Pt, Pt),
    hinge: Pt,
    r: f64,
    a0: f64,
    a1: f64,
}

fn door_geo(d: &Door, region: &[Vec<Pt>]) -> DoorGeo {
    let w = d.width();
    let u = (d.b - d.a).norm();
    let n = u.perp();
    let m = d.mid();
    let inward = if point_in_region(m + n * 300.0, region) { n } else { -n };
    let a0 = u.y.atan2(u.x).to_degrees();
    let a1 = inward.y.atan2(inward.x).to_degrees();
    let mut da = a1 - a0;
    while da > 180.0 {
        da -= 360.0;
    }
    while da < -180.0 {
        da += 360.0;
    }
    let (s, e) = if da >= 0.0 { (a0, a0 + da) } else { (a0 + da, a0) };
    DoorGeo { layer: if d.kind == DoorKind::Main { "A-PUERTA-PPAL" } else { "PUERTAS" }, leaf: (d.a, d.a + inward * w), hinge: d.a, r: w, a0: s, a1: e }
}

pub fn simple_dxf(sp: &SimplePlan, out_mm: f64) -> String {
    let k = out_mm.max(1e-9);
    let mut s = String::new();
    let mut pair = |c: i32, v: &dyn std::fmt::Display| {
        let _ = write!(s, "{c}\n{v}\n");
    };
    let f = |v: f64| format!("{:.3}", v / k);
    pair(0, &"SECTION");
    pair(2, &"HEADER");
    pair(9, &"$ACADVER");
    pair(1, &"AC1009");
    pair(9, &"$INSUNITS");
    pair(70, &insunits_code(k));
    pair(0, &"ENDSEC");
    pair(0, &"SECTION");
    pair(2, &"TABLES");
    pair(0, &"TABLE");
    pair(2, &"LAYER");
    pair(70, &LAYERS.len());
    for (n, c) in LAYERS {
        pair(0, &"LAYER");
        pair(2, &n);
        pair(70, &0);
        pair(62, &c);
        pair(6, &"CONTINUOUS");
    }
    pair(0, &"ENDTAB");
    pair(0, &"ENDSEC");
    pair(0, &"SECTION");
    pair(2, &"ENTITIES");
    let poly = |pair: &mut dyn FnMut(i32, &dyn std::fmt::Display), layer: &str, pts: &[Pt], closed: bool| {
        if pts.len() == 2 && !closed {
            pair(0, &"LINE");
            pair(8, &layer);
            pair(10, &f(pts[0].x));
            pair(20, &f(pts[0].y));
            pair(30, &0);
            pair(11, &f(pts[1].x));
            pair(21, &f(pts[1].y));
            pair(31, &0);
            return;
        }
        pair(0, &"POLYLINE");
        pair(8, &layer);
        pair(66, &1);
        pair(10, &0);
        pair(20, &0);
        pair(30, &0);
        pair(70, &(closed as u8));
        for p in pts {
            pair(0, &"VERTEX");
            pair(8, &layer);
            pair(10, &f(p.x));
            pair(20, &f(p.y));
            pair(30, &0);
        }
        pair(0, &"SEQEND");
        pair(8, &layer);
    };
    for w in &sp.walls {
        poly(&mut pair, "MUROS", w, false);
    }
    for c in &sp.columns {
        poly(&mut pair, "COLUMNAS", c, true);
    }
    for c in &sp.cores {
        poly(&mut pair, "NUCLEO", c, true);
    }
    for d in &sp.doors {
        let g = door_geo(d, &sp.region);
        poly(&mut pair, g.layer, &[g.leaf.0, g.leaf.1], false);
        pair(0, &"ARC");
        pair(8, &g.layer);
        pair(10, &f(g.hinge.x));
        pair(20, &f(g.hinge.y));
        pair(30, &0);
        pair(40, &f(g.r));
        pair(50, &format!("{:.4}", g.a0.rem_euclid(360.0)));
        pair(51, &format!("{:.4}", g.a1.rem_euclid(360.0)));
    }
    for (p, t) in &sp.labels {
        pair(0, &"TEXT");
        pair(8, &"TEXTO");
        pair(10, &f(p.x));
        pair(20, &f(p.y));
        pair(30, &0);
        pair(40, &f(220.0));
        pair(1, &dxf_text(t));
        pair(72, &1);
        pair(73, &2);
        pair(11, &f(p.x));
        pair(21, &f(p.y));
        pair(31, &0);
    }
    pair(0, &"ENDSEC");
    pair(0, &"EOF");
    s
}

/// El mismo plano como `Drawing` en mm (para el PDF con capas).
pub fn simple_drawing(sp: &SimplePlan, name: &str) -> Drawing {
    let mut d = Drawing::new(name, SourceKind::Dxf);
    let ids: Vec<u32> = [("MUROS", "#25302b"), ("COLUMNAS", "#c86b5c"), ("PUERTAS", "#2a8fb0"), ("A-PUERTA-PPAL", "#1a9c3b"), ("TEXTO", "#8a958e"), ("NUCLEO", "#7d8781")]
        .iter()
        .map(|(n, c)| d.layer_id(n, c))
        .collect();
    for w in &sp.walls {
        d.add_path(ids[0], w.clone(), false);
    }
    for c in &sp.columns {
        d.add_path(ids[1], c.clone(), true);
    }
    for c in &sp.cores {
        d.add_path(ids[5], c.clone(), true);
    }
    for dr in &sp.doors {
        let g = door_geo(dr, &sp.region);
        let l = if g.layer == "PUERTAS" { ids[2] } else { ids[3] };
        d.add_path(l, vec![g.leaf.0, g.leaf.1], false);
        let arc = crate::import::flatten_arc(g.hinge, g.r, g.a0.to_radians(), g.a1.to_radians());
        d.add_path(l, arc.clone(), false);
        d.add_curve(l, arc);
    }
    for (p, t) in &sp.labels {
        d.texts.push(crate::import::DText { layer: ids[4], pos: *p, height: 220.0, rot: 0.0, text: t.clone() });
    }
    d.declared_unit_mm = Some(1.0);
    d.finalize();
    d
}
