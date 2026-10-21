// ¿Cabe un escritorio aquí? Revisión geométrica rápida (para la vista previa al colocar a mano):
// recinto, obstáculos y sus separaciones, despejes de puertas, pasillo principal y otros escritorios
// con sus sillas. La revisión completa (acceso a cada puesto, recorridos) la hace el motor al colocarlo.
import { chairClear, deskBody, deskDims, doorInfo, isTwo, moduleInfo, obstacleInfo, pathZones, regionInfo, ruleVal, wallDefault } from './clear';
import type { Config, DeskType, Layout, Plan, Pt } from './types';

function pip(p: Pt, poly: Pt[]) {
  let c = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const a = poly[i], b = poly[j];
    if ((a.y > p.y) !== (b.y > p.y) && p.x < ((b.x - a.x) * (p.y - a.y)) / (b.y - a.y) + a.x) c = !c;
  }
  return c;
}
const inRegion = (p: Pt, rings: Pt[][]) => rings.reduce((c, r) => (pip(p, r) ? !c : c), false);

interface Box { c: Pt; u: Pt; v: Pt; a0: number; a1: number; b0: number; b1: number }
/** Puntos de muestra dentro de un rectángulo local (un poco hacia adentro, para que tocar no cuente). */
function samples(b: Box, step = 100): Pt[] {
  const out: Pt[] = [];
  const m = 40;
  const na = Math.max(1, Math.ceil((b.a1 - b.a0 - 2 * m) / step)), nb = Math.max(1, Math.ceil((b.b1 - b.b0 - 2 * m) / step));
  for (let i = 0; i <= na; i++) for (let j = 0; j <= nb; j++) {
    const a = b.a0 + m + ((b.a1 - b.a0 - 2 * m) * i) / na, q = b.b0 + m + ((b.b1 - b.b0 - 2 * m) * j) / nb;
    out.push({ x: b.c.x + b.u.x * a + b.v.x * q, y: b.c.y + b.u.y * a + b.v.y * q });
  }
  return out;
}
const bbox = (poly: Pt[]) => {
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const p of poly) { x0 = Math.min(x0, p.x); y0 = Math.min(y0, p.y); x1 = Math.max(x1, p.x); y1 = Math.max(y1, p.y); }
  return { x0, y0, x1, y1 };
};
interface Shape { poly: Pt[]; bb: ReturnType<typeof bbox> }
const shape = (poly: Pt[]): Shape => ({ poly, bb: bbox(poly) });
const hits = (pts: Pt[], shapes: Shape[]) =>
  pts.some((p) => shapes.some((s) => p.x >= s.bb.x0 && p.x <= s.bb.x1 && p.y >= s.bb.y0 && p.y <= s.bb.y1 && pip(p, s.poly)));

export function checkDesk(plan: Plan, config: Config, layout: Layout | null, t: DeskType, center: Pt, angleDeg: number, seatsUp: boolean): string[] {
  const a = (angleDeg * Math.PI) / 180;
  const u = { x: Math.cos(a), y: Math.sin(a) }, v = { x: -u.y, y: u.x };
  const { length: len, depth: dep } = deskDims(t);
  const hl = len / 2, hd = dep / 2;
  const two = isTwo(t);
  const notch = t.shape === 'l' ? Math.max(0, dep - t.depth) : 0;
  const cc = Math.max(0, chairClear(config, t) - notch);
  const box = (a0: number, a1: number, b0: number, b1: number): Box => ({ c: center, u, v, a0, a1, b0, b1 });
  const body = samples(box(-hl, hl, -hd, hd));
  const sides = two ? [1, -1] : [seatsUp ? 1 : -1];
  const chair: Pt[] = sides.flatMap((s) => (cc > 0 ? samples(s > 0 ? box(-hl, hl, hd, hd + cc) : box(-hl, hl, -hd - cc, -hd)) : []));
  const extra: Pt[] = [];
  if (!two && (t.clearBack ?? 0) > 0) { const s = seatsUp ? -1 : 1, bk = t.clearBack!; extra.push(...samples(s > 0 ? box(-hl, hl, hd, hd + bk) : box(-hl, hl, -hd - bk, -hd))); }
  if ((t.clearLeft ?? 0) > 0) extra.push(...samples(box(-hl - t.clearLeft!, -hl, -hd, hd)));
  if ((t.clearRight ?? 0) > 0) extra.push(...samples(box(hl, hl + t.clearRight!, -hd, hd)));

  const blocking = plan.obstacles.filter((o) => o.kind !== 'reserved');
  const obst = blocking.map((o) => shape(o.poly));
  const obstClear = blocking.flatMap((o) => obstacleInfo(config, o).zones).map(shape);
  const wallClear = plan.region.flatMap((_, i) => regionInfo(plan, i, wallDefault(config)).zones).map(shape);
  const doorZones = [...plan.doors.filter((d) => d.kind !== 'ignore').flatMap((d) => doorInfo(config, plan, d).zones), ...(layout?.keepouts ?? [])].map(shape);
  const corridor = (layout?.corridors ?? []).map(shape);
  const paths = pathZones(config, plan).map(shape);
  const mods = layout?.modules ?? [];
  const bodies = mods.map((m) => shape(m.body));
  const otherClear = mods.flatMap((m) => { const tt = config.catalog.find((x) => x.id === m.typeId); return tt ? moduleInfo(config, m, tt).zones : []; }).map(shape);

  // filas enfrentadas por la espalda de la silla (regla E2): silla con silla y silla con escritorio
  const e2ss = ruleVal(config, 'E2', 'silla_silla') ?? 0, e2se = ruleVal(config, 'E2', 'silla_espalda') ?? 0;
  const chairD = ruleVal(config, 'E1', 'silla') ?? 500;
  const e2why: string[] = [];
  const m2 = (mm: number) => (mm / 1000).toFixed(2);
  const others = layout?.seats ?? [];
  for (const sd of sides) {
    const f = { x: -v.x * sd, y: -v.y * sd }; // hacia el escritorio desde la silla
    const zone = (d: number) => [0, 1, 2, 3].map((k) => {
      const a = k === 0 || k === 3 ? -hl + 60 : hl - 60, q = k < 2 ? hd : hd + d;
      return { x: center.x + u.x * a + v.x * q * sd, y: center.y + u.y * a + v.y * q * sd };
    });
    if (e2ss > 0) {
      const z = zone(e2ss - 25); // tolerancia: justo en la medida cuenta como que cumple
      const bad = others.find((o) => {
        if (o.facing.x * f.x + o.facing.y * f.y > -0.7) return false; // solo sillas que dan la espalda a la nuestra
        const e = { x: o.pos.x + o.facing.x * chairD * 0.5, y: o.pos.y + o.facing.y * chairD * 0.5 };
        return pip(e, z);
      });
      if (bad) e2why.push(`Chair back to back with seat ${bad.number}: ${m2(e2ss)} m are needed between the desks (rule “chairs back to back”)`);
    }
    if (e2se > 0 && hits(samples({ c: center, u, v: { x: v.x * sd, y: v.y * sd }, a0: -hl + 60, a1: hl - 60, b0: hd, b1: hd + e2se - 25 }), bodies)) {
      e2why.push(`Less than ${m2(e2se)} m behind the chair to another desk (rule “chair to the back of a desk”)`);
    }
  }
  // y al revés: el escritorio nuevo no puede quedar detrás de la silla de otro a menos de esa medida
  if (e2se > 0) {
    const bad = others.find((o) => {
      const e = { x: o.pos.x + o.facing.x * chairD * 0.5, y: o.pos.y + o.facing.y * chairD * 0.5 };
      const sdv = { x: -o.facing.y, y: o.facing.x }, w = 280;
      const z = [
        { x: e.x + sdv.x * w, y: e.y + sdv.y * w }, { x: e.x - sdv.x * w, y: e.y - sdv.y * w },
        { x: e.x - sdv.x * w - o.facing.x * (e2se - 25), y: e.y - sdv.y * w - o.facing.y * (e2se - 25) }, { x: e.x + sdv.x * w - o.facing.x * (e2se - 25), y: e.y + sdv.y * w - o.facing.y * (e2se - 25) },
      ];
      return body.some((p) => pip(p, z));
    });
    if (bad) e2why.push(`It is less than ${m2(e2se)} m behind the chair of seat ${bad.number} (rule “chair to the back of a desk”)`);
  }

  // escritorios en L o del Studio: la silla tiene que tener por dónde salir (no encajonada contra los muros)
  const shaped = deskBody(t, center, angleDeg, seatsUp);
  if (shaped.chairs?.length) {
    const sq = ruleVal(config, 'C2', 'paso_sillas') ?? 600;
    const freeAt = (q: Pt) => inRegion(q, plan.region) && !pip(q, shaped.body) && !obst.some((o) => pip(q, o.poly)) && !(mods.length && bodies.some((b) => pip(q, b.poly)));
    for (const c of shaped.chairs) {
      const ok = [0, 45, 90, 135, 180, 225, 270, 315].some((deg) => {
        const r = (deg * Math.PI) / 180;
        // un paso de ancho mínimo hacia afuera: tres puntos seguidos libres
        return [0.6, 1.0, 1.4].every((f) => freeAt({ x: c.x + Math.cos(r) * (sq * f + 250), y: c.y + Math.sin(r) * (sq * f + 250) }));
      });
      if (!ok) e2why.push('The chair is boxed in: there is no way out');
    }
  }

  const why = new Set<string>();
  const all = [...body, ...chair, ...extra];
  if (all.some((p) => !inRegion(p, plan.region))) why.add('It goes out of the area or hits a wall');
  if (hits(body, obst)) why.add('It hits an office, column or area');
  if (hits([...chair, ...extra], obst)) why.add('No room for the chair: there is an office or column behind');
  if (hits(body, obstClear) || hits(body, wallClear)) why.add('It is inside the clearance of an office, column or wall');
  if (hits([...body, ...chair], doorZones)) why.add('It blocks the clearance of a door or the entrance');
  if (hits([...body, ...chair], corridor)) why.add('It is on the main corridor');
  if (hits([...body, ...chair], paths)) why.add('Blocks a circulation path');
  if (hits(body, bodies)) why.add('It overlaps another desk');
  else if (hits([...chair, ...extra], bodies) || hits(body, otherClear)) why.add('Not enough room next to another desk (chairs or clearance)');
  e2why.forEach((w) => why.add(w));
  return [...why];
}
