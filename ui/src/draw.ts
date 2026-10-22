// Drawing the plan by hand (like AutoCAD): geometry helpers and the check that decides whether the
// areas, doors and obstacles are good enough to continue.
import polygonClipping, { type MultiPolygon, type Polygon } from 'polygon-clipping';
import type { Door, Plan, Pt } from './types';

// ------------------------------------------------------------ basic geometry
export const ringArea = (r: Pt[]) => Math.abs(r.reduce((a, p, i) => { const q = r[(i + 1) % r.length]; return a + p.x * q.y - q.x * p.y; }, 0)) / 2;

export function distSeg(p: Pt, a: Pt, b: Pt) {
  const dx = b.x - a.x, dy = b.y - a.y;
  const l2 = dx * dx + dy * dy || 1;
  const t = Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / l2));
  return Math.hypot(p.x - (a.x + dx * t), p.y - (a.y + dy * t));
}
export function projSeg(p: Pt, a: Pt, b: Pt): Pt {
  const dx = b.x - a.x, dy = b.y - a.y;
  const l2 = dx * dx + dy * dy || 1;
  const t = Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / l2));
  return { x: a.x + dx * t, y: a.y + dy * t };
}
/** Closest point on the infinite line a–b. */
export function projLine(p: Pt, a: Pt, b: Pt): Pt {
  const dx = b.x - a.x, dy = b.y - a.y;
  const l2 = dx * dx + dy * dy || 1;
  const t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / l2;
  return { x: a.x + dx * t, y: a.y + dy * t };
}
export function pip(p: Pt, poly: Pt[]) {
  let c = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const a = poly[i], b = poly[j];
    if ((a.y > p.y) !== (b.y > p.y) && p.x < ((b.x - a.x) * (p.y - a.y)) / (b.y - a.y) + a.x) c = !c;
  }
  return c;
}
export const ringDist = (p: Pt, r: Pt[]) => {
  let d = Infinity;
  for (let i = 0; i < r.length; i++) d = Math.min(d, distSeg(p, r[i], r[(i + 1) % r.length]));
  return d;
};
/** Strictly inside (not on the outline, within `tol`). */
const inside = (p: Pt, r: Pt[], tol = 2) => pip(p, r) && ringDist(p, r) > tol;

function properCross(a: Pt, b: Pt, c: Pt, d: Pt, eps = 1e-6) {
  const o = (p: Pt, q: Pt, r: Pt) => (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
  const d1 = o(a, b, c), d2 = o(a, b, d), d3 = o(c, d, a), d4 = o(c, d, b);
  const s = Math.max(Math.hypot(b.x - a.x, b.y - a.y), Math.hypot(d.x - c.x, d.y - c.y), 1);
  const e = eps * s * s;
  return ((d1 > e && d2 < -e) || (d1 < -e && d2 > e)) && ((d3 > e && d4 < -e) || (d3 < -e && d4 > e));
}

/** Where the outline crosses itself (null = it does not). */
export function selfCross(r: Pt[], closed = true): Pt | null {
  const n = r.length;
  const m = closed ? n : n - 1;
  for (let i = 0; i < m; i++) {
    const a = r[i], b = r[(i + 1) % n];
    for (let j = i + 2; j < m; j++) {
      if (closed && i === 0 && j === n - 1) continue; // neighbours through the closing edge
      const c = r[j], d = r[(j + 1) % n];
      if (properCross(a, b, c, d)) return { x: (a.x + b.x + c.x + d.x) / 4, y: (a.y + b.y + c.y + d.y) / 4 };
    }
  }
  return null;
}

/** Without repeated points and without points in the middle of a straight run. */
export function tidyRing(pts: Pt[]): Pt[] {
  let r = pts.filter((p, i) => { const q = pts[(i + pts.length - 1) % pts.length]; return Math.hypot(p.x - q.x, p.y - q.y) > 1; });
  let changed = true;
  while (changed && r.length > 3) {
    changed = false;
    for (let i = 0; i < r.length; i++) {
      const a = r[(i + r.length - 1) % r.length], b = r[i], c = r[(i + 1) % r.length];
      const cross = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
      const len = Math.hypot(c.x - a.x, c.y - a.y) || 1;
      const dot = (b.x - a.x) * (c.x - b.x) + (b.y - a.y) * (c.y - b.y);
      if (Math.abs(cross) / len < 1 && dot > 0) { r = r.filter((_, k) => k !== i); changed = true; break; }
    }
  }
  return r;
}

// ------------------------------------------------------------ areas (outlines + holes)
export interface AreaGroup { outer: number; holes: number[] }
/** Groups the outlines of the area: each one inside an even number of others is an area; the ones
 *  inside an odd number are its holes. */
export function areaGroups(region: Pt[][]): AreaGroup[] {
  const within = (i: number, j: number) => {
    const a = region[i], b = region[j];
    if (ringArea(a) >= ringArea(b)) return false;
    let yes = 0, no = 0;
    for (const p of a) { if (ringDist(p, b) <= 2) continue; if (pip(p, b)) yes++; else no++; }
    return yes > 0 && yes >= no;
  };
  const parents = region.map((_, i) => region.map((_, j) => j).filter((j) => j !== i && within(i, j)));
  const groups: AreaGroup[] = [];
  region.forEach((_, i) => { if (parents[i].length % 2 === 0) groups.push({ outer: i, holes: [] }); });
  region.forEach((_, i) => {
    if (parents[i].length % 2 === 0) return;
    // the smallest area that contains it
    const host = groups.filter((g) => parents[i].includes(g.outer)).sort((a, b) => ringArea(region[a.outer]) - ringArea(region[b.outer]))[0];
    host?.holes.push(i);
  });
  return groups;
}
export function regionArea(region: Pt[][]) {
  return areaGroups(region).reduce((s, g) => s + ringArea(region[g.outer]) - g.holes.reduce((h, i) => h + ringArea(region[i]), 0), 0);
}

const toPoly = (rings: Pt[][]): Polygon => rings.map((r) => [...r.map((p) => [p.x, p.y] as [number, number]), [r[0].x, r[0].y] as [number, number]]);
const fromRing = (r: [number, number][]): Pt[] => tidyRing(r.slice(0, -1).map(([x, y]) => ({ x, y })));
const mpArea = (mp: MultiPolygon) => mp.reduce((s, poly) => s + poly.reduce((h, r, k) => h + (k ? -1 : 1) * ringArea(r.map(([x, y]) => ({ x, y }))), 0), 0);

/** Adds an outline to the area. If it overlaps areas already there, they become one; if it only
 *  touches them, it stays a separate area. Returns the new outlines and which groups were joined. */
export function addAreaRing(plan: Plan, ring: Pt[]): { region: Pt[][]; regionClear: number[][]; joined: number } {
  const region = plan.region;
  const groups = areaGroups(region);
  const nr = toPoly([ring]);
  const hit: AreaGroup[] = [];
  for (const g of groups) {
    const gp = toPoly([region[g.outer], ...g.holes.map((h) => region[h])]);
    try {
      if (mpArea(polygonClipping.intersection(gp, nr)) > 5e4) hit.push(g); // > 0.05 m²
    } catch { /* degenerate: treat as not overlapping */ }
  }
  const rc = plan.regionClear ?? [];
  const keepIdx = region.map((_, i) => i).filter((i) => !hit.some((g) => g.outer === i || g.holes.includes(i)));
  const out: Pt[][] = keepIdx.map((i) => region[i]);
  const outClear: number[][] = keepIdx.map((i) => rc[i] ?? []);
  if (!hit.length) {
    out.push(ring);
    outClear.push([]);
    return { region: out, regionClear: outClear, joined: 0 };
  }
  let u: MultiPolygon;
  try {
    u = polygonClipping.union(nr, ...hit.map((g) => toPoly([region[g.outer], ...g.holes.map((h) => region[h])])));
  } catch {
    out.push(ring);
    outClear.push([]);
    return { region: out, regionClear: outClear, joined: 0 };
  }
  for (const poly of u) for (const r of poly) {
    const pts = fromRing(r);
    if (pts.length >= 3) { out.push(pts); outClear.push([]); }
  }
  return { region: out, regionClear: outClear, joined: hit.length };
}

/** Removes an area (its outline and its holes). */
export function removeAreaGroup(plan: Plan, outer: number): { region: Pt[][]; regionClear: number[][] } {
  const g = areaGroups(plan.region).find((x) => x.outer === outer);
  const drop = new Set([outer, ...(g?.holes ?? [])]);
  const rc = plan.regionClear ?? [];
  const idx = plan.region.map((_, i) => i).filter((i) => !drop.has(i));
  return { region: idx.map((i) => plan.region[i]), regionClear: idx.map((i) => rc[i] ?? []) };
}

/** Which area (outer outline index) contains the point, or -1. */
export function areaAt(plan: Plan, p: Pt): number {
  const groups = areaGroups(plan.region);
  const g = groups.filter((x) => pip(p, plan.region[x.outer]) && !x.holes.some((h) => pip(p, plan.region[h])))
    .sort((a, b) => ringArea(plan.region[a.outer]) - ringArea(plan.region[b.outer]))[0];
  return g ? g.outer : -1;
}

/** Everything drawn, scaled about the origin (when the scale of the drawing changes). */
export function scalePlan(plan: Plan, f: number): Plan {
  const s = (p: Pt): Pt => ({ x: p.x * f, y: p.y * f });
  return {
    ...plan,
    region: plan.region.map((r) => r.map(s)),
    obstacles: plan.obstacles.map((o) => ({ ...o, poly: o.poly.map(s) })),
    doors: plan.doors.map((d) => ({ ...d, a: s(d.a), b: s(d.b) })),
    windows: plan.windows.map((w) => ({ a: s(w.a), b: s(w.b) })),
    rooms: plan.rooms?.map((r) => ({ ...r, poly: r.poly.map(s) })),
    building: plan.building?.map(s),
    paths: [],
  };
}

export const emptyPlan = (): Plan => ({ region: [], obstacles: [], doors: [], windows: [], rooms: [], building: [], regionClear: [], paths: [] });

// ------------------------------------------------------------ the check
export interface Issue { level: 'error' | 'warn'; text: string; at?: Pt }

const DOOR_TOL = 300; // a door counts as on an area when its middle is this close to the outline
const mid = (d: Door): Pt => ({ x: (d.a.x + d.b.x) / 2, y: (d.a.y + d.b.y) / 2 });
const center = (r: Pt[]): Pt => {
  // a point inside the outline (centroid if it is inside; otherwise the middle of the widest run)
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const p of r) { x0 = Math.min(x0, p.x); y0 = Math.min(y0, p.y); x1 = Math.max(x1, p.x); y1 = Math.max(y1, p.y); }
  const c = { x: r.reduce((a, p) => a + p.x, 0) / r.length, y: r.reduce((a, p) => a + p.y, 0) / r.length };
  if (pip(c, r)) return c;
  let best = c, bd = -1;
  for (let i = 1; i < 12; i++) for (let j = 1; j < 12; j++) {
    const p = { x: x0 + ((x1 - x0) * i) / 12, y: y0 + ((y1 - y0) * j) / 12 };
    if (!pip(p, r)) continue;
    const d = ringDist(p, r);
    if (d > bd) { bd = d; best = p; }
  }
  return best;
};
export const areaCenter = center;

/** How long two outlines run together (shared wall), in mm. */
function sharedLength(a: Pt[], b: Pt[]) {
  let len = 0;
  for (let i = 0; i < a.length; i++) {
    const p = a[i], q = a[(i + 1) % a.length];
    const L = Math.hypot(q.x - p.x, q.y - p.y);
    const n = Math.max(1, Math.ceil(L / 200));
    let on = 0;
    for (let k = 0; k <= n; k++) {
      const t = k / n;
      if (ringDist({ x: p.x + (q.x - p.x) * t, y: p.y + (q.y - p.y) * t }, b) < 30) on++;
    }
    len += (L * on) / (n + 1);
  }
  return len;
}

function bbox(r: Pt[]) {
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const p of r) { x0 = Math.min(x0, p.x); y0 = Math.min(y0, p.y); x1 = Math.max(x1, p.x); y1 = Math.max(y1, p.y); }
  return { x0, y0, x1, y1 };
}
const boxesMeet = (a: ReturnType<typeof bbox>, b: ReturnType<typeof bbox>, m = 50) => a.x0 <= b.x1 + m && b.x0 <= a.x1 + m && a.y0 <= b.y1 + m && b.y0 <= a.y1 + m;

const cache = new WeakMap<Plan, Issue[]>();

/** Is the plan ready for the optimizer? Errors stop the next step; warnings do not. */
export function checkPlan(plan: Plan | null): Issue[] {
  if (!plan) return [{ level: 'error', text: 'Open a plan or start drawing.' }];
  const hit = cache.get(plan);
  if (hit) return hit;
  const out: Issue[] = [];
  const R = plan.region;
  const groups = areaGroups(R);
  const name = (gi: number) => `Area ${gi + 1}`;

  if (!groups.length) out.push({ level: 'error', text: 'Draw at least one area to furnish (a closed outline).' });

  // each outline
  const boxes = R.map(bbox);
  groups.forEach((g, gi) => {
    const r = R[g.outer];
    if (r.length < 3) { out.push({ level: 'error', text: `${name(gi)} has fewer than 3 corners.` }); return; }
    const x = R.length < 60 || r.length < 400 ? selfCross(r) : null;
    if (x) out.push({ level: 'error', text: `${name(gi)}: its outline crosses itself. Drag the corners so the lines do not cross.`, at: x });
    const a = ringArea(r) - g.holes.reduce((s, h) => s + ringArea(R[h]), 0);
    if (a < 1e6) out.push({ level: 'error', text: `${name(gi)} is too small (${(a / 1e6).toFixed(2)} m²).`, at: center(r) });
  });
  // overlapping areas (only touching is fine)
  for (let i = 0; i < groups.length; i++) for (let j = i + 1; j < groups.length; j++) {
    const a = R[groups[i].outer], b = R[groups[j].outer];
    if (!boxesMeet(boxes[groups[i].outer], boxes[groups[j].outer], 0)) continue;
    let at: Pt | null = null;
    for (let s = 0; s < a.length && !at; s++) for (let t = 0; t < b.length && !at; t++) {
      if (properCross(a[s], a[(s + 1) % a.length], b[t], b[(t + 1) % b.length])) at = a[s];
    }
    if (!at) at = a.find((p) => inside(p, b)) ?? b.find((p) => inside(p, a)) ?? null;
    if (at) out.push({ level: 'error', text: `${name(i)} and ${name(j)} overlap. Move a corner so they only touch, or draw them again (overlapping outlines are joined).`, at });
  }

  // doors
  const doors = plan.doors.filter((d) => d.kind !== 'ignore');
  const onAreas = (d: Door) => groups.map((g, gi) => ({ g, gi })).filter(({ g }) => {
    const m = mid(d);
    return ringDist(m, R[g.outer]) <= DOOR_TOL || (pip(m, R[g.outer]) && !g.holes.some((h) => pip(m, R[h]) && ringDist(m, R[h]) > DOOR_TOL))
      || g.holes.some((h) => ringDist(m, R[h]) <= DOOR_TOL);
  }).map(({ gi }) => gi);
  const mains = plan.doors.filter((d) => d.kind === 'main');
  if (!mains.length) out.push({ level: 'error', text: 'Draw the main entrance: the door people come in through, on the edge of an area.' });
  if (mains.length > 1) out.push({ level: 'error', text: `There are ${mains.length} main entrances: keep only one (the others can be exits or office doors).`, at: mid(mains[1]) });
  for (const d of mains) if (groups.length && !onAreas(d).length) out.push({ level: 'error', text: 'The main entrance does not touch any area: draw it on the edge of an area.', at: mid(d) });
  const off = doors.filter((d) => d.kind !== 'main' && groups.length && !onAreas(d).length);
  if (off.length === 1) out.push({ level: 'warn', text: 'A door is not on the edge of an area: it is ignored.', at: mid(off[0]) });
  else if (off.length > 1) out.push({ level: 'warn', text: `${off.length} doors are not on the edge of an area: they are ignored.`, at: mid(off[0]) });
  for (const d of doors) {
    const w = Math.hypot(d.b.x - d.a.x, d.b.y - d.a.y);
    if (w < 500) out.push({ level: 'warn', text: `A door is only ${(w / 1000).toFixed(2)} m wide. Is it right?`, at: mid(d) });
    else if (w > 4000) out.push({ level: 'warn', text: `A door is ${(w / 1000).toFixed(2)} m wide. Is it right?`, at: mid(d) });
  }

  // can people get into every area? (its own door, or through an area it shares a wall with)
  if (groups.length > 1 && doors.length) {
    const reach = groups.map(() => false);
    doors.forEach((d) => onAreas(d).forEach((gi) => { reach[gi] = true; }));
    const adj = groups.map(() => [] as number[]);
    for (let i = 0; i < groups.length; i++) for (let j = i + 1; j < groups.length; j++) {
      if (!boxesMeet(boxes[groups[i].outer], boxes[groups[j].outer])) continue;
      if (sharedLength(R[groups[i].outer], R[groups[j].outer]) >= 600) { adj[i].push(j); adj[j].push(i); }
    }
    const queue = reach.map((v, i) => (v ? i : -1)).filter((i) => i >= 0);
    while (queue.length) { const i = queue.pop()!; for (const j of adj[i]) if (!reach[j]) { reach[j] = true; queue.push(j); } }
    reach.forEach((v, gi) => {
      if (!v) out.push({ level: 'error', text: `${name(gi)} has no door: draw the door people use to get in (or join it to an area that has one).`, at: center(R[groups[gi].outer]) });
    });
  }

  // obstacles
  const outside: Pt[] = [];
  for (const o of plan.obstacles) {
    if (o.poly.length < 3) continue;
    const x = o.poly.length < 400 ? selfCross(o.poly) : null;
    if (x) out.push({ level: 'error', text: `An obstacle${o.label ? ` (${o.label})` : ''} crosses itself: redraw it.`, at: x });
    const ob = bbox(o.poly);
    const touches = groups.some((g) => {
      const r = R[g.outer];
      if (!boxesMeet(ob, boxes[g.outer])) return false;
      return o.poly.some((p) => pip(p, r)) || r.some((p) => pip(p, o.poly))
        || o.poly.some((p, i) => r.some((q, k) => properCross(p, o.poly[(i + 1) % o.poly.length], q, r[(k + 1) % r.length])));
    });
    if (groups.length && !touches) outside.push(center(o.poly));
  }
  if (outside.length === 1) out.push({ level: 'warn', text: 'An obstacle is outside the areas: it has no effect.', at: outside[0] });
  else if (outside.length > 1) out.push({ level: 'warn', text: `${outside.length} obstacles are outside the areas: they have no effect.`, at: outside[0] });

  cache.set(plan, out);
  return out;
}

export const planReady = (plan: Plan | null) => !checkPlan(plan).some((i) => i.level === 'error');
