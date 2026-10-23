// Edición manual estilo AutoCAD: selección por ventana / captura, mover, girar, borrar, fijar.
import { call } from './api';
import { areaGroups, regionArea } from './draw';
import { emptySel, selCount, useApp, type Selection } from './store';
import type { Layout, Module, Pt } from './types';

const S = () => useApp.getState();

// ------------------------------------------------------------ geometría
export interface Rect { x0: number; y0: number; x1: number; y1: number }
export const normRect = (a: Pt, b: Pt): Rect => ({ x0: Math.min(a.x, b.x), y0: Math.min(a.y, b.y), x1: Math.max(a.x, b.x), y1: Math.max(a.y, b.y) });
const inRect = (p: Pt, r: Rect) => p.x >= r.x0 && p.x <= r.x1 && p.y >= r.y0 && p.y <= r.y1;

function segHitsRect(a: Pt, b: Pt, r: Rect) {
  if (inRect(a, r) || inRect(b, r)) return true;
  const corners = [{ x: r.x0, y: r.y0 }, { x: r.x1, y: r.y0 }, { x: r.x1, y: r.y1 }, { x: r.x0, y: r.y1 }];
  for (let i = 0; i < 4; i++) if (segInter(a, b, corners[i], corners[(i + 1) % 4])) return true;
  return false;
}
function segInter(a: Pt, b: Pt, c: Pt, d: Pt) {
  const o = (p: Pt, q: Pt, r: Pt) => Math.sign((q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x));
  return o(a, b, c) !== o(a, b, d) && o(c, d, a) !== o(c, d, b);
}
export function pip(p: Pt, poly: Pt[]) {
  let c = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const a = poly[i], b = poly[j];
    if ((a.y > p.y) !== (b.y > p.y) && p.x < ((b.x - a.x) * (p.y - a.y)) / (b.y - a.y) + a.x) c = !c;
  }
  return c;
}
/** Ventana (azul): todo adentro. Captura (verde): cualquier contacto. */
export function polyPicked(poly: Pt[], r: Rect, crossing: boolean, closed = true) {
  if (!crossing) return poly.every((p) => inRect(p, r));
  const n = poly.length;
  for (let i = 0; i < (closed ? n : n - 1); i++) if (segHitsRect(poly[i], poly[(i + 1) % n], r)) return true;
  return closed && pip({ x: (r.x0 + r.x1) / 2, y: (r.y0 + r.y1) / 2 }, poly);
}

/** Selección dentro de un rectángulo. */
export function pickRect(r: Rect, crossing: boolean): Selection {
  const { plan, layout } = S();
  const sel = emptySel();
  layout?.modules.forEach((m) => { if (polyPicked(m.body, r, crossing)) sel.modules.push(m.id); });
  plan?.obstacles.forEach((o) => { if (polyPicked(o.poly, r, crossing)) sel.obstacles.push(o.id); });
  plan?.doors.forEach((d) => { if (polyPicked([d.a, d.b], r, crossing, false)) sel.doors.push(d.id); });
  plan?.windows.forEach((w, i) => { if (polyPicked([w.a, w.b], r, crossing, false)) sel.windows.push(i); });
  return sel;
}

function distSeg(p: Pt, a: Pt, b: Pt) {
  const dx = b.x - a.x, dy = b.y - a.y;
  const l2 = dx * dx + dy * dy || 1;
  const t = Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / l2));
  return Math.hypot(p.x - (a.x + dx * t), p.y - (a.y + dy * t));
}

/** Elemento bajo el cursor (tolerancia en mm). */
export function pickPoint(p: Pt, tol: number): Selection {
  const { plan, layout } = S();
  const sel = emptySel();
  const m = layout?.modules.find((m) => pip(p, m.body));
  if (m) { sel.modules.push(m.id); return sel; }
  // un clic sobre una silla selecciona su escritorio
  const seat = layout?.seats.find((s) => pip(p, s.chair));
  if (seat) { sel.modules.push(seat.moduleId); return sel; }
  if (plan) {
    const d = plan.doors.find((d) => distSeg(p, d.a, d.b) < tol);
    if (d) { sel.doors.push(d.id); return sel; }
    const wi = plan.windows.findIndex((w) => distSeg(p, w.a, w.b) < tol);
    if (wi >= 0) { sel.windows.push(wi); return sel; }
    const o = [...plan.obstacles].reverse().find((o) => pip(p, o.poly));
    if (o) { sel.obstacles.push(o.id); return sel; }
    // borde del recinto (para ver y editar su separación por arista)
    const ri = plan.region.findIndex((r) => r.some((a, i) => distSeg(p, a, r[(i + 1) % r.length]) < tol * 1.5));
    if (ri >= 0) { sel.region = [ri]; return sel; }
  }
  return sel;
}

export function mergeSel(a: Selection, b: Selection, remove: boolean): Selection {
  const op = <T,>(x: T[], y: T[]) => (remove ? x.filter((v) => !y.includes(v)) : [...x, ...y.filter((v) => !x.includes(v))]);
  return { modules: op(a.modules, b.modules), obstacles: op(a.obstacles, b.obstacles), doors: op(a.doors, b.doors), windows: op(a.windows, b.windows), region: op(a.region ?? [], b.region ?? []), bg: op(a.bg ?? [], b.bg ?? []) };
}

// ------------------------------------------------------------ transformaciones
const tr = (p: Pt, dx: number, dy: number): Pt => ({ x: p.x + dx, y: p.y + dy });
const rot = (p: Pt, c: Pt, deg: number): Pt => {
  const a = (deg * Math.PI) / 180, cs = Math.cos(a), sn = Math.sin(a);
  return { x: c.x + (p.x - c.x) * cs - (p.y - c.y) * sn, y: c.y + (p.x - c.x) * sn + (p.y - c.y) * cs };
};

/** Personas actuales de un módulo (para bloquearlas al fijar). */
function peopleOf(l: Layout, id: string) {
  return l.seats.filter((s) => s.moduleId === id).map((s) => ({ pos: s.pos, roleId: s.roleId, clientId: s.clientId }));
}

/** Sincroniza config.pinned con los módulos fijados del layout. */
function syncPinned(l: Layout) {
  S().updateConfig((c) => {
    c.pinned = l.modules.filter((m) => m.pinned).map((m) => ({ ...m, people: peopleOf(l, m.id) }));
  });
}

let evalTimer: ReturnType<typeof setTimeout> | null = null;
/** Recalcula indicadores y avisos tras una edición (con pequeña espera para agrupar cambios). */
export function evaluateSoon() {
  if (evalTimer) clearTimeout(evalTimer);
  evalTimer = setTimeout(async () => {
    const { plan, config, layout } = S();
    if (!plan || !config || !layout) return;
    try {
      const nl = await call<Layout>('evaluate', { problem: { plan, config }, layout });
      S().set({ layout: nl });
    } catch (e) {
      S().say(String(e instanceof Error ? e.message : e), 'error');
    }
  }, 250);
}

function editLayout(f: (l: Layout) => void) {
  const l = S().layout;
  if (!l) return;
  const n = structuredClone(l);
  f(n);
  S().set({ layout: n });
  syncPinned(n);
}

export function moveSelection(dx: number, dy: number) {
  const sel = S().selection;
  if (!sel.modules.length && !sel.obstacles.length && !sel.doors.length && !sel.windows.length && !(sel.region?.length && S().step === 0)) return;
  S().snapshot();
  if (sel.modules.length) {
    editLayout((l) => {
      for (const m of l.modules) {
        if (!sel.modules.includes(m.id)) continue;
        m.body = m.body.map((p) => tr(p, dx, dy));
        m.center = tr(m.center, dx, dy);
        m.pinned = true; // lo que se acomoda a mano queda fijado
      }
      for (const s of l.seats) {
        if (!sel.modules.includes(s.moduleId)) continue;
        s.pos = tr(s.pos, dx, dy);
        s.chair = s.chair.map((p) => tr(p, dx, dy));
        if (s.roleId) s.locked = true;
      }
    });
  }
  if (sel.obstacles.length || sel.doors.length || sel.windows.length) {
    const plan = S().plan;
    if (plan) {
      const n = structuredClone(plan);
      n.obstacles.forEach((o) => { if (sel.obstacles.includes(o.id)) o.poly = o.poly.map((p) => tr(p, dx, dy)); });
      n.doors.forEach((d) => { if (sel.doors.includes(d.id)) { d.a = tr(d.a, dx, dy); d.b = tr(d.b, dx, dy); } });
      n.windows.forEach((w, i) => { if (sel.windows.includes(i)) { w.a = tr(w.a, dx, dy); w.b = tr(w.b, dx, dy); } });
      if (sel.doors.length) n.paths = [];
      S().set({ plan: n });
    }
  }
  if (sel.region?.length && S().step === 0) {
    // whole areas (with their holes) move too
    const plan = S().plan;
    if (plan) {
      const groups = areaGroups(plan.region);
      const move = new Set<number>();
      for (const i of sel.region) { move.add(i); groups.find((g) => g.outer === i)?.holes.forEach((h) => move.add(h)); }
      const n = structuredClone(plan);
      n.region = n.region.map((r, i) => (move.has(i) ? r.map((p) => tr(p, dx, dy)) : r));
      n.paths = [];
      S().set({ plan: n, layout: null, planMode: 'draw' });
    }
  }
  evaluateSoon();
}

const midOf = (a: Pt, b: Pt): Pt => ({ x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 });

/** Doors: flip the side they open to. Obstacles: mirror them left to right. */
export function flipSelection() {
  const sel = S().selection;
  const plan = S().plan;
  if (!plan || (!sel.doors.length && !sel.obstacles.length)) return;
  S().snapshot();
  const n = structuredClone(plan);
  n.doors.forEach((d) => { if (sel.doors.includes(d.id)) d.flip = !d.flip; });
  n.obstacles.forEach((o) => {
    if (!sel.obstacles.includes(o.id)) return;
    const cx = o.poly.reduce((a, p) => a + p.x, 0) / o.poly.length;
    o.poly = o.poly.map((p) => ({ x: 2 * cx - p.x, y: p.y })).reverse();
    if (o.clear?.length === 4) o.clear = [o.clear[2], o.clear[1], o.clear[0], o.clear[3]];
  });
  n.paths = sel.doors.length ? [] : n.paths;
  S().set({ plan: n });
  evaluateSoon();
}

/** Doors: the hinge goes to the other end of the opening. */
export function flipHinge() {
  const sel = S().selection;
  const plan = S().plan;
  if (!plan || !sel.doors.length) return;
  S().snapshot();
  const n = structuredClone(plan);
  n.doors.forEach((d) => { if (sel.doors.includes(d.id)) { const a = d.a; d.a = d.b; d.b = a; d.flip = !d.flip; } });
  S().set({ plan: n });
}

/** Asks for an angle and rotates the selection by it. */
export function rotatePrompt() {
  const ans = prompt('Rotate by how many degrees? (positive = counterclockwise)', '45');
  const deg = parseFloat((ans ?? '').replace(',', '.'));
  if (isFinite(deg) && deg !== 0) rotateSelection(deg);
}

export function rotateSelection(deg = 90) {
  const sel = S().selection;
  if (!sel.modules.length && !sel.obstacles.length && !sel.doors.length && !sel.windows.length) return;
  S().snapshot();
  if (sel.modules.length) editLayout((l) => {
    for (const m of l.modules) {
      if (!sel.modules.includes(m.id)) continue;
      const c = m.center;
      m.body = m.body.map((p) => rot(p, c, deg));
      m.angle = (m.angle + deg) % 360;
      m.pinned = true;
      for (const s of l.seats) {
        if (s.moduleId !== m.id) continue;
        s.pos = rot(s.pos, c, deg);
        s.chair = s.chair.map((p) => rot(p, c, deg));
        s.facing = rot(s.facing, { x: 0, y: 0 }, deg);
        if (s.roleId) s.locked = true;
      }
    }
  });
  if (sel.obstacles.length || sel.doors.length || sel.windows.length) {
    const plan = S().plan;
    if (plan) {
      const n = structuredClone(plan);
      n.obstacles.forEach((o) => {
        if (!sel.obstacles.includes(o.id)) return;
        const c = o.poly.reduce((a, p) => ({ x: a.x + p.x / o.poly.length, y: a.y + p.y / o.poly.length }), { x: 0, y: 0 });
        o.poly = o.poly.map((p) => rot(p, c, deg));
      });
      // doors and windows turn about their middle
      n.doors.forEach((d) => { if (sel.doors.includes(d.id)) { const c = midOf(d.a, d.b); d.a = rot(d.a, c, deg); d.b = rot(d.b, c, deg); } });
      n.windows.forEach((w, i) => { if (sel.windows.includes(i)) { const c = midOf(w.a, w.b); w.a = rot(w.a, c, deg); w.b = rot(w.b, c, deg); } });
      if (sel.doors.length) n.paths = [];
      S().set({ plan: n });
    }
  }
  evaluateSoon();
}

/** Hides the selected lines of the drawing (only on screen: the file does not change). */
export function hideSelectedLines() {
  const sel = S().selection;
  if (!sel.bg?.length) return;
  const h = new Set(S().hiddenPaths);
  sel.bg.forEach((k) => h.add(k));
  S().set({ hiddenPaths: [...h], selection: { ...sel, bg: [] } });
}

export function deleteSelection() {
  const sel = S().selection;
  if (sel.bg?.length) hideSelectedLines(); // lines of the drawing are only hidden
  if (selCount({ ...sel, bg: [] }) === 0) return;
  S().snapshot();
  if (sel.modules.length) {
    editLayout((l) => {
      l.modules = l.modules.filter((m) => !sel.modules.includes(m.id));
      l.seats = l.seats.filter((s) => !sel.modules.includes(s.moduleId));
    });
  }
  if (sel.obstacles.length || sel.doors.length || sel.windows.length) {
    const plan = S().plan;
    if (plan) {
      const n = structuredClone(plan);
      n.obstacles = n.obstacles.filter((o) => !sel.obstacles.includes(o.id));
      n.doors = n.doors.filter((d) => !sel.doors.includes(d.id));
      n.windows = n.windows.filter((_, i) => !sel.windows.includes(i));
      S().set({ plan: n });
    }
  }
  if (sel.region?.length && S().step === 0) {
    const plan = S().plan;
    if (plan) {
      // an area goes with its holes
      const groups = areaGroups(plan.region);
      const drop = new Set<number>();
      for (const i of sel.region) { drop.add(i); groups.find((g) => g.outer === i)?.holes.forEach((h) => drop.add(h)); }
      const rc = plan.regionClear ?? [];
      const keep = plan.region.map((_, i) => i).filter((i) => !drop.has(i));
      const n = { ...structuredClone(plan), region: keep.map((i) => plan.region[i]), regionClear: keep.map((i) => rc[i] ?? []), paths: [] };
      S().set({ plan: n, areaM2: regionArea(n.region) / 1e6, layout: null, planMode: 'draw' });
    }
  }
  S().set({ selection: emptySel() });
  evaluateSoon();
}

/** Fija los escritorios seleccionados; con `lockPeople` también bloquea a las personas sentadas. */
export function pinSelection(pin: boolean, lockPeople = true) {
  const sel = S().selection;
  if (!sel.modules.length) return;
  S().snapshot();
  editLayout((l) => {
    for (const m of l.modules) if (sel.modules.includes(m.id)) m.pinned = pin;
    for (const s of l.seats) if (sel.modules.includes(s.moduleId)) s.locked = pin && lockPeople && !!s.roleId;
  });
  const n = sel.modules.length;
  S().say(pin ? `${n} desk${n > 1 ? 's' : ''} pinned: the next optimization keeps ${n > 1 ? 'them' : 'it'}${lockPeople ? ' with their people' : ''}.` : `${n} desk${n > 1 ? 's' : ''} released.`);
}

export function selectAll() {
  const l = S().layout;
  if (l) S().set({ selection: { ...emptySel(), modules: l.modules.map((m) => m.id) } });
}

/** Resumen de la selección para la barra de acciones. */
export function describeSel(sel: Selection): string {
  const { layout } = S();
  const parts: string[] = [];
  if (sel.modules.length) {
    const seats = layout?.seats.filter((s) => sel.modules.includes(s.moduleId)).length ?? 0;
    parts.push(`${sel.modules.length} desk${sel.modules.length > 1 ? 's' : ''} (${seats} seats)`);
  }
  if (sel.obstacles.length) parts.push(`${sel.obstacles.length} obstacle${sel.obstacles.length > 1 ? 's' : ''}`);
  if (sel.doors.length) parts.push(`${sel.doors.length} door${sel.doors.length > 1 ? 's' : ''}`);
  if (sel.windows.length) parts.push(`${sel.windows.length} window${sel.windows.length > 1 ? 's' : ''}`);
  if (sel.region?.length) parts.push('area outline');
  if (sel.bg?.length) {
    const t = sel.bg.filter((k) => k.startsWith('t')).length, l = sel.bg.length - t;
    if (l) parts.push(`${l} drawing line${l > 1 ? 's' : ''}`);
    if (t) parts.push(`${t} text${t > 1 ? 's' : ''}`);
  }
  return parts.join(', ');
}

export function pinnedModuleCount(): number {
  return S().layout?.modules.filter((m: Module) => m.pinned).length ?? 0;
}
