// Estudio de escritorios: dibujar una forma sencilla, poner las sillas y decidir las separaciones.
// El resultado es un tipo de escritorio más del catálogo (forma «propia»).
import { useEffect, useMemo, useRef, useState } from 'react';
import { ruleVal } from '../clear';
import { uid, useApp } from '../store';
import type { CustomSeat, DeskType, Pt } from '../types';
import { Num } from './ui';

const SNAP = 50; // mm
const CHAIR = 500; // fondo de la silla (dibujo)
const CW = 480; // ancho de la silla (dibujo)

type Mode = 'edit' | 'seat' | 'draw';

const rect = (w: number, d: number): Pt[] => [{ x: -w / 2, y: -d / 2 }, { x: w / 2, y: -d / 2 }, { x: w / 2, y: d / 2 }, { x: -w / 2, y: d / 2 }];
/** L: barra arriba (fondo `bd`) y retorno a la izquierda (largo `rl`, fondo `rd`). */
const lShape = (w: number, bd: number, rl: number, rd: number): Pt[] => [
  { x: 0, y: rl }, { x: w, y: rl }, { x: w, y: rl - bd }, { x: rd, y: rl - bd }, { x: rd, y: 0 }, { x: 0, y: 0 },
];
/** U: barra arriba y dos brazos hacia abajo. */
const uShape = (w: number, bd: number, arm: number, ad: number): Pt[] => [
  { x: 0, y: arm }, { x: w, y: arm }, { x: w, y: 0 }, { x: w - ad, y: 0 }, { x: w - ad, y: arm - bd }, { x: ad, y: arm - bd }, { x: ad, y: 0 }, { x: 0, y: 0 },
];

function bbox(ps: Pt[]) {
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const p of ps) { x0 = Math.min(x0, p.x); y0 = Math.min(y0, p.y); x1 = Math.max(x1, p.x); y1 = Math.max(y1, p.y); }
  return { x0, y0, x1, y1, w: x1 - x0, h: y1 - y0, cx: (x0 + x1) / 2, cy: (y0 + y1) / 2 };
}
const area = (poly: Pt[]) => poly.reduce((s, p, i) => { const q = poly[(i + 1) % poly.length]; return s + p.x * q.y - q.x * p.y; }, 0) / 2;
const snap = (v: number) => Math.round(v / SNAP) * SNAP;

/** Silla más cercana al borde: la persona mira hacia el escritorio, la silla queda afuera. */
function seatOnEdge(poly: Pt[], p: Pt): CustomSeat | null {
  const ccw = area(poly) > 0;
  let best: { d: number; s: CustomSeat } | null = null;
  for (let i = 0; i < poly.length; i++) {
    const a = poly[i], b = poly[(i + 1) % poly.length];
    const ex = b.x - a.x, ey = b.y - a.y, l = Math.hypot(ex, ey);
    if (l < CW * 0.6) continue;
    const ux = ex / l, uy = ey / l;
    // normal hacia afuera
    const out = ccw ? { x: uy, y: -ux } : { x: -uy, y: ux };
    let t = ((p.x - a.x) * ux + (p.y - a.y) * uy);
    t = Math.max(CW / 2, Math.min(l - CW / 2, snap(t)));
    const q = { x: a.x + ux * t, y: a.y + uy * t };
    const d = Math.hypot(p.x - q.x, p.y - q.y);
    if (!best || d < best.d) best = { d, s: { x: q.x + out.x * CHAIR / 2, y: q.y + out.y * CHAIR / 2, facing: { x: -out.x, y: -out.y } } };
  }
  return best?.s ?? null;
}

export default function DeskStudio() {
  const st = useApp();
  const c = st.config;
  const editT = st.studio?.editId ? c?.catalog.find((t) => t.id === st.studio!.editId) : undefined;
  const e1 = c ? ruleVal(c, 'E1', 'muro') ?? 900 : 900;

  const [name, setName] = useState(editT?.name ?? 'Escritorio propio');
  const [color, setColor] = useState(editT?.color ?? '#7a8fb5');
  const [poly, setPoly] = useState<Pt[]>(() => editT?.body?.length ? editT.body.map((p) => ({ ...p })) : rect(1400, 700));
  const [seats, setSeats] = useState<CustomSeat[]>(() => editT?.seats?.length ? editT.seats.map((s) => ({ ...s, facing: { ...s.facing } })) : [{ x: 0, y: 350 + CHAIR / 2, facing: { x: 0, y: -1 } }]);
  const [clr, setClr] = useState({ chair: editT?.clearChair ?? null as number | null, back: editT?.clearBack ?? 0, left: editT?.clearLeft ?? 0, right: editT?.clearRight ?? 0 });
  const [mode, setMode] = useState<Mode>('edit');
  const [draft, setDraft] = useState<Pt[]>([]);
  const [selSeat, setSelSeat] = useState<number | null>(null);
  const [selV, setSelV] = useState<number | null>(null);
  const [drag, setDrag] = useState<{ kind: 'v' | 's'; i: number } | null>(null);
  const [cursor, setCursor] = useState<Pt | null>(null);
  const [preset, setPreset] = useState({ w: 1400, d: 700, a: 1200, ad: 600 });
  const svg = useRef<SVGSVGElement>(null);

  const bb = bbox(poly);
  const sides = { top: seats.some((s) => s.facing.y < -0.5), bottom: seats.some((s) => s.facing.y > 0.5), left: seats.some((s) => s.facing.x > 0.5), right: seats.some((s) => s.facing.x < -0.5) };
  const chairClear = clr.chair ?? e1;

  // vista: el escritorio con margen para sillas y separaciones
  const pad = Math.max(900, chairClear + 250, clr.left + 300, clr.right + 300, clr.back + 300);
  const view = { x0: bb.x0 - pad, y0: bb.y0 - pad, w: bb.w + 2 * pad, h: bb.h + 2 * pad };
  const W = 640, H = 420;
  const k = Math.min(W / view.w, H / view.h);
  const ox = (W - view.w * k) / 2, oy = (H - view.h * k) / 2;
  const X = (x: number) => ox + (x - view.x0) * k;
  const Y = (y: number) => oy + (view.y0 + view.h - y) * k; // +y hacia arriba
  const toLocal = (e: React.MouseEvent): Pt => {
    const r = svg.current!.getBoundingClientRect();
    const sx = ((e.clientX - r.left) / r.width) * W, sy = ((e.clientY - r.top) / r.height) * H;
    return { x: view.x0 + (sx - ox) / k, y: view.y0 + view.h - (sy - oy) / k };
  };
  const ptsD = (ps: Pt[], close = true) => ps.map((p, i) => `${i ? 'L' : 'M'}${X(p.x).toFixed(1)} ${Y(p.y).toFixed(1)}`).join('') + (close ? 'Z' : '');

  // al poner una silla en un extremo, dejarle su espacio en la separación de ese lado
  useEffect(() => {
    setClr((c0) => ({
      ...c0,
      left: sides.left ? Math.max(c0.left, chairClear) : c0.left,
      right: sides.right ? Math.max(c0.right, chairClear) : c0.right,
    }));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sides.left, sides.right]);

  // borrar con Supr
  useEffect(() => {
    const kd = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement)?.tagName === 'INPUT') return;
      if (e.key === 'Delete' || e.key === 'Backspace') {
        if (selSeat != null) { setSeats((s) => s.filter((_, i) => i !== selSeat)); setSelSeat(null); }
        else if (selV != null && poly.length > 3) { setPoly((p) => p.filter((_, i) => i !== selV)); setSelV(null); }
      }
      if (e.key === 'Escape') { setMode('edit'); setDraft([]); }
      e.stopPropagation();
    };
    window.addEventListener('keydown', kd, true);
    return () => window.removeEventListener('keydown', kd, true);
  }, [selSeat, selV, poly.length]);

  const onDown = (e: React.MouseEvent) => {
    const p = toLocal(e);
    if (mode === 'draw') {
      const q = { x: snap(p.x), y: snap(p.y) };
      if (draft.length >= 3 && Math.hypot(q.x - draft[0].x, q.y - draft[0].y) < 120) {
        setPoly(draft); setDraft([]); setMode('edit');
        setSeats((ss) => ss.map((s) => seatOnEdge(draft, s) ?? s));
        return;
      }
      setDraft([...draft, q]);
      return;
    }
    if (mode === 'seat') {
      const s = seatOnEdge(poly, p);
      if (s) { setSeats([...seats, s]); setSelSeat(seats.length); }
      setMode('edit');
      return;
    }
    setSelSeat(null); setSelV(null);
  };
  const onMove = (e: React.MouseEvent) => {
    const p = toLocal(e);
    setCursor(p);
    if (!drag) return;
    if (drag.kind === 'v') setPoly(poly.map((q, i) => (i === drag.i ? { x: snap(p.x), y: snap(p.y) } : q)));
    else { const s = seatOnEdge(poly, p); if (s) setSeats(seats.map((q, i) => (i === drag.i ? s : q))); }
  };
  const onUp = () => {
    if (drag?.kind === 'v') setSeats((ss) => ss.map((s) => seatOnEdge(poly, s) ?? s));
    setDrag(null);
  };
  // doble clic sobre un borde: agrega un vértice
  const addVertex = (i: number, e: React.MouseEvent) => {
    e.stopPropagation();
    const p = toLocal(e);
    const np = [...poly];
    np.splice(i + 1, 0, { x: snap(p.x), y: snap(p.y) });
    setPoly(np);
  };

  // zonas doradas de separación (sobre el rectángulo que contiene la forma, como las usa el optimizador)
  const zones = useMemo(() => {
    const z: { d: string; label: string; x: number; y: number }[] = [];
    const R = (x0: number, y0: number, x1: number, y1: number) => `M${X(x0)} ${Y(y0)}L${X(x1)} ${Y(y0)}L${X(x1)} ${Y(y1)}L${X(x0)} ${Y(y1)}Z`;
    const m2 = (v: number) => (v / 1000).toFixed(2);
    if (sides.top || (!sides.bottom && !sides.left && !sides.right)) z.push({ d: R(bb.x0, bb.y1, bb.x1, bb.y1 + chairClear), label: `silla ${m2(chairClear)}`, x: bb.cx, y: bb.y1 + chairClear / 2 });
    if (sides.bottom) z.push({ d: R(bb.x0, bb.y0 - chairClear, bb.x1, bb.y0), label: `silla ${m2(chairClear)}`, x: bb.cx, y: bb.y0 - chairClear / 2 });
    else if (clr.back > 0) z.push({ d: R(bb.x0, bb.y0 - clr.back, bb.x1, bb.y0), label: `atrás ${m2(clr.back)}`, x: bb.cx, y: bb.y0 - clr.back / 2 });
    if (clr.left > 0) z.push({ d: R(bb.x0 - clr.left, bb.y0, bb.x0, bb.y1), label: `izq. ${m2(clr.left)}`, x: bb.x0 - clr.left / 2, y: bb.cy });
    if (clr.right > 0) z.push({ d: R(bb.x1, bb.y0, bb.x1 + clr.right, bb.y1), label: `der. ${m2(clr.right)}`, x: bb.x1 + clr.right / 2, y: bb.cy });
    return z;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [poly, seats, clr, chairClear, k]);

  const problems: string[] = [];
  if (seats.length === 0) problems.push('Agrega al menos una silla.');
  if (bb.w < 300 || bb.h < 300) problems.push('La forma es demasiado chica.');

  const save = () => {
    if (!c || problems.length) return;
    // origen en el centro del rectángulo; el lado principal de las sillas arriba (+y)
    let body = poly.map((p) => ({ x: p.x - bb.cx, y: p.y - bb.cy }));
    let ss = seats.map((s) => ({ x: s.x - bb.cx, y: s.y - bb.cy, facing: { ...s.facing } }));
    let back = clr.back;
    if (!sides.top && sides.bottom) {
      body = body.map((p) => ({ x: p.x, y: -p.y })).reverse();
      ss = ss.map((s) => ({ x: s.x, y: -s.y, facing: { x: s.facing.x, y: -s.facing.y } }));
      back = 0;
    }
    const two = (sides.top || !sides.bottom) && sides.bottom && sides.top;
    const t: DeskType = {
      id: editT?.id ?? uid('custom'), name: name.trim() || 'Escritorio propio', color, shape: 'custom',
      seatWidth: Math.round(bb.w), depth: Math.round(bb.h), sides: two ? 2 : 1, perSide: 1, flexible: false, minPerSide: 1, maxPerSide: 1,
      returnLength: 0, returnDepth: 0,
      clearChair: clr.chair, clearBack: two ? 0 : back, clearLeft: clr.left, clearRight: clr.right,
      body, seats: ss,
    };
    st.snapshot();
    st.updateConfig((cc) => {
      const i = cc.catalog.findIndex((x) => x.id === t.id);
      if (i >= 0) cc.catalog[i] = t; else cc.catalog.push(t);
    });
    st.set({ studio: null });
    st.say(editT ? `«${t.name}» actualizado.` : `«${t.name}» agregado al catálogo (${ss.length} puesto${ss.length > 1 ? 's' : ''}). Pon su cantidad en Escritorios o colócalo con «+ Agregar».`);
  };

  return (
    <div className="studio-back" role="dialog" aria-modal="true" aria-label="Estudio de escritorios" onMouseDown={(e) => { if (e.target === e.currentTarget) st.set({ studio: null }); }}>
      <div className="studio">
        <header>
          <h2>{editT ? 'Editar escritorio' : 'Estudio de escritorios'}</h2>
          <button className="ghost small" onClick={() => st.set({ studio: null })} aria-label="Cerrar">✕</button>
        </header>
        <div className="studio-body">
          <div className="studio-side">
            <label className="field">Nombre<input value={name} onChange={(e) => setName(e.target.value)} /></label>
            <label className="field inline">Color<input type="color" value={color} onChange={(e) => setColor(e.target.value)} /></label>

            <h3>1 · Forma</h3>
            <div className="dims">
              <label>Ancho<Num value={preset.w} unit="mm" onChange={(v) => setPreset({ ...preset, w: v })} /></label>
              <label>Fondo<Num value={preset.d} unit="mm" onChange={(v) => setPreset({ ...preset, d: v })} /></label>
              <label title="Para L y U">Largo del retorno<Num value={preset.a} unit="mm" onChange={(v) => setPreset({ ...preset, a: v })} /></label>
              <label title="Para L y U">Fondo del retorno<Num value={preset.ad} unit="mm" onChange={(v) => setPreset({ ...preset, ad: v })} /></label>
            </div>
            <div className="row wrap">
              <button className="small" onClick={() => { setPoly(rect(preset.w, preset.d)); setSeats([{ x: 0, y: preset.d / 2 + CHAIR / 2, facing: { x: 0, y: -1 } }]); }}>Rectángulo</button>
              <button className="small" onClick={() => { const p = lShape(preset.w, preset.d, preset.a, preset.ad); setPoly(p); const q = seatOnEdge(p, { x: preset.ad + (preset.w - preset.ad) / 2, y: preset.a - preset.d - 300 }); setSeats(q ? [q] : []); }}>En L</button>
              <button className="small" onClick={() => { const p = uShape(preset.w, preset.d, preset.a, preset.ad); setPoly(p); const q = seatOnEdge(p, { x: preset.w / 2, y: preset.a - preset.d - 300 }); setSeats(q ? [q] : []); }}>En U</button>
              <button className={'small' + (mode === 'draw' ? ' active-tool' : '')} onClick={() => { setMode(mode === 'draw' ? 'edit' : 'draw'); setDraft([]); }}>Dibujar a mano</button>
            </div>
            <p className="hint">Arrastra las esquinas (de a 5 cm). Doble clic en un borde agrega una esquina; selecciónala y Supr la quita.</p>

            <h3>2 · Sillas</h3>
            <div className="row wrap">
              <button className={'small' + (mode === 'seat' ? ' active-tool' : '')} onClick={() => setMode(mode === 'seat' ? 'edit' : 'seat')}>{mode === 'seat' ? 'Clic junto a un borde…' : 'Agregar silla'}</button>
              {selSeat != null && <button className="small danger" onClick={() => { setSeats(seats.filter((_, i) => i !== selSeat)); setSelSeat(null); }}>Quitar silla</button>}
            </div>
            <p className="hint">La silla se pega al borde más cercano y la persona mira hacia el escritorio. Arrástrala para moverla por los bordes.</p>

            <h3>3 · Separaciones</h3>
            <div className="dims clears">
              <label title="Vacío = el de «Espacios»">Lado de la silla<Num value={chairClear} unit="mm" min={0} onChange={(v) => setClr({ ...clr, chair: v })} /></label>
              {!(sides.top && sides.bottom) && <label>Atrás<Num value={clr.back} unit="mm" min={0} onChange={(v) => setClr({ ...clr, back: v })} /></label>}
              <label>Izquierda<Num value={clr.left} unit="mm" min={0} onChange={(v) => setClr({ ...clr, left: v })} /></label>
              <label>Derecha<Num value={clr.right} unit="mm" min={0} onChange={(v) => setClr({ ...clr, right: v })} /></label>
            </div>
            <p className="hint">El optimizador acomoda el rectángulo que contiene la forma, con estas separaciones alrededor (en dorado).</p>
          </div>
          <div className="studio-canvas">
            <svg ref={svg} viewBox={`0 0 ${W} ${H}`} onMouseDown={onDown} onMouseMove={onMove} onMouseUp={onUp} onMouseLeave={onUp}
              className={'studio-svg mode-' + mode}>
              <defs>
                <pattern id="sgrid" width={100 * k} height={100 * k} patternUnits="userSpaceOnUse" x={X(0)} y={Y(0)}>
                  <path d={`M${100 * k} 0L0 0 0 ${100 * k}`} className="sgrid" />
                </pattern>
              </defs>
              <rect width={W} height={H} fill="url(#sgrid)" />
              {zones.map((z, i) => <path key={'z' + i} className="clear-zone" d={z.d} />)}
              <rect className="studio-bbox" x={X(bb.x0)} y={Y(bb.y1)} width={bb.w * k} height={bb.h * k} />
              <path className="desk-shape studio-desk" d={ptsD(poly)} fill={color} />
              {poly.map((a, i) => {
                const b = poly[(i + 1) % poly.length];
                return <line key={'e' + i} className="studio-edge" x1={X(a.x)} y1={Y(a.y)} x2={X(b.x)} y2={Y(b.y)} onDoubleClick={(e) => addVertex(i, e)} />;
              })}
              {seats.map((s, i) => {
                const sd = { x: -s.facing.y, y: s.facing.x };
                const c4 = [[-1, -1], [1, -1], [1, 1], [-1, 1]].map(([a, b]) => ({ x: s.x + sd.x * a * CW / 2 + s.facing.x * b * CHAIR / 2, y: s.y + sd.y * a * CW / 2 + s.facing.y * b * CHAIR / 2 }));
                const tip = { x: s.x + s.facing.x * CHAIR * 0.32, y: s.y + s.facing.y * CHAIR * 0.32 };
                return (
                  <g key={'s' + i} className={'studio-seat' + (selSeat === i ? ' on' : '')}
                    onMouseDown={(e) => { if (mode !== 'edit') return; e.stopPropagation(); setSelSeat(i); setSelV(null); setDrag({ kind: 's', i }); }}>
                    <path d={ptsD(c4)} />
                    <line x1={X(s.x)} y1={Y(s.y)} x2={X(tip.x)} y2={Y(tip.y)} />
                  </g>
                );
              })}
              {mode === 'edit' && poly.map((p, i) => (
                <rect key={'v' + i} className={'v-handle' + (selV === i ? ' on' : '')} x={X(p.x) - 5} y={Y(p.y) - 5} width={10} height={10} rx={2}
                  onMouseDown={(e) => { e.stopPropagation(); setSelV(i); setSelSeat(null); setDrag({ kind: 'v', i }); }} />
              ))}
              {zones.map((z, i) => <text key={'zl' + i} className="studio-zlabel" x={X(z.x)} y={Y(z.y) + 4}>{z.label}</text>)}
              <text className="studio-dim" x={X(bb.cx)} y={Y(bb.y0) + 16}>{(bb.w / 1000).toFixed(2)} m</text>
              <text className="studio-dim" x={X(bb.x0) - 8} y={Y(bb.cy)} textAnchor="end">{(bb.h / 1000).toFixed(2)} m</text>
              {mode === 'draw' && draft.length > 0 && (
                <path className="studio-draft" d={ptsD([...draft, ...(cursor ? [{ x: snap(cursor.x), y: snap(cursor.y) }] : [])], false)} />
              )}
              {mode === 'seat' && cursor && (() => {
                const s = seatOnEdge(poly, cursor);
                return s ? <circle className="studio-ghost" cx={X(s.x)} cy={Y(s.y)} r={CW * 0.45 * k} /> : null;
              })()}
            </svg>
            <div className="studio-legend">
              <span>{seats.length} puesto{seats.length === 1 ? '' : 's'}</span>
              <span>{(bb.w / 1000).toFixed(2)} × {(bb.h / 1000).toFixed(2)} m</span>
              {mode === 'draw' && <span className="mat">Clic en cada esquina · clic en la primera para cerrar · Esc cancela</span>}
            </div>
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
