// Separaciones (despejes) de cada elemento, para verlas en dorado y editarlas en el plano.
import type { CircPath, Config, DeskType, Door, Module, Obstacle, Plan, Pt } from './types';

export type ClearKey = 'chair' | 'back' | 'left' | 'right' | 'depth' | 'width' | number;
export interface ClearLabel {
  pos: Pt; value: number; key: ClearKey; title: string; prefix?: string;
  /** Hacia dónde crece la medida al arrastrar (unitario) y cuánto cambia por mm arrastrado. */
  dir: Pt; mult?: number;
}
/** Borde exterior de una zona dorada: se puede arrastrar para cambiar la medida. */
export interface ClearEdge { a: Pt; b: Pt; key: ClearKey; dir: Pt; mult?: number; value: number }
export interface ClearInfo { zones: Pt[][]; labels: ClearLabel[]; edges?: ClearEdge[]; /** Para el recinto: aristas de cada grupo (una etiqueta por grupo). */ groups?: number[][] }

const add = (a: Pt, b: Pt): Pt => ({ x: a.x + b.x, y: a.y + b.y });
const sub = (a: Pt, b: Pt): Pt => ({ x: a.x - b.x, y: a.y - b.y });
const mul = (a: Pt, k: number): Pt => ({ x: a.x * k, y: a.y * k });
const norm = (a: Pt): Pt => { const l = Math.hypot(a.x, a.y) || 1; return { x: a.x / l, y: a.y / l }; };

function pip(p: Pt, poly: Pt[]) {
  let c = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const a = poly[i], b = poly[j];
    if ((a.y > p.y) !== (b.y > p.y) && p.x < ((b.x - a.x) * (p.y - a.y)) / (b.y - a.y) + a.x) c = !c;
  }
  return c;
}
const inRegion = (p: Pt, rings: Pt[][]) => rings.reduce((c, r) => (pip(p, r) ? !c : c), false);
const area = (poly: Pt[]) => poly.reduce((s, p, i) => { const q = poly[(i + 1) % poly.length]; return s + p.x * q.y - q.x * p.y; }, 0) / 2;

/** Valor de una sub-regla si la regla y la sub-regla están activas. */
export function ruleVal(c: Config, rid: string, iid: string): number | null {
  const r = c.rules.find((x) => x.id === rid);
  if (!r || !r.enabled) return null;
  const it = r.items.find((i) => i.id === iid);
  if (!it || !it.enabled) return null;
  return it.value;
}
const ruleOn = (c: Config, rid: string, iid?: string) => {
  const r = c.rules.find((x) => x.id === rid);
  if (!r || !r.enabled) return false;
  if (!iid) return true;
  return !!r.items.find((i) => i.id === iid)?.enabled;
};

/** Separación general según el tipo de obstáculo. */
export function obstacleDefault(c: Config, o: Obstacle): number {
  if (o.kind === 'column') return ruleVal(c, 'O1', 'dist') ?? 0;
  if (o.kind === 'fixedOffice') return ruleVal(c, 'O3', 'dist') ?? 0;
  if (o.kind === 'restricted') return ruleVal(c, 'O2', 'dist') ?? 0;
  return 0;
}

/** Separaciones por lado de un obstáculo (las que valen ahora, propias o generales). */
export function obstacleClears(c: Config, o: Obstacle): number[] {
  const n = o.poly.length;
  const def = obstacleDefault(c, o);
  if (o.clear && o.clear.length === n) return o.clear;
  const v = o.clear && o.clear.length ? o.clear[0] : def;
  return Array(n).fill(v);
}

export function obstacleInfo(c: Config, o: Obstacle): ClearInfo {
  const n = o.poly.length;
  const d = obstacleClears(c, o);
  const ccw = area(o.poly) > 0;
  const zones: Pt[][] = [];
  const labels: ClearLabel[] = [];
  const edges: ClearEdge[] = [];
  const perSide = n === 4;
  for (let i = 0; i < n; i++) {
    const a = o.poly[i], b = o.poly[(i + 1) % n];
    const u = norm(sub(b, a));
    const out = ccw ? { x: u.y, y: -u.x } : { x: -u.y, y: u.x };
    const di = Math.max(0, d[i]);
    const dp = Math.max(0, d[(i + n - 1) % n]), dn = Math.max(0, d[(i + 1) % n]);
    if (di > 0) {
      const a2 = sub(a, mul(u, dp)), b2 = add(b, mul(u, dn));
      zones.push([a2, b2, add(b2, mul(out, di)), add(a2, mul(out, di))]);
    }
    if (perSide || di > 0) edges.push({ a: add(a, mul(out, di)), b: add(b, mul(out, di)), key: perSide ? i : 0, dir: out, value: di });
    if (perSide || i === 0) {
      const mid = mul(add(a, b), 0.5);
      labels.push({ pos: add(mid, mul(out, Math.max(di, 180) * 0.5 + (di > 0 ? 0 : 160))), value: di, dir: out, key: perSide ? i : 0, title: perSide ? 'Clearance on this side' : 'Clearance around' });
    }
  }
  return { zones, labels, edges };
}

/** Despeje frente a una puerta: fondo y ancho (propios o de las reglas C4 / C3). */
export function doorClear(c: Config, plan: Plan, d: Door) {
  const w = Math.max(300, Math.hypot(d.b.x - d.a.x, d.b.y - d.a.y));
  const u = norm(sub(d.b, d.a));
  const nrm = { x: -u.y, y: u.x };
  const mid = mul(add(d.a, d.b), 0.5);
  // lado libre: dentro del recinto y fuera de oficinas y columnas (igual que el motor); “flip” lo invierte
  const free = (q: Pt) => inRegion(q, plan.region) && !plan.obstacles.some((o) => (o.kind === 'column' || o.kind === 'fixedOffice') && pip(q, o.poly));
  const score = (s: number) => [1, 2, 3, 4, 5, 6].filter((k) => free(add(mid, mul(nrm, s * 150 * k)))).length;
  let inward = score(1) >= score(-1) ? nrm : mul(nrm, -1);
  if (d.flip) inward = mul(inward, -1);
  const applies = d.kind === 'main' || (d.kind === 'exit' && ruleOn(c, 'C4', 'salidas')) || (d.kind === 'office' && ruleOn(c, 'C4', 'oficinas'));
  let depth = 0;
  if (ruleOn(c, 'C4') && applies) depth = (ruleOn(c, 'C4', 'barrido') ? w : 0) + (ruleVal(c, 'C4', 'extra') ?? 0);
  let width = w + 400;
  if (d.clearDepth != null) depth = d.clearDepth;
  if (d.clearWidth != null) width = d.clearWidth;
  return { depth, width, mid, u, inward, w };
}

export function doorInfo(c: Config, plan: Plan, d: Door): ClearInfo {
  const { depth, width, mid, u, inward } = doorClear(c, plan, d);
  const h = width / 2;
  const zone = [sub(mid, mul(u, h)), add(mid, mul(u, h)), add(add(mid, mul(u, h)), mul(inward, depth)), add(sub(mid, mul(u, h)), mul(inward, depth))];
  return {
    zones: depth > 0 ? [zone] : [],
    edges: [{ a: zone[3], b: zone[2], key: 'depth', dir: inward, value: depth }, { a: zone[1], b: zone[2], key: 'width', dir: u, mult: 2, value: width }],
    labels: [
      { pos: add(mid, mul(inward, Math.max(depth, 400) * 0.5)), value: depth, dir: inward, key: 'depth', title: 'Free depth in front of the door', prefix: 'depth' },
      { pos: add(add(mid, mul(inward, Math.max(depth, 400) + 220)), mul(u, 0)), value: width, dir: u, mult: 2, key: 'width', title: 'Free width in front of the door', prefix: 'width' },
    ],
  };
}

/** Separación del lado de la silla de un tipo (la propia o la regla E1). */
export function chairClear(c: Config, t: DeskType): number {
  const e1 = ruleVal(c, 'E1', 'muro') ?? 0;
  return Math.max(t.clearChair ?? e1, 0);
}

export function moduleInfo(c: Config, m: Module, t: DeskType): ClearInfo {
  const a = (m.angle * Math.PI) / 180;
  const u = { x: Math.cos(a), y: Math.sin(a) }, v = { x: -u.y, y: u.x };
  const hl = m.length / 2, hd = m.depth / 2;
  const L = (p: number, q: number) => add(m.center, add(mul(u, p), mul(v, q)));
  const rect = (a0: number, a1: number, b0: number, b1: number) => [L(a0, b0), L(a1, b0), L(a1, b1), L(a0, b1)];
  const two = isTwo(t);
  const notch = t.shape === 'l' ? Math.max(0, m.depth - t.depth) : 0;
  const cc = Math.max(0, chairClear(c, t) - notch);
  const sgn = m.seatsUp ? 1 : -1;
  const zones: Pt[][] = [];
  const labels: ClearLabel[] = [];
  const edges: ClearEdge[] = [];
  const seatSides = two ? [1, -1] : [sgn];
  for (const s of seatSides) {
    if (cc > 0) zones.push(s > 0 ? rect(-hl, hl, hd, hd + cc) : rect(-hl, hl, -hd - cc, -hd));
    edges.push({ a: L(-hl, s * (hd + cc)), b: L(hl, s * (hd + cc)), key: 'chair', dir: mul(v, s), value: chairClear(c, t) });
  }
  labels.push({ pos: L(0, sgn * (hd + Math.max(cc, 300) / 2)), value: chairClear(c, t), dir: mul(v, sgn), key: 'chair', title: 'Free space on the chair side', prefix: 'chair' });
  if (!two) {
    const bk = Math.max(0, t.clearBack ?? 0);
    if (bk > 0) zones.push(sgn > 0 ? rect(-hl, hl, -hd - bk, -hd) : rect(-hl, hl, hd, hd + bk));
    edges.push({ a: L(-hl, -sgn * (hd + bk)), b: L(hl, -sgn * (hd + bk)), key: 'back', dir: mul(v, -sgn), value: bk });
    labels.push({ pos: L(0, -sgn * (hd + Math.max(bk, 300) / 2)), value: bk, dir: mul(v, -sgn), key: 'back', title: 'Clearance behind the desk', prefix: 'back' });
  }
  const cl = Math.max(0, t.clearLeft ?? 0), cr = Math.max(0, t.clearRight ?? 0);
  if (cl > 0) zones.push(rect(-hl - cl, -hl, -hd, hd));
  if (cr > 0) zones.push(rect(hl, hl + cr, -hd, hd));
  edges.push({ a: L(-hl - cl, -hd), b: L(-hl - cl, hd), key: 'left', dir: mul(u, -1), value: cl });
  edges.push({ a: L(hl + cr, -hd), b: L(hl + cr, hd), key: 'right', dir: u, value: cr });
  labels.push({ pos: L(-hl - Math.max(cl, 300) / 2, 0), value: cl, dir: mul(u, -1), key: 'left', title: 'Clearance on the left', prefix: 'left' });
  labels.push({ pos: L(hl + Math.max(cr, 300) / 2, 0), value: cr, dir: u, key: 'right', title: 'Clearance on the right', prefix: 'right' });
  return { zones, labels, edges };
}

/** ¿Sillas en los dos lados largos? */
export const isTwo = (t: DeskType) => t.shape !== 'l' && t.sides >= 2;

/** Largo y fondo del módulo (el rectángulo que usa el optimizador). */
export function deskDims(t: DeskType) {
  const per = t.flexible ? Math.max(t.minPerSide, Math.min(t.maxPerSide, t.perSide)) : t.perSide;
  const length = t.shape === 'straight' ? t.seatWidth * per : t.seatWidth;
  const depth = t.shape === 'straight' ? t.depth * t.sides : t.shape === 'l' ? Math.max(t.returnLength, t.depth) : t.depth;
  return { per, length, depth };
}

/** Cuerpo de un escritorio colocado a mano (vista previa) y, para los propios, sus sillas. */
export function deskBody(t: DeskType, center: Pt, angleDeg: number, seatsUp = true) {
  const { per, length, depth } = deskDims(t);
  const a = (angleDeg * Math.PI) / 180;
  const u = { x: Math.cos(a), y: Math.sin(a) }, v = { x: -u.y, y: u.x };
  const L = (p: number, q: number) => add(center, add(mul(u, p), mul(v, q)));
  if (t.shape === 'l') {
    // igual que el motor: barra del lado opuesto a la silla y retorno a la izquierda; la silla en el rincón
    const hl = length / 2, hd = depth / 2, s = seatsUp ? -1 : 1;
    const d = Math.min(t.depth, depth), rd = Math.min(Math.max(t.returnDepth, 300), length * 0.6);
    const body = [L(-hl, -hd * s), L(-hl + rd, -hd * s), L(-hl + rd, (hd - d) * s), L(hl, (hd - d) * s), L(hl, hd * s), L(-hl, hd * s)];
    return { length, depth, per, u, v, body, chairs: [L(rd / 2, (hd - d - 250) * s)] };
  }
  if (t.shape === 'custom' && t.body && t.body.length >= 3) {
    const my = seatsUp || isTwo(t) ? 1 : -1;
    return {
      length, depth, per, u, v,
      body: t.body.map((q) => L(q.x, q.y * my)),
      chairs: (t.seats ?? []).map((s) => L(s.x, s.y * my)),
    };
  }
  return { length, depth, per, body: [L(-length / 2, -depth / 2), L(length / 2, -depth / 2), L(length / 2, depth / 2), L(-length / 2, depth / 2)], u, v, chairs: undefined as Pt[] | undefined };
}

/** Separación hacia adentro de cada arista del contorno del recinto. Las aristas cortas seguidas
 *  (curvas dibujadas con muchos tramos) se agrupan y comparten una sola medida. */
/** Separación general a las paredes del recinto (E8, si no se permite tocar el muro). */
export function wallDefault(c: Config): number {
  if (!ruleOn(c, 'E8') || ruleOn(c, 'E8', 'tocar')) return 0;
  return ruleVal(c, 'E8', 'separacion') ?? 300;
}

export function regionInfo(plan: Plan, ri: number, def = 0): ClearInfo {
  const ring = plan.region[ri];
  if (!ring || ring.length < 3) return { zones: [], labels: [] };
  const n = ring.length;
  const cl = plan.regionClear?.[ri] ?? [];
  const val = (i: number) => { const x = cl[i]; return Math.max(0, x === undefined || x === null || x < 0 ? def : x); };
  const len = (i: number) => Math.hypot(ring[(i + 1) % n].x - ring[i].x, ring[(i + 1) % n].y - ring[i].y);
  const inward = (i: number) => {
    const a = ring[i], b = ring[(i + 1) % n];
    const u = norm(sub(b, a));
    const nn = { x: -u.y, y: u.x };
    return inRegion(add(mul(add(a, b), 0.5), mul(nn, 60)), plan.region) ? nn : mul(nn, -1);
  };
  // grupos: cada arista larga sola; las cortas consecutivas juntas
  const SHORT = 600;
  const groups: number[][] = [];
  let cur: number[] = [];
  for (let i = 0; i < n; i++) {
    if (len(i) >= SHORT) {
      if (cur.length) { groups.push(cur); cur = []; }
      groups.push([i]);
    } else cur.push(i);
  }
  if (cur.length) {
    // unir con el primer grupo si también era de aristas cortas (el anillo da la vuelta)
    if (groups.length && len(groups[0][0]) < SHORT) groups[0] = [...cur, ...groups[0]];
    else groups.push(cur);
  }
  const zones: Pt[][] = [];
  const labels: ClearLabel[] = [];
  const edges: ClearEdge[] = [];
  groups.forEach((g, gi) => {
    for (const i of g) {
      const d = val(i);
      const a = ring[i], b = ring[(i + 1) % n], inn = inward(i);
      edges.push({ a: add(a, mul(inn, d)), b: add(b, mul(inn, d)), key: gi, dir: inn, value: val(g[0]) });
      if (d <= 0) continue;
      zones.push([a, b, add(b, mul(inn, d)), add(a, mul(inn, d))]);
    }
    const mid = g[Math.floor(g.length / 2)];
    const a = ring[mid], b = ring[(mid + 1) % n];
    const p = g.length > 1 && g.length % 2 === 0 ? a : mul(add(a, b), 0.5);
    const d = val(g[0]);
    labels.push({ pos: add(p, mul(inward(mid), Math.max(d, 300) * 0.5 + (d > 0 ? 0 : 120))), value: d, dir: inward(mid), key: gi, title: g.length > 1 ? `Clearance along this curved stretch (${g.length} edges)` : 'Clearance inward from this edge' });
  });
  return { zones, labels, edges, groups };
}

/** Width of a circulation path: its own, or the general one for its kind (rule P1). */
export function circWidth(c: Config, p: CircPath): number {
  if (p.width) return p.width;
  return ruleVal(c, 'P1', p.kind === 'primary' ? 'primary' : 'secondary') ?? (p.kind === 'primary' ? 1200 : 900);
}

/** Clear bands of the kept circulation paths (one rectangle per run, with square ends). */
export function pathZones(c: Config, plan: Plan): Pt[][] {
  if (!ruleOn(c, 'P1')) return [];
  const out: Pt[][] = [];
  for (const p of plan.paths ?? []) {
    if (!p.keep || p.pts.length < 2) continue;
    const h = circWidth(c, p) / 2;
    for (let i = 0; i + 1 < p.pts.length; i++) {
      const a = p.pts[i], b = p.pts[i + 1];
      const l = Math.hypot(b.x - a.x, b.y - a.y);
      if (l < 1) continue;
      const u = { x: (b.x - a.x) / l, y: (b.y - a.y) / l }, n = { x: -u.y, y: u.x };
      const a2 = { x: a.x - u.x * h, y: a.y - u.y * h }, b2 = { x: b.x + u.x * h, y: b.y + u.y * h };
      out.push([add(a2, mul(n, h)), add(b2, mul(n, h)), add(b2, mul(n, -h)), add(a2, mul(n, -h))]);
    }
  }
  return out;
}
