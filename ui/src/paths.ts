// Circulation paths that run side by side become one: the other path follows this one up to a node
// and leaves it there (like a corridor that branches).
import type { CircPath, Pt } from './types';

const d2 = (a: Pt, b: Pt) => Math.hypot(a.x - b.x, a.y - b.y);
function distSeg(p: Pt, a: Pt, b: Pt) {
  const dx = b.x - a.x, dy = b.y - a.y, l2 = dx * dx + dy * dy || 1;
  const t = Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / l2));
  return Math.hypot(p.x - (a.x + dx * t), p.y - (a.y + dy * t));
}
const distLine = (p: Pt, pts: Pt[]) => {
  let d = Infinity;
  for (let i = 0; i + 1 < pts.length; i++) d = Math.min(d, distSeg(p, pts[i], pts[i + 1]));
  return d;
};
/** Arc length along `pts` of the point closest to `p`. */
function along(p: Pt, pts: Pt[]): number {
  let best = Infinity, at = 0, acc = 0;
  for (let i = 0; i + 1 < pts.length; i++) {
    const a = pts[i], b = pts[i + 1];
    const dx = b.x - a.x, dy = b.y - a.y, L = Math.hypot(dx, dy), l2 = L * L || 1;
    const t = Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / l2));
    const d = Math.hypot(p.x - (a.x + dx * t), p.y - (a.y + dy * t));
    if (d < best) { best = d; at = acc + t * L; }
    acc += L;
  }
  return at;
}
function pointAt(pts: Pt[], s: number): { p: Pt; seg: number } {
  let acc = 0;
  for (let i = 0; i + 1 < pts.length; i++) {
    const L = d2(pts[i], pts[i + 1]);
    if (acc + L >= s || i + 2 === pts.length) {
      const t = L > 0 ? Math.max(0, Math.min(1, (s - acc) / L)) : 0;
      return { p: { x: pts[i].x + (pts[i + 1].x - pts[i].x) * t, y: pts[i].y + (pts[i + 1].y - pts[i].y) * t }, seg: i };
    }
    acc += L;
  }
  return { p: pts[pts.length - 1], seg: Math.max(0, pts.length - 2) };
}
/** Does the run a–b stay within `tol` of the line `pts` all along? */
function runNear(a: Pt, b: Pt, pts: Pt[], tol: number) {
  const n = Math.max(1, Math.ceil(d2(a, b) / 150));
  for (let k = 0; k <= n; k++) {
    const t = k / n;
    if (distLine({ x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t }, pts) > tol) return false;
  }
  return true;
}
function projSeg(p: Pt, a: Pt, b: Pt): Pt {
  const dx = b.x - a.x, dy = b.y - a.y, l2 = dx * dx + dy * dy || 1;
  const t = Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / l2));
  return { x: a.x + dx * t, y: a.y + dy * t };
}
/** Without repeated nodes, nodes in the middle of a straight run, or spikes (going and coming back). */
function tidy(pts: Pt[]): Pt[] {
  const out: Pt[] = [];
  for (const p of pts) if (!out.length || d2(p, out[out.length - 1]) > 1) out.push(p);
  let changed = true;
  while (changed) {
    changed = false;
    for (let i = out.length - 2; i >= 1; i--) {
      const a = out[i - 1], b = out[i], c = out[i + 1];
      const cross = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
      const base = Math.max(Math.hypot(c.x - a.x, c.y - a.y), Math.hypot(b.x - a.x, b.y - a.y), 1);
      if (Math.abs(cross) / base < 1) { out.splice(i, 1); changed = true; break; }
    }
  }
  return out;
}

/** `other` follows `ref` from their common start up to the last node of `ref` it runs next to
 *  (at most `upTo`), then goes on as before. Null when they do not start together. */
function mergeFromStart(ref: Pt[], other: Pt[], tol: number, upTo: number): Pt[] | null {
  if (ref.length < 2 || other.length < 2 || d2(ref[0], other[0]) > tol) return null;
  // the last node of ref reached while the whole run so far stays next to other
  let k = 0;
  for (let i = 1; i < ref.length && i <= upTo; i++) {
    if (!runNear(ref[i - 1], ref[i], other, tol)) break;
    k = i;
  }
  // then along the next run of ref, as far as the other one keeps next to it: it parts there
  if (k < upTo && k < ref.length - 1) {
    const shared = ref.slice(0, k + 2);
    let m = 0;
    while (m < other.length && distLine(other[m], shared) <= tol) m++;
    const a = ref[k], b = ref[k + 1];
    if (m >= other.length) {
      // the other one ends next to this run
      const end = other[other.length - 1];
      return tidy([...ref.slice(0, k + 1), projSeg(end, a, b), end]);
    }
    if (m > 0) {
      const J = projSeg(other[m - 1], a, b);
      if (d2(J, a) > tol) return tidy([...ref.slice(0, k + 1), J, ...other.slice(m - 1)]);
    }
  }
  if (k === 0) return null;
  const junction = ref[k];
  const s = along(junction, other);
  const total = other.reduce((a, p, i) => (i ? a + d2(other[i - 1], p) : 0), 0);
  // other ends there too: it is the same route
  if (total - s < tol) return tidy([...ref.slice(0, k + 1), other[other.length - 1]]);
  const { seg } = pointAt(other, s);
  const rest = other.slice(seg + 1);
  const v = rest[0];
  const out = [...ref.slice(0, k + 1)];
  if (v && Math.abs(v.x - junction.x) > 1 && Math.abs(v.y - junction.y) > 1) {
    // keep the runs straight: a corner toward the next node of the other path
    const c1 = { x: junction.x, y: v.y }, c2 = { x: v.x, y: junction.y };
    out.push(distLine(c1, other) <= distLine(c2, other) ? c1 : c2);
  }
  out.push(...rest);
  return tidy(out);
}

/** `other` merged into `ref` where they run side by side, from either end. Null = nothing to merge. */
export function mergeInto(ref: Pt[], other: Pt[], tol: number, upTo = Infinity): Pt[] | null {
  const same = (a: Pt[], b: Pt[]) => a.length === b.length && a.every((p, i) => d2(p, b[i]) < 1);
  const tries: (() => Pt[] | null)[] = [
    () => mergeFromStart(ref, other, tol, upTo),
    () => { const r = mergeFromStart(ref, [...other].reverse(), tol, upTo); return r && r.reverse(); },
    () => { const r = mergeFromStart([...ref].reverse(), other, tol, upTo === Infinity ? Infinity : ref.length - 1 - upTo); return r; },
    () => { const r = mergeFromStart([...ref].reverse(), [...other].reverse(), tol, upTo === Infinity ? Infinity : ref.length - 1 - upTo); return r && r.reverse(); },
  ];
  for (const t of tries) {
    const r = t();
    if (r && r.length >= 2 && !same(r, other)) {
      // only when it really is shorter or the same length (no detours)
      const len = (pts: Pt[]) => pts.reduce((a, p, i) => (i ? a + d2(pts[i - 1], p) : 0), 0);
      if (len(r) <= len(other) + tol * 2) return r;
    }
  }
  return null;
}

/** Paths that run side by side (closer than `tol`) share their common stretch. Entrance paths
 *  come first and stay; the later ones follow them. `fixed` paths are never changed. */
export function mergeSimilar(paths: CircPath[], tolOf: (a: CircPath, b: CircPath) => number, fixed: (p: CircPath) => boolean = (p) => !!p.edited): { paths: CircPath[]; merged: number } {
  // paths edited by hand lead, then entrance paths, then office to office
  const rank = (p: CircPath) => (p.edited ? 0 : p.kind === 'primary' ? 1 : 2);
  const order = [...paths].sort((a, b) => rank(a) - rank(b));
  const done: CircPath[] = [];
  let merged = 0;
  for (const p0 of order) {
    let p = p0;
    if (!fixed(p)) {
      for (const q of done) {
        const r = mergeInto(q.pts, p.pts, tolOf(q, p));
        if (r) { p = { ...p, pts: r }; merged++; }
      }
    }
    done.push(p);
  }
  return { paths: paths.map((x) => done.find((d) => d.id === x.id) ?? x), merged };
}
