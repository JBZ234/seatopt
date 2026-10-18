// ¿Cabe un escritorio aquí? Revisión geométrica rápida (para la vista previa al colocar a mano):
// recinto, obstáculos y sus separaciones, despejes de puertas, pasillo principal y otros escritorios
// con sus sillas. La revisión completa (acceso a cada puesto, recorridos) la hace el motor al colocarlo.
import { chairClear, doorInfo, moduleInfo, obstacleInfo, regionInfo, ruleVal } from './clear';
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
  const per = t.flexible ? Math.max(t.minPerSide, Math.min(t.maxPerSide, t.perSide)) : t.perSide;
  const len = t.shape === 'l' ? t.seatWidth : t.seatWidth * per;
  const dep = t.shape === 'l' ? Math.max(t.returnLength, t.depth) : t.depth * t.sides;
  const hl = len / 2, hd = dep / 2;
  const two = t.shape === 'straight' && t.sides >= 2;
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
  const wallClear = plan.region.flatMap((_, i) => regionInfo(plan, i).zones).map(shape);
  const doorZones = [...plan.doors.filter((d) => d.kind !== 'ignore').flatMap((d) => doorInfo(config, plan, d).zones), ...(layout?.keepouts ?? [])].map(shape);
  const corridor = (layout?.corridors ?? []).map(shape);
  const mods = layout?.modules ?? [];
  const bodies = mods.map((m) => shape(m.body));
  const otherClear = mods.flatMap((m) => { const tt = config.catalog.find((x) => x.id === m.typeId); return tt ? moduleInfo(config, m, tt).zones : []; }).map(shape);

  // filas enfrentadas por la espalda de la silla (regla E2): silla con silla y silla con escritorio
  const e2ss = ruleVal(config, 'E2', 'silla_silla') ?? 0, e2se = ruleVal(config, 'E2', 'silla_espalda') ?? 0;
  const chairD = ruleVal(config, 'E1', 'silla') ?? 500;
  const e2why: string[] = [];
  const m2 = (mm: number) => (mm / 1000).toFixed(2).replace('.', ',');
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
      if (bad) e2why.push(`Silla espalda con espalda con el puesto ${bad.number}: entre escritorios deben quedar ${m2(e2ss)} m (regla «silla con silla»)`);
    }
    if (e2se > 0 && hits(samples({ c: center, u, v: { x: v.x * sd, y: v.y * sd }, a0: -hl + 60, a1: hl - 60, b0: hd, b1: hd + e2se - 25 }), bodies)) {
      e2why.push(`Detrás de la silla quedan menos de ${m2(e2se)} m hasta otro escritorio (regla «silla con espalda»)`);
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
    if (bad) e2why.push(`Queda a menos de ${m2(e2se)} m detrás de la silla del puesto ${bad.number} (regla «silla con espalda»)`);
  }

  const why = new Set<string>();
  const all = [...body, ...chair, ...extra];
  if (all.some((p) => !inRegion(p, plan.region))) why.add('Se sale del recinto o choca con un muro');
  if (hits(body, obst)) why.add('Choca con una oficina, columna o zona');
  if (hits([...chair, ...extra], obst)) why.add('La silla no tiene espacio: hay una oficina o columna detrás');
  if (hits(body, obstClear) || hits(body, wallClear)) why.add('Invade la separación de una oficina, columna o muro');
  if (hits([...body, ...chair], doorZones)) why.add('Invade el despeje de una puerta o de la entrada');
  if (hits([...body, ...chair], corridor)) why.add('Invade el pasillo principal');
  if (hits(body, bodies)) why.add('Se monta sobre otro escritorio');
  else if (hits([...chair, ...extra], bodies) || hits(body, otherClear)) why.add('Queda sin espacio con otro escritorio (sillas o separación)');
  e2why.forEach((w) => why.add(w));
  return [...why];
}
