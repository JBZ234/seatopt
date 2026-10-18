// Studio de escritorios: se arma la forma juntando rectángulos, se ponen las sillas y se deciden
// las separaciones. El resultado es un tipo más del catálogo (forma «propia»).
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ruleVal } from '../clear';
import { uid, useApp } from '../store';
import type { CustomSeat, DeskType, Pt } from '../types';
import { Num, Switch } from './ui';

const CHAIR = 500; // fondo de la silla (dibujo)
const CW = 480; // ancho de la silla (dibujo)

interface Part { x: number; y: number; w: number; h: number }
interface Clear { chair: number | null; back: number; left: number; right: number }
interface Doc { parts: Part[]; seats: CustomSeat[]; clr: Clear }

// ------------------------------------------------------------------ geometría
function bbox(ps: Pt[]) {
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const p of ps) { x0 = Math.min(x0, p.x); y0 = Math.min(y0, p.y); x1 = Math.max(x1, p.x); y1 = Math.max(y1, p.y); }
  return { x0, y0, x1, y1, w: x1 - x0, h: y1 - y0, cx: (x0 + x1) / 2, cy: (y0 + y1) / 2 };
}
const partPts = (r: Part): Pt[] => [{ x: r.x, y: r.y }, { x: r.x + r.w, y: r.y }, { x: r.x + r.w, y: r.y + r.h }, { x: r.x, y: r.y + r.h }];
const area = (poly: Pt[]) => poly.reduce((s, p, i) => { const q = poly[(i + 1) % poly.length]; return s + p.x * q.y - q.x * p.y; }, 0) / 2;

/** Contorno de la unión de rectángulos (rejilla comprimida + recorrido del borde). `null` si las piezas no se tocan. */
function unionOutline(parts: Part[]): Pt[] | null {
  if (!parts.length) return null;
  const xs = [...new Set(parts.flatMap((r) => [r.x, r.x + r.w]))].sort((a, b) => a - b);
  const ys = [...new Set(parts.flatMap((r) => [r.y, r.y + r.h]))].sort((a, b) => a - b);
  const nx = xs.length - 1, ny = ys.length - 1;
  const fill = (i: number, j: number) => {
    if (i < 0 || j < 0 || i >= nx || j >= ny) return false;
    const cx = (xs[i] + xs[i + 1]) / 2, cy = (ys[j] + ys[j + 1]) / 2;
    return parts.some((r) => cx > r.x && cx < r.x + r.w && cy > r.y && cy < r.y + r.h);
  };
  // ¿una sola pieza conectada?
  const seen = new Set<string>();
  let start: [number, number] | null = null, total = 0;
  for (let i = 0; i < nx; i++) for (let j = 0; j < ny; j++) if (fill(i, j)) { total++; if (!start) start = [i, j]; }
  if (!start) return null;
  const stack = [start];
  while (stack.length) {
    const [i, j] = stack.pop()!;
    const k = i + ',' + j;
    if (seen.has(k) || !fill(i, j)) continue;
    seen.add(k);
    stack.push([i + 1, j], [i - 1, j], [i, j + 1], [i, j - 1]);
  }
  if (seen.size !== total) return null;
  // aristas del borde orientadas en sentido antihorario (relleno a la izquierda)
  const next = new Map<string, string>();
  const key = (i: number, j: number) => i + ',' + j;
  for (let i = 0; i < nx; i++) for (let j = 0; j < ny; j++) {
    if (!fill(i, j)) continue;
    if (!fill(i, j - 1)) next.set(key(i, j), key(i + 1, j));
    if (!fill(i + 1, j)) next.set(key(i + 1, j), key(i + 1, j + 1));
    if (!fill(i, j + 1)) next.set(key(i + 1, j + 1), key(i, j + 1));
    if (!fill(i - 1, j)) next.set(key(i, j + 1), key(i, j));
  }
  const first = next.keys().next().value as string;
  const loop: Pt[] = [];
  let cur = first, guard = 0;
  do {
    const [i, j] = cur.split(',').map(Number);
    loop.push({ x: xs[i], y: ys[j] });
    cur = next.get(cur)!;
    guard++;
  } while (cur && cur !== first && guard < 10000);
  // quitar puntos alineados
  const out: Pt[] = [];
  for (let i = 0; i < loop.length; i++) {
    const a = loop[(i + loop.length - 1) % loop.length], b = loop[i], c = loop[(i + 1) % loop.length];
    if (Math.abs((b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x)) > 1e-6) out.push(b);
  }
  return out;
}

/** Silla pegada al borde más cercano: queda afuera y la persona mira hacia el escritorio. */
function seatOnEdge(poly: Pt[], p: Pt, step: number): CustomSeat | null {
  const ccw = area(poly) > 0;
  let best: { d: number; s: CustomSeat } | null = null;
  for (let i = 0; i < poly.length; i++) {
    const a = poly[i], b = poly[(i + 1) % poly.length];
    const ex = b.x - a.x, ey = b.y - a.y, l = Math.hypot(ex, ey);
    if (l < CW * 0.6) continue;
    const ux = ex / l, uy = ey / l;
    const out = ccw ? { x: uy, y: -ux } : { x: -uy, y: ux };
    let t = (p.x - a.x) * ux + (p.y - a.y) * uy;
    if (step > 0) t = Math.round(t / step) * step;
    t = Math.max(CW / 2, Math.min(l - CW / 2, t));
    const q = { x: a.x + ux * t, y: a.y + uy * t };
    const d = Math.hypot(p.x - q.x, p.y - q.y);
    if (!best || d < best.d) best = { d, s: { x: q.x + (out.x * CHAIR) / 2, y: q.y + (out.y * CHAIR) / 2, facing: { x: -out.x, y: -out.y } } };
  }
  return best?.s ?? null;
}

// ------------------------------------------------------------------ plantillas
const seatTop = (x: number, top: number): CustomSeat => ({ x, y: top + CHAIR / 2, facing: { x: 0, y: -1 } });
const seatBottom = (x: number, bottom: number): CustomSeat => ({ x, y: bottom - CHAIR / 2, facing: { x: 0, y: 1 } });
const TEMPLATES: { id: string; name: string; make: () => { parts: Part[]; seats: CustomSeat[] } }[] = [
  { id: 'rect', name: 'Recto', make: () => ({ parts: [{ x: -600, y: -300, w: 1200, h: 600 }], seats: [seatTop(0, 300)] }) },
  { id: 'l', name: 'En L', make: () => ({ parts: [{ x: -750, y: 0, w: 1500, h: 600 }, { x: -750, y: -900, w: 600, h: 900 }], seats: [seatBottom(225, 0)] }) },
  { id: 'u', name: 'En U', make: () => ({ parts: [{ x: -900, y: 0, w: 1800, h: 600 }, { x: -900, y: -900, w: 500, h: 900 }, { x: 400, y: -900, w: 500, h: 900 }], seats: [seatBottom(0, 0)] }) },
  { id: 'pair', name: 'Pareja', make: () => ({ parts: [{ x: -600, y: -600, w: 1200, h: 600 }, { x: -600, y: 0, w: 1200, h: 600 }], seats: [seatTop(0, 600), seatBottom(0, -600)] }) },
  { id: 'isla4', name: 'Isla de 4', make: () => ({ parts: [{ x: -1200, y: -600, w: 2400, h: 1200 }], seats: [seatTop(-600, 600), seatTop(600, 600), seatBottom(-600, -600), seatBottom(600, -600)] }) },
];

export default function DeskStudio() {
  const st = useApp();
  const c = st.config;
  const editT = st.studio?.editId ? c?.catalog.find((t) => t.id === st.studio!.editId) : undefined;
  const e1 = c ? ruleVal(c, 'E1', 'muro') ?? 900 : 900;

  const initial = (): Doc => {
    if (editT) {
      const parts = (editT as DeskType & { parts?: Part[] }).parts;
      const bb = editT.body?.length ? bbox(editT.body) : null;
      return {
        parts: parts?.length ? parts.map((p) => ({ ...p })) : bb ? [{ x: bb.x0, y: bb.y0, w: bb.w, h: bb.h }] : TEMPLATES[0].make().parts,
        seats: (editT.seats ?? []).map((s) => ({ ...s, facing: { ...s.facing } })),
        clr: { chair: editT.clearChair ?? null, back: editT.clearBack ?? 0, left: editT.clearLeft ?? 0, right: editT.clearRight ?? 0 },
      };
    }
    const t = TEMPLATES[0].make();
    return { ...t, clr: { chair: null, back: 0, left: 0, right: 0 } };
  };
  const [name, setName] = useState(editT?.name ?? 'Escritorio propio');
  const [color, setColor] = useState(editT?.color ?? '#7a8fb5');
  const [doc, setDocRaw] = useState<Doc>(initial);
  const [past, setPast] = useState<Doc[]>([]);
  const [future, setFuture] = useState<Doc[]>([]);
  const [snapOn, setSnapOn] = useState(true);
  const [grid, setGrid] = useState(50);
  const [mode, setMode] = useState<'edit' | 'seat'>('edit');
  const [sel, setSel] = useState<{ kind: 'part' | 'seat'; i: number } | null>(null);
  const [drag, setDrag] = useState<{ kind: 'move' | 'resize' | 'seat'; i: number; h?: string; a: Pt; orig: Part; base: Doc } | null>(null);
  const [cursor, setCursor] = useState<Pt | null>(null);
  const svg = useRef<SVGSVGElement>(null);

  const step = snapOn ? grid : 0;
  const sn = useCallback((v: number) => (step ? Math.round(v / step) * step : Math.round(v)), [step]);
  /** Cambio con historial (deshacer). `commit=false` para cambios en vivo durante un arrastre. */
  const setDoc = (d: Doc, commit = true, base?: Doc) => {
    if (commit) { setPast((p) => [...p.slice(-60), base ?? doc]); setFuture([]); }
    setDocRaw(d);
  };
  const undo = () => { if (!past.length) return; setFuture((f) => [doc, ...f]); setDocRaw(past[past.length - 1]); setPast((p) => p.slice(0, -1)); setSel(null); };
  const redo = () => { if (!future.length) return; setPast((p) => [...p, doc]); setDocRaw(future[0]); setFuture((f) => f.slice(1)); setSel(null); };

  const outline = useMemo(() => unionOutline(doc.parts), [doc.parts]);
  const bb = bbox(doc.parts.flatMap(partPts));
  const seats = doc.seats;
  const clr = doc.clr;
  const sides = { top: seats.some((s) => s.facing.y < -0.5), bottom: seats.some((s) => s.facing.y > 0.5), left: seats.some((s) => s.facing.x > 0.5), right: seats.some((s) => s.facing.x < -0.5) };
  const chairClear = clr.chair ?? e1;

  // vista
  const pad = Math.max(900, chairClear + 300, clr.left + 300, clr.right + 300, clr.back + 300);
  const view = { x0: bb.x0 - pad, y0: bb.y0 - pad, w: bb.w + 2 * pad, h: bb.h + 2 * pad };
  const W = 680, H = 460;
  const k = Math.min(W / view.w, H / view.h);
  const ox = (W - view.w * k) / 2, oy = (H - view.h * k) / 2;
  const X = (x: number) => ox + (x - view.x0) * k;
  const Y = (y: number) => oy + (view.y0 + view.h - y) * k; // +y hacia arriba
  const toLocal = (e: React.MouseEvent | MouseEvent): Pt => {
    const r = svg.current!.getBoundingClientRect();
    const sx = ((e.clientX - r.left) / r.width) * W, sy = ((e.clientY - r.top) / r.height) * H;
    return { x: view.x0 + (sx - ox) / k, y: view.y0 + view.h - (sy - oy) / k };
  };
  const ptsD = (ps: Pt[]) => ps.map((p, i) => `${i ? 'L' : 'M'}${X(p.x).toFixed(1)} ${Y(p.y).toFixed(1)}`).join('') + 'Z';
  // la vista no se reacomoda mientras se arrastra (si no, la pieza «huye» del mouse)
  const frozen = useRef<{ view: typeof view; k: number; ox: number; oy: number } | null>(null);
  if (!drag) frozen.current = null;
  else if (!frozen.current) frozen.current = { view, k, ox, oy };
  const fv = frozen.current;
  const Xf = fv ? (x: number) => fv.ox + (x - fv.view.x0) * fv.k : X;
  const Yf = fv ? (y: number) => fv.oy + (fv.view.y0 + fv.view.h - y) * fv.k : Y;
  const toLocalF = (e: React.MouseEvent): Pt => {
    if (!fv) return toLocal(e);
    const r = svg.current!.getBoundingClientRect();
    const sx = ((e.clientX - r.left) / r.width) * W, sy = ((e.clientY - r.top) / r.height) * H;
    return { x: fv.view.x0 + (sx - fv.ox) / fv.k, y: fv.view.y0 + fv.view.h - (sy - fv.oy) / fv.k };
  };
  const D = (ps: Pt[]) => ps.map((p, i) => `${i ? 'L' : 'M'}${Xf(p.x).toFixed(1)} ${Yf(p.y).toFixed(1)}`).join('') + 'Z';
  void ptsD;

  // sillas en un extremo: reservar su espacio en la separación de ese lado
  useEffect(() => {
    const l = sides.left ? Math.max(clr.left, chairClear) : clr.left;
    const r = sides.right ? Math.max(clr.right, chairClear) : clr.right;
    if (l !== clr.left || r !== clr.right) setDocRaw({ ...doc, clr: { ...clr, left: l, right: r } });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sides.left, sides.right]);

  // teclado: Supr, Ctrl+Z / Ctrl+Y, Esc
  useEffect(() => {
    const kd = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === 'INPUT' || tag === 'SELECT') return;
      const mod = e.ctrlKey || e.metaKey;
      if (mod && e.key.toLowerCase() === 'z' && !e.shiftKey) { e.preventDefault(); undo(); }
      else if (mod && (e.key.toLowerCase() === 'y' || (e.key.toLowerCase() === 'z' && e.shiftKey))) { e.preventDefault(); redo(); }
      else if ((e.key === 'Delete' || e.key === 'Backspace') && sel) {
        if (sel.kind === 'seat') setDoc({ ...doc, seats: doc.seats.filter((_, i) => i !== sel.i) });
        else if (doc.parts.length > 1) setDoc({ ...doc, parts: doc.parts.filter((_, i) => i !== sel.i) });
        setSel(null);
      } else if (e.key === 'Escape') { if (mode !== 'edit') setMode('edit'); else st.set({ studio: null }); }
      else return;
      e.stopPropagation();
    };
    window.addEventListener('keydown', kd, true);
    return () => window.removeEventListener('keydown', kd, true);
  });

  const reattach = (d: Doc): Doc => {
    const o = unionOutline(d.parts);
    return o ? { ...d, seats: d.seats.map((s) => seatOnEdge(o, s, 0) ?? s) } : d;
  };

  const onDown = (e: React.MouseEvent) => {
    const p = toLocal(e);
    if (mode === 'seat') {
      if (outline) {
        const s = seatOnEdge(outline, p, step);
        if (s) { setDoc({ ...doc, seats: [...doc.seats, s] }); setSel({ kind: 'seat', i: doc.seats.length }); }
      }
      setMode('edit');
      return;
    }
    setSel(null);
  };
  const onMove = (e: React.MouseEvent) => {
    const p = toLocalF(e);
    setCursor(p);
    if (!drag) return;
    const dx = p.x - drag.a.x, dy = p.y - drag.a.y;
    const o = drag.orig;
    if (drag.kind === 'move') {
      const np = { ...o, x: sn(o.x + dx), y: sn(o.y + dy) };
      setDoc({ ...drag.base, parts: drag.base.parts.map((q, i) => (i === drag.i ? np : q)) }, false);
    } else if (drag.kind === 'resize') {
      let { x, y, w, h } = o;
      const hh = drag.h!;
      const min = Math.max(step, 100);
      if (hh.includes('e')) w = Math.max(min, sn(o.x + o.w + dx) - o.x);
      if (hh.includes('w')) { const nx = Math.min(sn(o.x + dx), o.x + o.w - min); w = o.x + o.w - nx; x = nx; }
      if (hh.includes('n')) h = Math.max(min, sn(o.y + o.h + dy) - o.y);
      if (hh.includes('s')) { const ny = Math.min(sn(o.y + dy), o.y + o.h - min); h = o.y + o.h - ny; y = ny; }
      setDoc({ ...drag.base, parts: drag.base.parts.map((q, i) => (i === drag.i ? { x, y, w, h } : q)) }, false);
    } else if (drag.kind === 'seat' && outline) {
      const s = seatOnEdge(outline, p, step);
      if (s) setDoc({ ...drag.base, seats: drag.base.seats.map((q, i) => (i === drag.i ? s : q)) }, false);
    }
  };
  const onUp = () => {
    if (!drag) return;
    const changed = JSON.stringify(drag.base) !== JSON.stringify(doc);
    if (changed) setDoc(drag.kind === 'seat' ? doc : reattach(doc), true, drag.base);
    setDrag(null);
  };

  const addPart = () => {
    // a la derecha del conjunto, pegado
    const np: Part = { x: sn(bb.x1), y: sn(bb.y1 - 600), w: 600, h: 600 };
    setDoc({ ...doc, parts: [...doc.parts, np] });
    setSel({ kind: 'part', i: doc.parts.length });
  };
  const useTemplate = (id: string) => {
    const t = TEMPLATES.find((x) => x.id === id)!.make();
    setDoc({ ...doc, ...t });
    setSel(null);
  };
  const restart = () => { setDoc({ ...TEMPLATES[0].make(), clr: { chair: null, back: 0, left: 0, right: 0 } }); setSel(null); setMode('edit'); };
  const setClr = (cl: Partial<Clear>) => setDoc({ ...doc, clr: { ...clr, ...cl } });
  const selPart = sel?.kind === 'part' ? doc.parts[sel.i] : null;
  const updPart = (f: (p: Part) => Part) => { if (sel?.kind !== 'part') return; setDoc(reattach({ ...doc, parts: doc.parts.map((q, i) => (i === sel.i ? f(q) : q)) })); };

  // zonas doradas (sobre el rectángulo que contiene la forma, como las usa el optimizador)
  const zones = useMemo(() => {
    const z: { d: string; label: string; x: number; y: number }[] = [];
    const R = (x0: number, y0: number, x1: number, y1: number) => `M${Xf(x0)} ${Yf(y0)}L${Xf(x1)} ${Yf(y0)}L${Xf(x1)} ${Yf(y1)}L${Xf(x0)} ${Yf(y1)}Z`;
    const m2 = (v: number) => (v / 1000).toFixed(2);
    if (sides.top || (!sides.bottom && !sides.left && !sides.right)) z.push({ d: R(bb.x0, bb.y1, bb.x1, bb.y1 + chairClear), label: `silla ${m2(chairClear)}`, x: bb.cx, y: bb.y1 + chairClear / 2 });
    if (sides.bottom) z.push({ d: R(bb.x0, bb.y0 - chairClear, bb.x1, bb.y0), label: `silla ${m2(chairClear)}`, x: bb.cx, y: bb.y0 - chairClear / 2 });
    else if (clr.back > 0) z.push({ d: R(bb.x0, bb.y0 - clr.back, bb.x1, bb.y0), label: `atrás ${m2(clr.back)}`, x: bb.cx, y: bb.y0 - clr.back / 2 });
    if (clr.left > 0) z.push({ d: R(bb.x0 - clr.left, bb.y0, bb.x0, bb.y1), label: `izq. ${m2(clr.left)}`, x: bb.x0 - clr.left / 2, y: bb.cy });
    if (clr.right > 0) z.push({ d: R(bb.x1, bb.y0, bb.x1 + clr.right, bb.y1), label: `der. ${m2(clr.right)}`, x: bb.x1 + clr.right / 2, y: bb.cy });
    return z;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [doc, chairClear, k, drag]);

  const problems: string[] = [];
  if (!outline) problems.push('Las piezas deben tocarse para formar un solo escritorio.');
  if (seats.length === 0) problems.push('Agrega al menos una silla.');

  const save = () => {
    if (!c || problems.length || !outline) return;
    const cx = bb.cx, cy = bb.cy;
    let body = outline.map((p) => ({ x: p.x - cx, y: p.y - cy }));
    let ss = seats.map((s) => ({ x: s.x - cx, y: s.y - cy, facing: { ...s.facing } }));
    let parts = doc.parts.map((p) => ({ ...p, x: p.x - cx, y: p.y - cy }));
    let back = clr.back;
    // convención: el lado principal de las sillas arriba (+y)
    if (!sides.top && sides.bottom) {
      body = body.map((p) => ({ x: p.x, y: -p.y })).reverse();
      ss = ss.map((s) => ({ x: s.x, y: -s.y, facing: { x: s.facing.x, y: -s.facing.y } }));
      parts = parts.map((p) => ({ ...p, y: -p.y - p.h }));
      back = 0;
    }
    const two = sides.top && sides.bottom;
    const t: DeskType & { parts: Part[] } = {
      id: editT?.id ?? uid('custom'), name: name.trim() || 'Escritorio propio', color, shape: 'custom',
      seatWidth: Math.round(bb.w), depth: Math.round(bb.h), sides: two ? 2 : 1, perSide: 1, flexible: false, minPerSide: 1, maxPerSide: 1,
      returnLength: 0, returnDepth: 0,
      clearChair: clr.chair, clearBack: two ? 0 : back, clearLeft: clr.left, clearRight: clr.right,
      body, seats: ss, parts,
    };
    st.snapshot();
    st.updateConfig((cc) => {
      const i = cc.catalog.findIndex((x) => x.id === t.id);
      if (i >= 0) cc.catalog[i] = t; else cc.catalog.push(t);
    });
    st.set({ studio: null });
    st.say(editT ? `«${t.name}» actualizado.` : `«${t.name}» agregado al catálogo (${ss.length} puesto${ss.length > 1 ? 's' : ''}). Pon su cantidad en Escritorios o colócalo con el botón +.`);
  };

  const handles = (r: Part, i: number) => {
    const hs: [string, number, number, string][] = [
      ['sw', r.x, r.y, 'nesw-resize'], ['se', r.x + r.w, r.y, 'nwse-resize'], ['ne', r.x + r.w, r.y + r.h, 'nesw-resize'], ['nw', r.x, r.y + r.h, 'nwse-resize'],
      ['s', r.x + r.w / 2, r.y, 'ns-resize'], ['n', r.x + r.w / 2, r.y + r.h, 'ns-resize'], ['w', r.x, r.y + r.h / 2, 'ew-resize'], ['e', r.x + r.w, r.y + r.h / 2, 'ew-resize'],
    ];
    return hs.map(([h, x, y, cur]) => (
      <rect key={h} className={'v-handle' + (h.length === 1 ? ' mid' : '')} x={Xf(x) - 5} y={Yf(y) - 5} width={10} height={10} rx={h.length === 1 ? 5 : 2} style={{ cursor: cur }}
        onMouseDown={(e) => { e.stopPropagation(); setDrag({ kind: 'resize', i, h, a: toLocalF(e), orig: r, base: doc }); }} />
    ));
  };
  const gstep = Math.max(grid, 50);

  return (
    <div className="studio-back" role="dialog" aria-modal="true" aria-label="Studio de escritorios" onMouseDown={(e) => { if (e.target === e.currentTarget) st.set({ studio: null }); }}>
      <div className="studio">
        <header>
          <h2>{editT ? 'Studio · editar escritorio' : 'Studio de escritorios'}</h2>
          <div className="studio-hist">
            <button className="ghost small" disabled={!past.length} onClick={undo} title="Deshacer (Ctrl+Z)">↶ Deshacer</button>
            <button className="ghost small" disabled={!future.length} onClick={redo} title="Rehacer (Ctrl+Y)">↷ Rehacer</button>
            <button className="ghost small" onClick={restart} title="Volver a un escritorio recto con una silla">Empezar de nuevo</button>
            <button className="ghost small" onClick={() => st.set({ studio: null })} aria-label="Cerrar">✕</button>
          </div>
        </header>
        <div className="studio-body">
          <div className="studio-side">
            <label className="field">Nombre<input value={name} onChange={(e) => setName(e.target.value)} /></label>
            <label className="field inline">Color<input type="color" value={color} onChange={(e) => setColor(e.target.value)} /></label>

            <h3>1 · Forma con rectángulos</h3>
            <div className="studio-templates">
              {TEMPLATES.map((t) => <button key={t.id} className="small" onClick={() => useTemplate(t.id)}>{t.name}</button>)}
            </div>
            <div className="row wrap">
              <button className="small" onClick={addPart}>+ Rectángulo</button>
              {selPart && doc.parts.length > 1 && <button className="small danger" onClick={() => { setDoc(reattach({ ...doc, parts: doc.parts.filter((_, i) => i !== sel!.i) })); setSel(null); }}>Quitar rectángulo</button>}
            </div>
            {selPart ? (
              <div className="dims">
                <label>Ancho<Num value={selPart.w} unit="mm" min={100} onChange={(v) => updPart((p) => ({ ...p, w: Math.max(100, v) }))} /></label>
                <label>Fondo<Num value={selPart.h} unit="mm" min={100} onChange={(v) => updPart((p) => ({ ...p, h: Math.max(100, v) }))} /></label>
              </div>
            ) : <p className="hint">Arrastra un rectángulo para moverlo y sus manijas para cambiar el tamaño. Haz clic en uno para escribir sus medidas.</p>}

            <h3>2 · Sillas</h3>
            <div className="row wrap">
              <button className={'small' + (mode === 'seat' ? ' active-tool' : '')} disabled={!outline} onClick={() => setMode(mode === 'seat' ? 'edit' : 'seat')}>{mode === 'seat' ? 'Clic junto a un borde…' : '+ Silla'}</button>
              {sel?.kind === 'seat' && <button className="small danger" onClick={() => { setDoc({ ...doc, seats: doc.seats.filter((_, i) => i !== sel.i) }); setSel(null); }}>Quitar silla</button>}
            </div>
            <p className="hint">La silla se pega al borde más cercano y la persona mira hacia el escritorio. Arrástrala para moverla.</p>

            <h3>3 · Separaciones</h3>
            <div className="dims clears">
              <label title="Vacío = el de «Espacios»">Lado de la silla<Num value={chairClear} unit="mm" min={0} onChange={(v) => setClr({ chair: v })} /></label>
              {!(sides.top && sides.bottom) && <label>Atrás<Num value={clr.back} unit="mm" min={0} onChange={(v) => setClr({ back: v })} /></label>}
              <label>Izquierda<Num value={clr.left} unit="mm" min={0} onChange={(v) => setClr({ left: v })} /></label>
              <label>Derecha<Num value={clr.right} unit="mm" min={0} onChange={(v) => setClr({ right: v })} /></label>
            </div>
          </div>
          <div className="studio-canvas">
            <div className="studio-bar">
              <label className="inline">Snap <Switch small on={snapOn} label="Snap a la cuadrícula" onChange={setSnapOn} /></label>
              <select value={grid} aria-label="Tamaño de la cuadrícula" onChange={(e) => setGrid(parseInt(e.target.value))}>
                {[10, 25, 50, 100, 200].map((g) => <option key={g} value={g}>{g >= 100 ? `${g / 1000} m` : `${g / 10} cm`}</option>)}
              </select>
              <span className="spacer" />
              <span>{seats.length} puesto{seats.length === 1 ? '' : 's'} · {(bb.w / 1000).toFixed(2)} × {(bb.h / 1000).toFixed(2)} m</span>
            </div>
            <svg ref={svg} viewBox={`0 0 ${W} ${H}`} onMouseDown={onDown} onMouseMove={onMove} onMouseUp={onUp} onMouseLeave={onUp}
              className={'studio-svg mode-' + mode}>
              <defs>
                <pattern id="sgrid" width={gstep * (fv?.k ?? k)} height={gstep * (fv?.k ?? k)} patternUnits="userSpaceOnUse" x={Xf(0)} y={Yf(0)}>
                  <path d={`M${gstep * (fv?.k ?? k)} 0L0 0 0 ${gstep * (fv?.k ?? k)}`} className="sgrid" />
                </pattern>
              </defs>
              <rect width={W} height={H} fill={snapOn ? 'url(#sgrid)' : 'none'} />
              {zones.map((z, i) => <path key={'z' + i} className="clear-zone" d={z.d} />)}
              {outline && <path className="desk-shape studio-desk" d={D(outline)} fill={color} />}
              {doc.parts.map((r, i) => (
                <path key={'p' + i} className={'studio-part' + (sel?.kind === 'part' && sel.i === i ? ' on' : '') + (outline ? '' : ' loose')} d={D(partPts(r))}
                  fill={outline ? 'transparent' : color}
                  onMouseDown={(e) => { if (mode !== 'edit') return; e.stopPropagation(); setSel({ kind: 'part', i }); setDrag({ kind: 'move', i, a: toLocalF(e), orig: r, base: doc }); }} />
              ))}
              {seats.map((s, i) => {
                const sd = { x: -s.facing.y, y: s.facing.x };
                const c4 = [[-1, -1], [1, -1], [1, 1], [-1, 1]].map(([a, b]) => ({ x: s.x + (sd.x * a * CW) / 2 + (s.facing.x * b * CHAIR) / 2, y: s.y + (sd.y * a * CW) / 2 + (s.facing.y * b * CHAIR) / 2 }));
                const tip = { x: s.x + s.facing.x * CHAIR * 0.32, y: s.y + s.facing.y * CHAIR * 0.32 };
                return (
                  <g key={'s' + i} className={'studio-seat' + (sel?.kind === 'seat' && sel.i === i ? ' on' : '')}
                    onMouseDown={(e) => { if (mode !== 'edit') return; e.stopPropagation(); setSel({ kind: 'seat', i }); setDrag({ kind: 'seat', i, a: toLocalF(e), orig: doc.parts[0], base: doc }); }}>
                    <path d={D(c4)} />
                    <line x1={Xf(s.x)} y1={Yf(s.y)} x2={Xf(tip.x)} y2={Yf(tip.y)} />
                  </g>
                );
              })}
              {sel?.kind === 'part' && doc.parts[sel.i] && mode === 'edit' && handles(doc.parts[sel.i], sel.i)}
              {zones.map((z, i) => <text key={'zl' + i} className="studio-zlabel" x={Xf(z.x)} y={Yf(z.y) + 4}>{z.label}</text>)}
              <text className="studio-dim" x={Xf(bb.cx)} y={Yf(bb.y0) + 16}>{(bb.w / 1000).toFixed(2)} m</text>
              <text className="studio-dim" x={Xf(bb.x0) - 8} y={Yf(bb.cy)} textAnchor="end">{(bb.h / 1000).toFixed(2)} m</text>
              {selPart && <text className="studio-dim sel" x={Xf(selPart.x + selPart.w / 2)} y={Yf(selPart.y + selPart.h / 2) + 4}>{(selPart.w / 1000).toFixed(2)} × {(selPart.h / 1000).toFixed(2)}</text>}
              {mode === 'seat' && cursor && outline && (() => {
                const s = seatOnEdge(outline, cursor, step);
                return s ? <circle className="studio-ghost" cx={Xf(s.x)} cy={Yf(s.y)} r={CW * 0.45 * (fv?.k ?? k)} /> : null;
              })()}
            </svg>
            <p className="hint">Ctrl+Z deshace, Ctrl+Y rehace, Supr quita lo seleccionado. El optimizador acomoda el rectángulo que contiene la forma, con las separaciones doradas alrededor.</p>
          </div>
        </div>
        <footer>
          {problems.map((p) => <span key={p} className="studio-warn">{p}</span>)}
          <span className="spacer" />
          <button className="ghost" onClick={() => st.set({ studio: null })}>Cancelar</button>
          <button className="primary" disabled={problems.length > 0} onClick={save}>{editT ? 'Guardar cambios' : 'Agregar al catálogo'}</button>
        </footer>
      </div>
    </div>
  );
}
