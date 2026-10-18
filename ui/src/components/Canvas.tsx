import { useEffect, useMemo, useRef, useState } from 'react';
import { placeDesk } from '../actions';
import { deskBody, doorInfo, moduleInfo, obstacleClears, obstacleInfo, regionInfo, type ClearInfo, type ClearKey } from '../clear';
import { checkDesk } from '../fit';
import { describeSel, deleteSelection, mergeSel, moveSelection, normRect, pickPoint, pickRect, pinSelection, rotateSelection } from '../edit';
import { emptySel, selCount, useApp, uid } from '../store';
import type { Door, DoorKind, Layout, Module, ObstacleKind, Plan, Pt, Seat } from '../types';

const DOOR_COLOR: Record<DoorKind, string> = { main: 'var(--egress)', exit: 'var(--exit)', office: 'var(--office-door)', ignore: 'var(--muted)' };
const OBS_FILL: Record<ObstacleKind, string> = { column: 'var(--column)', fixedOffice: 'var(--office)', restricted: 'var(--restricted)', reserved: 'var(--reserved)' };

function ringD(r: Pt[]) {
  if (!r.length) return '';
  let s = `M${r[0].x.toFixed(0)} ${r[0].y.toFixed(0)}`;
  for (let i = 1; i < r.length; i++) s += `L${r[i].x.toFixed(0)} ${r[i].y.toFixed(0)}`;
  return s + 'Z';
}

function distSeg(p: Pt, a: Pt, b: Pt) {
  const dx = b.x - a.x, dy = b.y - a.y;
  const l2 = dx * dx + dy * dy || 1;
  const t = Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / l2));
  return Math.hypot(p.x - (a.x + dx * t), p.y - (a.y + dy * t));
}

function pip(p: Pt, poly: Pt[]) {
  let c = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const a = poly[i], b = poly[j];
    if ((a.y > p.y) !== (b.y > p.y) && p.x < ((b.x - a.x) * (p.y - a.y)) / (b.y - a.y) + a.x) c = !c;
  }
  return c;
}

export function boundsOf(plan: Plan | null, bg: { paths: number[][] }[] | null): [number, number, number, number] | null {
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  const add = (x: number, y: number) => { x0 = Math.min(x0, x); y0 = Math.min(y0, y); x1 = Math.max(x1, x); y1 = Math.max(y1, y); };
  if (plan && plan.region.length) {
    for (const r of plan.region) for (const p of r) add(p.x, p.y);
    for (const p of plan.building ?? []) add(p.x, p.y);
    for (const r of plan.rooms ?? []) for (const p of r.poly) add(p.x, p.y);
  } else if (bg) {
    for (const l of bg) for (const p of l.paths) for (let i = 0; i < p.length; i += 2) add(p[i], p[i + 1]);
  }
  return isFinite(x0) ? [x0, y0, x1, y1] : null;
}

interface Props {
  onSeed: (p: Pt) => void;
  onCalibrated: (a: Pt, b: Pt) => void;
}

export default function Canvas({ onSeed, onCalibrated }: Props) {
  const st = useApp();
  const { plan, layout, config, background, hiddenLayers, tool, colorBy, showKeepouts, showBackground } = st;
  const svgRef = useRef<SVGSVGElement>(null);
  const [size, setSize] = useState({ w: 800, h: 600 });
  const [view, setView] = useState({ tx: 0, ty: 0, k: 0.02 });
  const [drag, setDrag] = useState<{ a: Pt; b: Pt } | null>(null);
  const [hover, setHover] = useState<{ seat: Seat; sx: number; sy: number } | null>(null);
  // selección por ventana (azul, izq→der) o captura (verde, der→izq), en pantalla
  const [box, setBox] = useState<{ ax: number; ay: number; bx: number; by: number } | null>(null);
  // arrastre de lo seleccionado (mundo)
  const [moving, setMoving] = useState<{ a: Pt; b: Pt } | null>(null);
  const [cursor, setCursor] = useState<Pt | null>(null);
  const panRef = useRef<{ sx: number; sy: number; tx: number; ty: number } | null>(null);
  // campo de edición de una separación (como los campos dinámicos de AutoCAD)
  const [editing, setEditing] = useState<{ key: ClearKey; x: number; y: number; text: string } | null>(null);

  // tamaño
  useEffect(() => {
    const el = svgRef.current?.parentElement;
    if (!el) return;
    const ro = new ResizeObserver(() => setSize({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // encuadre al cambiar de dibujo / recinto
  const fitKey = `${st.drawing?.drawingId}|${plan ? plan.region.length + ':' + (plan.region[0]?.length ?? 0) : 'np'}|${st.mmPerUnit}`;
  const fit = () => {
    const b = boundsOf(plan, background);
    if (!b) return;
    const [x0, y0, x1, y1] = b;
    const m = 40;
    const k = Math.min((size.w - 2 * m) / Math.max(1, x1 - x0), (size.h - 2 * m) / Math.max(1, y1 - y0));
    setView({ k, tx: size.w / 2 - ((x0 + x1) / 2) * k, ty: size.h / 2 + ((y0 + y1) / 2) * k });
  };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(fit, [fitKey, size.w > 0 && size.h > 0, background]);
  useEffect(() => {
    const h = () => fit();
    window.addEventListener('seatopt:fit', h);
    return () => window.removeEventListener('seatopt:fit', h);
  });

  const toWorld = (sx: number, sy: number): Pt => ({ x: (sx - view.tx) / view.k, y: -(sy - view.ty) / view.k });

  // ------------------------------------------------ enganche (como OSNAP): vértices, luego bordes, luego cuadrícula
  const snapIndex = useMemo(() => {
    const cell = 1000;
    const idx = new Map<string, number[]>();
    const seg = new Map<string, number[]>();
    const add = (x: number, y: number) => {
      const k = `${Math.floor(x / cell)},${Math.floor(y / cell)}`;
      const a = idx.get(k);
      if (a) a.push(x, y); else idx.set(k, [x, y]);
    };
    const addSeg = (x0: number, y0: number, x1: number, y1: number) => {
      if (Math.hypot(x1 - x0, y1 - y0) < 50) return;
      const cx0 = Math.floor(Math.min(x0, x1) / cell), cx1 = Math.floor(Math.max(x0, x1) / cell);
      const cy0 = Math.floor(Math.min(y0, y1) / cell), cy1 = Math.floor(Math.max(y0, y1) / cell);
      if ((cx1 - cx0 + 1) * (cy1 - cy0 + 1) > 400) return;
      for (let i = cx0; i <= cx1; i++) for (let j = cy0; j <= cy1; j++) {
        const k = `${i},${j}`;
        const a = seg.get(k);
        if (a) a.push(x0, y0, x1, y1); else seg.set(k, [x0, y0, x1, y1]);
      }
    };
    const ring = (r: Pt[], closed = true) => {
      r.forEach((q) => add(q.x, q.y));
      for (let i = 0; i < (closed ? r.length : r.length - 1); i++) { const q = r[(i + 1) % r.length]; addSeg(r[i].x, r[i].y, q.x, q.y); }
    };
    background?.forEach((l) => {
      if (hiddenLayers.includes(l.name)) return;
      l.paths.forEach((p) => {
        for (let i = 0; i < p.length; i += 2) add(p[i], p[i + 1]);
        for (let i = 0; i + 3 < p.length; i += 2) addSeg(p[i], p[i + 1], p[i + 2], p[i + 3]);
      });
    });
    plan?.region.forEach((r) => ring(r));
    plan?.obstacles.forEach((o) => ring(o.poly));
    plan?.doors.forEach((d) => ring([d.a, d.b], false));
    return { idx, seg, cell };
  }, [background, hiddenLayers, plan]);
  const SNAP_TOOLS: string[] = ['outline', 'drawDoor', 'window', 'column', 'fixedOffice', 'restricted', 'reserved', 'calibrate', 'placeDesk'];
  const snapPt = (w: Pt): Pt | null => {
    const r = 10 / view.k;
    const { idx, cell } = snapIndex;
    let best: Pt | null = null, bd = r;
    const cx = Math.floor(w.x / cell), cy = Math.floor(w.y / cell), span = Math.ceil(r / cell);
    for (let i = -span; i <= span; i++) for (let j = -span; j <= span; j++) {
      const a = idx.get(`${cx + i},${cy + j}`);
      if (!a) continue;
      for (let t = 0; t < a.length; t += 2) {
        const d = Math.hypot(a[t] - w.x, a[t + 1] - w.y);
        if (d < bd) { bd = d; best = { x: a[t], y: a[t + 1] }; }
      }
    }
    return best;
  };
  /** Punto más cercano sobre un borde (muros del recinto, oficinas, columnas, líneas del dibujo).
   *  Si hay cuadrícula con snap, se queda sobre el borde en la coordenada de la cuadrícula. */
  const snapEdge = (w: Pt, grid: boolean): Pt | null => {
    const r = 8 / view.k;
    const { seg, cell } = snapIndex;
    let best: Pt | null = null, bd = r;
    const cx = Math.floor(w.x / cell), cy = Math.floor(w.y / cell), span = Math.ceil(r / cell);
    const g = st.gridSize;
    for (let i = -span; i <= span; i++) for (let j = -span; j <= span; j++) {
      const a = seg.get(`${cx + i},${cy + j}`);
      if (!a) continue;
      for (let t = 0; t < a.length; t += 4) {
        const x0 = a[t], y0 = a[t + 1], x1 = a[t + 2], y1 = a[t + 3];
        const dx = x1 - x0, dy = y1 - y0, l2 = dx * dx + dy * dy;
        let u = Math.max(0, Math.min(1, ((w.x - x0) * dx + (w.y - y0) * dy) / l2));
        let q = { x: x0 + dx * u, y: y0 + dy * u };
        if (Math.hypot(q.x - w.x, q.y - w.y) >= bd) continue;
        if (grid) {
          // sobre el borde, en la línea de cuadrícula más cercana (en el eje que corre el borde)
          if (Math.abs(dx) >= Math.abs(dy) && Math.abs(dx) > 1) {
            const gx = Math.round(w.x / g) * g;
            const uu = (gx - x0) / dx;
            if (uu >= 0 && uu <= 1) { u = uu; q = { x: gx, y: y0 + dy * u }; }
          } else if (Math.abs(dy) > 1) {
            const gy = Math.round(w.y / g) * g;
            const uu = (gy - y0) / dy;
            if (uu >= 0 && uu <= 1) { u = uu; q = { x: x0 + dx * u, y: gy }; }
          }
        }
        bd = Math.hypot(q.x - w.x, q.y - w.y);
        best = q;
      }
    }
    return best;
  };
  const gridSnap = (w: Pt): Pt => {
    const g = st.gridSize;
    return { x: Math.round(w.x / g) * g, y: Math.round(w.y / g) * g };
  };
  // barra espaciadora sostenida: sin ningún enganche mientras se mantiene
  const [noSnap, setNoSnap] = useState(false);
  useEffect(() => {
    const isField = (e: KeyboardEvent) => { const t = e.target as HTMLElement; return !!t && ['INPUT', 'SELECT', 'TEXTAREA'].includes(t.tagName); };
    const down = (e: KeyboardEvent) => {
      if (e.code !== 'Space' || isField(e)) return;
      // que la barra no «vuelva a pulsar» el último botón usado
      e.preventDefault();
      if ((e.target as HTMLElement)?.tagName === 'BUTTON') (e.target as HTMLElement).blur();
      if (e.ctrlKey || e.metaKey) {
        if (!e.repeat) {
          const on = !useApp.getState().snapGrid;
          useApp.getState().set({ snapGrid: on });
          useApp.getState().say(on ? 'Snap a la cuadrícula activado' : 'Snap a la cuadrícula desactivado');
        }
        return;
      }
      setNoSnap(true);
    };
    const up = (e: KeyboardEvent) => { if (e.code === 'Space') setNoSnap(false); };
    const blur = () => setNoSnap(false);
    window.addEventListener('keydown', down);
    window.addEventListener('keyup', up);
    window.addEventListener('blur', blur);
    return () => { window.removeEventListener('keydown', down); window.removeEventListener('keyup', up); window.removeEventListener('blur', blur); };
  }, []);
  type SnapKind = 'vertex' | 'edge' | 'grid' | null;
  /** Enganche completo. `objects`: también a vértices y bordes. */
  const snapAll = (w: Pt, objects: boolean): { p: Pt; kind: SnapKind } => {
    if (noSnap) return { p: w, kind: null };
    if (objects) {
      const v = snapPt(w);
      if (v) return { p: v, kind: 'vertex' };
      const e = snapEdge(w, st.snapGrid);
      if (e) return { p: e, kind: 'edge' };
    }
    if (st.snapGrid) return { p: gridSnap(w), kind: 'grid' };
    return { p: w, kind: null };
  };
  const objectSnapNow = () => SNAP_TOOLS.includes(tool) || (tool === 'select' && !!moving);
  const worldAt = (sx: number, sy: number): Pt => snapAll(toWorld(sx, sy), objectSnapNow()).p;
  // contorno dibujado a mano
  const [outline, setOutline] = useState<Pt[]>([]);
  const [snapMark, setSnapMark] = useState<{ p: Pt; kind: SnapKind } | null>(null);
  useEffect(() => { if (tool !== 'outline') setOutline([]); }, [tool]);
  const finishOutline = (pts: Pt[]) => {
    if (pts.length < 3) return;
    const area = Math.abs(pts.reduce((a, p, i) => { const q = pts[(i + 1) % pts.length]; return a + p.x * q.y - q.x * p.y; }, 0)) / 2e6;
    const cur = st.plan;
    if (cur) st.snapshot();
    st.set({ plan: { region: [pts], obstacles: cur?.obstacles ?? [], doors: cur?.doors ?? [], windows: cur?.windows ?? [] }, areaM2: area, tool: 'select' });
    setOutline([]);
    st.say((cur?.doors.some((d) => d.kind === 'main') ? 'Contorno listo.' : 'Contorno listo. Ahora marca la puerta principal con «Dibujar puerta».'));
  };
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if (tool !== 'outline') return;
      if (e.key === 'Enter') finishOutline(outline);
      if (e.key === 'Backspace') setOutline((o) => o.slice(0, -1));
    };
    window.addEventListener('keydown', k);
    return () => window.removeEventListener('keydown', k);
  });
  const evPt = (e: React.MouseEvent | React.WheelEvent) => {
    const r = svgRef.current!.getBoundingClientRect();
    return { sx: e.clientX - r.left, sy: e.clientY - r.top };
  };

  // ------------------------------------------------ capas memorizadas
  const bgEls = useMemo(() => {
    if (!background) return null;
    return background.filter((l) => !hiddenLayers.includes(l.name)).map((l) => {
      let d = '';
      for (const p of l.paths) {
        if (p.length < 4) continue;
        d += `M${p[0]} ${p[1]}`;
        for (let i = 2; i < p.length; i += 2) d += `L${p[i]} ${p[i + 1]}`;
      }
      return <path key={l.name} d={d} stroke={l.color} />;
    });
  }, [background, hiddenLayers]);

  const typeColor = useMemo(() => {
    const m: Record<string, string> = {};
    config?.catalog.forEach((t) => (m[t.id] = t.color));
    return m;
  }, [config?.catalog]);
  const roleColor = useMemo(() => Object.fromEntries((config?.org.roles ?? []).map((r) => [r.id, r.color])), [config?.org.roles]);
  const clientColor = useMemo(() => Object.fromEntries((config?.org.clients ?? []).map((r) => [r.id, r.color])), [config?.org.clients]);

  const planEls = useMemo(() => {
    if (!plan) return null;
    return (
      <>
        {plan.building && plan.building.length > 2 && <path className="building" d={ringD(plan.building)} />}
        {plan.rooms?.map((r, i) => <path key={'r' + i} className="room" d={ringD(r.poly)} />)}
        <clipPath id="region-clip"><path d={plan.region.map(ringD).join('')} clipRule="evenodd" /></clipPath>
        <path className="region" d={plan.region.map(ringD).join('')} fillRule="evenodd" />
        {showKeepouts && layout?.keepouts.map((k, i) => <path key={'k' + i} className="keepout" d={ringD(k)} />)}
        {layout?.corridorLines && layout.corridorLines.length > 0 && layout.corridorWidth
          ? <path className="corridor-line" clipPath="url(#region-clip)" d={layout.corridorLines.map((l) => l.map((p, j) => `${j ? 'L' : 'M'}${p.x.toFixed(0)} ${p.y.toFixed(0)}`).join('')).join('')} strokeWidth={layout.corridorWidth} />
          : layout?.corridors.map((c, i) => <path key={'c' + i} className="corridor" d={ringD(c)} />)}
        {plan.obstacles.map((o) => <path key={o.id} className="obstacle" d={ringD(o.poly)} fill={OBS_FILL[o.kind]} />)}
        {plan.windows.map((w, i) => <line key={'w' + i} className="window" x1={w.a.x} y1={w.a.y} x2={w.b.x} y2={w.b.y} />)}
        {plan.doors.map((d) => <line key={d.id} className={'door ' + d.kind} x1={d.a.x} y1={d.a.y} x2={d.b.x} y2={d.b.y} stroke={DOOR_COLOR[d.kind]} />)}
      </>
    );
  }, [plan, layout?.keepouts, layout?.corridors, layout?.corridorLines, layout?.corridorWidth, showKeepouts]);

  const layoutEls = useMemo(() => {
    if (!layout) return null;
    const seatFill = (s: Seat) => {
      if (colorBy === 'role') return roleColor[s.roleId] ?? 'var(--paper)';
      if (colorBy === 'client') return clientColor[s.clientId] ?? 'var(--paper)';
      return 'var(--paper)';
    };
    const allMods: Module[] = [...layout.modules];
    // porción de escritorio de cada persona (para colorear por rol o cliente)
    const tile = (s: Seat): Pt[] => {
      const t = config?.catalog.find((t) => t.id === s.typeId);
      const w = (t?.seatWidth ?? 1200) * 0.5, d = t?.shape === 'l' ? (t?.depth ?? 600) : (t?.depth ?? 600);
      const f = s.facing, u = { x: -f.y, y: f.x };
      const near = 250; // del centro de la silla al borde del escritorio
      const c0 = { x: s.pos.x + f.x * near, y: s.pos.y + f.y * near };
      const c1 = { x: c0.x + f.x * d, y: c0.y + f.y * d };
      return [
        { x: c0.x - u.x * w, y: c0.y - u.y * w }, { x: c0.x + u.x * w, y: c0.y + u.y * w },
        { x: c1.x + u.x * w, y: c1.y + u.y * w }, { x: c1.x - u.x * w, y: c1.y - u.y * w },
      ];
    };
    return (
      <>
        {allMods.map((m) => (
          <path key={m.id} className={'desk' + (m.pinned ? ' pinned' : '')} d={ringD(m.body)}
            fill={colorBy === 'type' ? typeColor[m.typeId] ?? '#777' : 'var(--desk-neutral)'} />
        ))}
        {colorBy !== 'type' && layout.seats.filter((s) => (colorBy === 'role' ? s.roleId : s.clientId)).map((s) => (
          <path key={'t' + s.id} className="seat-tile" d={ringD(tile(s))} fill={seatFill(s)} />
        ))}
        {layout.seats.map((s) => <path key={s.id} className="chair" d={ringD(s.chair)} fill={seatFill(s)} />)}
      </>
    );
  }, [layout, colorBy, typeColor, roleColor, clientColor, config?.catalog]);

  // resaltado de la selección (y vista previa al mover)
  const sel = st.selection;
  const selEls = useMemo(() => {
    if (!selCount(sel)) return null;
    const mods = layout?.modules.filter((m) => sel.modules.includes(m.id)) ?? [];
    const seats = layout?.seats.filter((s) => sel.modules.includes(s.moduleId)) ?? [];
    return (
      <>
        {mods.map((m) => <path key={'s' + m.id} className="sel" d={ringD(m.body)} />)}
        {seats.map((s) => <path key={'sc' + s.id} className="sel thin" d={ringD(s.chair)} />)}
        {plan?.obstacles.filter((o) => sel.obstacles.includes(o.id)).map((o) => <path key={'so' + o.id} className="sel" d={ringD(o.poly)} />)}
        {plan?.doors.filter((d) => sel.doors.includes(d.id)).map((d) => <line key={'sd' + d.id} className="sel" x1={d.a.x} y1={d.a.y} x2={d.b.x} y2={d.b.y} />)}
        {plan?.windows.filter((_, i) => sel.windows.includes(i)).map((w, i) => <line key={'sw' + i} className="sel" x1={w.a.x} y1={w.a.y} x2={w.b.x} y2={w.b.y} />)}
        {plan?.region.filter((_, i) => sel.region?.includes(i)).map((r, i) => <path key={'sr' + i} className="sel" fill="none" d={ringD(r)} />)}
      </>
    );
  }, [sel, layout, plan]);

  // textos (fuera del grupo invertido)
  const sx = (p: Pt) => p.x * view.k + view.tx;
  const sy = (p: Pt) => -p.y * view.k + view.ty;
  const showNums = view.k * 1000 > 18;

  // ------------------------------------------------ cuadrícula (se adapta al zoom)
  const gridEls = useMemo(() => {
    if (!st.gridOn) return null;
    let g = st.gridSize;
    while (g * view.k < 7) g *= 2;
    const major = g * 5;
    const x0 = -view.tx / view.k, x1 = (size.w - view.tx) / view.k;
    const y1 = view.ty / view.k, y0 = -(size.h - view.ty) / view.k;
    let minor = '', maj = '';
    for (let x = Math.floor(x0 / g) * g; x <= x1; x += g) {
      const d = `M${x} ${y0}L${x} ${y1}`;
      if (Math.abs(x / major - Math.round(x / major)) < 1e-6) maj += d; else minor += d;
    }
    for (let y = Math.floor(y0 / g) * g; y <= y1; y += g) {
      const d = `M${x0} ${y}L${x1} ${y}`;
      if (Math.abs(y / major - Math.round(y / major)) < 1e-6) maj += d; else minor += d;
    }
    return <g className="grid"><path className="minor" d={minor} /><path className="major" d={maj} /></g>;
  }, [st.gridOn, st.gridSize, view, size]);

  // ------------------------------------------------ separaciones del elemento seleccionado (dorado, editables)
  const one = selCount(sel) === 1;
  const clr: ClearInfo | null = useMemo(() => {
    if (!one || !config || !plan) return null;
    if (sel.obstacles.length) {
      const o = plan.obstacles.find((x) => x.id === sel.obstacles[0]);
      return o ? obstacleInfo(config, o) : null;
    }
    if (sel.doors.length) {
      const d = plan.doors.find((x) => x.id === sel.doors[0]);
      return d && d.kind !== 'ignore' ? doorInfo(config, plan, d) : null;
    }
    if (sel.modules.length) {
      const m = layout?.modules.find((x) => x.id === sel.modules[0]);
      const t = m && config.catalog.find((x) => x.id === m.typeId);
      return m && t ? moduleInfo(config, m, t) : null;
    }
    if (sel.region?.length) return regionInfo(plan, sel.region[0]);
    return null;
  }, [one, sel, plan, layout, config]);

  const commitClear = (key: ClearKey, mm: number) => {
    if (!config || !plan || !isFinite(mm) || mm < 0) return;
    if (sel.region?.length && clr?.groups && typeof key === 'number') {
      const ri = sel.region[0];
      const edges = clr.groups[key] ?? [];
      st.updatePlan((p) => {
        const rc = p.regionClear ? p.regionClear.map((r) => [...r]) : [];
        while (rc.length < p.region.length) rc.push([]);
        const row = rc[ri];
        while (row.length < p.region[ri].length) row.push(0);
        for (const e of edges) row[e] = mm;
        p.regionClear = rc;
      });
      return;
    }
    if (sel.obstacles.length) {
      const id = sel.obstacles[0];
      st.updatePlan((p) => {
        const o = p.obstacles.find((x) => x.id === id);
        if (!o) return;
        if (o.poly.length === 4 && typeof key === 'number') {
          const cur = obstacleClears(config, o);
          cur[key] = mm;
          o.clear = cur;
        } else {
          o.clear = [mm];
        }
      });
    } else if (sel.doors.length) {
      const id = sel.doors[0];
      st.updatePlan((p) => {
        const d = p.doors.find((x) => x.id === id);
        if (!d) return;
        if (key === 'depth') d.clearDepth = mm;
        if (key === 'width') d.clearWidth = mm;
      });
    } else if (sel.modules.length) {
      const m = layout?.modules.find((x) => x.id === sel.modules[0]);
      if (!m) return;
      st.snapshot();
      st.updateConfig((c) => {
        const t = c.catalog.find((x) => x.id === m.typeId);
        if (!t) return;
        if (key === 'chair') t.clearChair = mm;
        if (key === 'back') t.clearBack = mm;
        if (key === 'left') t.clearLeft = mm;
        if (key === 'right') t.clearRight = mm;
      });
      st.say('Separación del tipo de escritorio cambiada: se aplica a todos los de ese tipo en la próxima optimización.');
    }
  };
  useEffect(() => { setEditing(null); }, [sel]);

  // ¿cabe el escritorio que se está colocando a mano?
  const placeType = config?.catalog.find((x) => x.id === st.placeType);
  const fitWhy = useMemo(() => {
    if (tool !== 'placeDesk' || !cursor || !plan || !config || !placeType) return [];
    return checkDesk(plan, config, layout, placeType, cursor, st.placeAngle, st.placeUp);
  }, [tool, cursor, plan, config, layout, placeType, st.placeAngle, st.placeUp]);

  // ------------------------------------------------ interacción
  const nearestDoor = (p: Pt): Door | null => {
    if (!plan) return null;
    let best: Door | null = null, bd = 900 / Math.min(1, view.k * 50);
    for (const d of plan.doors) {
      const dd = distSeg(p, d.a, d.b);
      if (dd < bd) { bd = dd; best = d; }
    }
    return best;
  };
  const seatAt = (p: Pt): Seat | null => {
    if (!layout) return null;
    let best: Seat | null = null, bd = 700;
    for (const s of layout.seats) {
      const d = Math.hypot(s.pos.x - p.x, s.pos.y - p.y);
      if (d < bd) { bd = d; best = s; }
    }
    return best;
  };

  const onDown = (e: React.MouseEvent) => {
    const { sx: x, sy: y } = evPt(e);
    const w = worldAt(x, y);
    if (e.button === 1 || e.button === 2 || tool === 'pan') {
      panRef.current = { sx: x, sy: y, tx: view.tx, ty: view.ty };
      return;
    }
    if (tool === 'select' && e.button === 0) {
      // ¿clic sobre algo ya seleccionado? → mover
      const hit = pickPoint(toWorld(x, y), 8 / view.k);
      const already = hit.modules.some((id) => sel.modules.includes(id)) || hit.obstacles.some((id) => sel.obstacles.includes(id))
        || hit.doors.some((id) => sel.doors.includes(id)) || hit.windows.some((i) => sel.windows.includes(i));
      if (already && !e.shiftKey && !(e.ctrlKey || e.metaKey)) { const a = snapAll(toWorld(x, y), true).p; setMoving({ a, b: a }); }
      else setBox({ ax: x, ay: y, bx: x, by: y });
      return;
    }
    if (['column', 'fixedOffice', 'restricted', 'reserved', 'window', 'drawDoor'].includes(tool)) {
      setDrag({ a: w, b: w });
    }
  };

  const onMove = (e: React.MouseEvent) => {
    const { sx: x, sy: y } = evPt(e);
    const sn = snapAll(toWorld(x, y), objectSnapNow());
    const w = sn.p;
    setCursor(w);
    if (sn.kind) {
      if (sn.p.x !== snapMark?.p.x || sn.p.y !== snapMark?.p.y || sn.kind !== snapMark?.kind) setSnapMark(sn);
    } else if (snapMark) setSnapMark(null);
    if (panRef.current) {
      const p = panRef.current;
      setView((v) => ({ ...v, tx: p.tx + x - p.sx, ty: p.ty + y - p.sy }));
      return;
    }
    if (drag) {
      setDrag({ a: drag.a, b: w });
      return;
    }
    if (box) {
      setBox({ ...box, bx: x, by: y });
      if (hover) setHover(null);
      return;
    }
    if (moving) {
      setMoving({ a: moving.a, b: w });
      return;
    }
    const s = seatAt(w);
    setHover(s ? { seat: s, sx: x, sy: y } : null);
  };

  const onUp = (e: React.MouseEvent) => {
    const { sx: x, sy: y } = evPt(e);
    const w = worldAt(x, y);
    if (panRef.current) {
      const moved = Math.hypot(x - panRef.current.sx, y - panRef.current.sy) > 4;
      panRef.current = null;
      if (moved || tool === 'pan') return;
    }
    if (box) {
      const b = box;
      setBox(null);
      const tiny = Math.hypot(b.bx - b.ax, b.by - b.ay) < 5;
      let picked;
      if (tiny) {
        picked = pickPoint(toWorld(x, y), 8 / view.k);
        if (!selCount(picked)) {
          if (!e.shiftKey && !(e.ctrlKey || e.metaKey)) st.set({ selection: emptySel() });
          return;
        }
      } else {
        const r = normRect(toWorld(b.ax, b.ay), toWorld(b.bx, b.by));
        picked = pickRect(r, b.bx < b.ax);
      }
      // como AutoCAD: una selección nueva reemplaza la anterior; Ctrl suma, Shift quita
      const add = e.ctrlKey || e.metaKey;
      st.set({ selection: e.shiftKey ? mergeSel(st.selection, picked, true) : add ? mergeSel(st.selection, picked, false) : picked });
      return;
    }
    if (moving) {
      const m = moving;
      setMoving(null);
      const dx = m.b.x - m.a.x, dy = m.b.y - m.a.y;
      if (Math.hypot(dx, dy) * view.k < 3) {
        // clic simple sobre lo seleccionado: dejar solo ese elemento
        const hit = pickPoint(toWorld(x, y), 8 / view.k);
        if (selCount(hit)) st.set({ selection: hit });
        return;
      }
      moveSelection(dx, dy);
      return;
    }
    if (drag) {
      const { a, b } = drag;
      setDrag(null);
      if (Math.hypot(a.x - b.x, a.y - b.y) < 100) return;
      if (tool === 'window') {
        st.updatePlan((p) => { p.windows.push({ a, b }); });
      } else if (tool === 'drawDoor') {
        const kind: DoorKind = st.plan?.doors.some((d) => d.kind === 'main') ? 'office' : 'main';
        st.updatePlan((p) => {
          p.doors.push({ id: uid('d'), kind, a, b, label: '' });
        });
        if (kind === 'main') st.set({ mainConfirmed: true, tool: 'select' });
        st.say(kind === 'main' ? 'Puerta principal agregada. Las siguientes que dibujes serán de oficina; cámbialas con «Cambiar tipo».' : 'Puerta agregada.');
      } else {
        const kind = tool as ObstacleKind;
        const label = kind === 'reserved' ? (prompt('Nombre de la zona reservada (p. ej. Impresoras):') ?? '') : '';
        st.updatePlan((p) => {
          p.obstacles.push({
            id: uid('ob'), kind, label,
            poly: [{ x: a.x, y: a.y }, { x: b.x, y: a.y }, { x: b.x, y: b.y }, { x: a.x, y: b.y }],
          });
        });
      }
      return;
    }
    if (e.button !== 0) return;
    switch (tool) {
      case 'outline': {
        if (outline.length >= 3 && Math.hypot((outline[0].x - w.x) * view.k, (outline[0].y - w.y) * view.k) < 12) {
          finishOutline(outline);
        } else {
          setOutline([...outline, w]);
        }
        break;
      }
      case 'seed':
        onSeed(w);
        break;
      case 'placeDesk':
        if (fitWhy.length) { st.say('No cabe ahí: ' + fitWhy.join('; ') + '. Mantén Espacio para moverlo sin enganche.', 'error'); break; }
        placeDesk(w);
        break;
      case 'door': case 'mainDoor': case 'exitDoor': {
        const d = nearestDoor(toWorld(x, y));
        if (!d) { st.say('No hay una puerta ahí: haz clic sobre una línea de color en el borde, o usa «Dibujar puerta».'); return; }
        if (tool === 'mainDoor') {
          st.set({ mainConfirmed: true, tool: 'select' });
          st.say('Puerta principal marcada.');
        }
        st.updatePlan((p) => {
          const dd = p.doors.find((x) => x.id === d.id)!;
          if (tool === 'mainDoor') {
            p.doors.forEach((x) => { if (x.kind === 'main') x.kind = 'office'; });
            dd.kind = 'main';
          } else if (tool === 'exitDoor') {
            dd.kind = dd.kind === 'exit' ? 'office' : 'exit';
          } else {
            const order: DoorKind[] = ['office', 'exit', 'main', 'ignore'];
            const next = order[(order.indexOf(dd.kind) + 1) % order.length];
            if (next === 'main') p.doors.forEach((x) => { if (x.kind === 'main') x.kind = 'office'; });
            dd.kind = next;
          }
        });
        break;
      }
      case 'calibrate': {
        const c = [...st.calib, w];
        if (c.length === 2) { st.set({ calib: [] }); onCalibrated(c[0], c[1]); } else st.set({ calib: c });
        break;
      }
      case 'erase': {
        if (!plan) return;
        const r = toWorld(x, y);
        const ob = [...plan.obstacles].reverse().find((o) => pip(r, o.poly));
        if (ob) { st.updatePlan((p) => { p.obstacles = p.obstacles.filter((o) => o.id !== ob.id); }); return; }
        const wi = plan.windows.findIndex((q) => distSeg(r, q.a, q.b) < 400);
        if (wi >= 0) { st.updatePlan((p) => { p.windows.splice(wi, 1); }); return; }
        const d = nearestDoor(r);
        if (d) st.updatePlan((p) => { p.doors = p.doors.filter((x) => x.id !== d.id); });
        break;
      }
      case 'pan':
        break;
    }
  };

  const zoomBy = (f: number) => {
    setView((v) => {
      const k = Math.min(2, Math.max(0.0005, v.k * f));
      const x = size.w / 2, y = size.h / 2;
      const wx = (x - v.tx) / v.k, wy = (y - v.ty) / v.k;
      return { k, tx: x - wx * k, ty: y - wy * k };
    });
  };

  const onWheel = (e: React.WheelEvent) => {
    const { sx: x, sy: y } = evPt(e);
    const f = Math.exp(-e.deltaY * 0.0015);
    setView((v) => {
      const k = Math.min(2, Math.max(0.0005, v.k * f));
      const wx = (x - v.tx) / v.k, wy = (y - v.ty) / v.k;
      return { k, tx: x - wx * k, ty: y - wy * k };
    });
  };

  const cursorCls = tool === 'pan' ? 'grab' : tool === 'select' ? (moving ? 'grab' : 'arrow') : 'cross';
  const moveOff = moving ? { x: moving.b.x - moving.a.x, y: moving.b.y - moving.a.y } : null;
  const flagged = (s: Seat) => s.flags.some((f) => f.startsWith('Sin acceso') || f.startsWith('Recorrido') || f.startsWith('Regla'));

  return (
    <div className={'canvas ' + cursorCls}>
      <svg ref={svgRef} width={size.w} height={size.h} onMouseDown={onDown} onMouseMove={onMove} onMouseUp={onUp}
        onMouseLeave={() => { panRef.current = null; setHover(null); setCursor(null); }} onWheel={onWheel}
        onContextMenu={(e) => e.preventDefault()} onDoubleClick={() => { if (tool === 'outline') finishOutline(outline); }} role="img" aria-label="Plano de la oficina">
        <g transform={`translate(${view.tx} ${view.ty}) scale(${view.k} ${-view.k})`}>
          {showBackground && <g className="bg">{bgEls}</g>}
          {planEls}
          {gridEls}
          {layoutEls}
          {clr?.zones.map((z, i) => <path key={'cz' + i} className="clear-zone" d={ringD(z)} />)}
          {tool === 'placeDesk' && cursor && (() => {
            const t = config?.catalog.find((x) => x.id === st.placeType);
            if (!t) return null;
            const g = deskBody(t, cursor, st.placeAngle);
            const two = t.shape === 'straight' && t.sides >= 2;
            const sgn = st.placeUp ? 1 : -1;
            const chairAt = (s: number) => ({ x: cursor.x + g.v.x * s * (g.depth / 2 + 260), y: cursor.y + g.v.y * s * (g.depth / 2 + 260) });
            return (
              <g className={'ghost' + (fitWhy.length ? ' bad' : ' ok')}>
                <path d={ringD(g.body)} fill={t.color} />
                {(two ? [1, -1] : [sgn]).map((s) => { const c = chairAt(s); return <circle key={s} cx={c.x} cy={c.y} r={220} />; })}
              </g>
            );
          })()}
          {moveOff ? <g transform={`translate(${moveOff.x} ${moveOff.y})`} opacity={0.7}>{selEls}</g> : selEls}
          {drag && (tool === 'window'
            ? <line className="draft" x1={drag.a.x} y1={drag.a.y} x2={drag.b.x} y2={drag.b.y} />
            : tool === 'drawDoor' ? <line className="draft door-draft" x1={drag.a.x} y1={drag.a.y} x2={drag.b.x} y2={drag.b.y} />
            : <rect className="draft" x={Math.min(drag.a.x, drag.b.x)} y={Math.min(drag.a.y, drag.b.y)} width={Math.abs(drag.b.x - drag.a.x)} height={Math.abs(drag.b.y - drag.a.y)} />)}
          {st.calib.length === 1 && cursor && <line className="draft" x1={st.calib[0].x} y1={st.calib[0].y} x2={cursor.x} y2={cursor.y} />}
          {outline.length > 0 && (
            <polyline className="draft outline-draft" points={[...outline, ...(cursor ? [cursor] : [])].map((p) => `${p.x},${p.y}`).join(' ')} />
          )}
        </g>
        <g className="labels">
          {snapMark?.kind === 'vertex' && <rect className="snap" x={sx(snapMark.p) - 5} y={sy(snapMark.p) - 5} width={10} height={10} />}
          {snapMark?.kind === 'edge' && <path className="snap" d={`M${sx(snapMark.p) - 5} ${sy(snapMark.p) - 5}L${sx(snapMark.p) + 5} ${sy(snapMark.p) + 5}M${sx(snapMark.p) + 5} ${sy(snapMark.p) - 5}L${sx(snapMark.p) - 5} ${sy(snapMark.p) + 5}`} />}
          {snapMark?.kind === 'grid' && <circle className="snap grid-snap" cx={sx(snapMark.p)} cy={sy(snapMark.p)} r={3.5} />}
          {outline.length > 0 && <circle className="snap-start" cx={sx(outline[0])} cy={sy(outline[0])} r={6} />}
          {plan?.rooms?.filter((r) => r.label).map((r, i) => {
            const c = r.poly.reduce((a, p) => ({ x: a.x + p.x / r.poly.length, y: a.y + p.y / r.poly.length }), { x: 0, y: 0 });
            return <text key={'rl' + i} x={sx(c)} y={sy(c)} className="ob-label">{r.label}</text>;
          })}
          {plan?.obstacles.filter((o) => o.label).map((o) => {
            const c = o.poly.reduce((a, p) => ({ x: a.x + p.x / o.poly.length, y: a.y + p.y / o.poly.length }), { x: 0, y: 0 });
            return <text key={o.id} x={sx(c)} y={sy(c)} className="ob-label">{o.label}</text>;
          })}
          {showNums && layout?.seats.map((s) => <text key={s.id} x={sx(s.pos)} y={sy(s.pos) + 3.5} className="seat-num">{s.number}</text>)}
          {clr?.labels.map((l, i) => (
            <g key={'cl' + i} className="clear-label" onMouseDown={(e) => e.stopPropagation()} onMouseUp={(e) => e.stopPropagation()}
              onClick={(e) => { e.stopPropagation(); setEditing({ key: l.key, x: sx(l.pos), y: sy(l.pos), text: (l.value / 1000).toFixed(2) }); }}>
              <title>{l.title} · clic para cambiar</title>
              <rect x={sx(l.pos) - (l.prefix ? 33 : 21)} y={sy(l.pos) - 9} width={l.prefix ? 66 : 42} height={18} rx={4} />
              <text x={sx(l.pos)} y={sy(l.pos) + 4}>{l.prefix ? `${l.prefix} ` : ''}{(l.value / 1000).toFixed(2)}</text>
            </g>
          ))}
          {layout?.seats.filter(flagged).map((s) => <circle key={'f' + s.id} cx={sx(s.pos)} cy={sy(s.pos)} r={4} className="flag" />)}
          {layout?.violations.filter((v) => v.at).map((v, i) => <circle key={'v' + i} cx={sx(v.at!)} cy={sy(v.at!)} r={14} className={'viol' + (v.hard ? ' hard' : '')} />)}
        </g>
      </svg>
      {box && (() => {
        const crossing = box.bx < box.ax;
        return <div className={'selbox ' + (crossing ? 'crossing' : 'window')} style={{ left: Math.min(box.ax, box.bx), top: Math.min(box.ay, box.by), width: Math.abs(box.bx - box.ax), height: Math.abs(box.by - box.ay) }} />;
      })()}
      {selCount(sel) > 0 && (
        <div className="selbar" role="toolbar" aria-label="Acciones sobre la selección">
          <span>{describeSel(sel)}</span>
          {sel.modules.length > 0 && <button className="small" title="Conserva estos escritorios y a sus personas en las próximas optimizaciones (P)" onClick={() => pinSelection(true)}>Fijar y bloquear</button>}
          {sel.modules.length > 0 && <button className="small" onClick={() => pinSelection(false)}>Soltar</button>}
          {sel.doors.length > 0 && (
            <button className="small" title="Cambia el lado hacia el que abre la puerta (y su despeje)"
              onClick={() => st.updatePlan((p) => { p.doors.forEach((d) => { if (sel.doors.includes(d.id)) d.flip = !d.flip; }); })}>Invertir sentido</button>
          )}
          {(sel.modules.length > 0 || sel.obstacles.length > 0) && <button className="small" title="Gira los escritorios u obstáculos seleccionados 90° sobre su centro (R)" onClick={() => rotateSelection(90)}>Girar 90°</button>}
          {(sel.modules.length + sel.obstacles.length + sel.doors.length + sel.windows.length) > 0 && <button className="small danger" title="Supr" onClick={deleteSelection}>Borrar</button>}
          {sel.region?.length ? <span className="hint-inline">Clic en un número dorado para la separación de esa arista</span> : null}
          <button className="ghost small" title="Esc" onClick={() => st.set({ selection: emptySel() })}>Quitar selección</button>
        </div>
      )}
      <div className="canvas-tools" role="toolbar" aria-label="Vista">
        <button className={tool === 'select' ? 'on' : ''} aria-pressed={tool === 'select'} title="Seleccionar (Esc)" onClick={() => st.set({ tool: 'select' })}>
          <svg viewBox="0 0 20 20" aria-hidden><path d="M5 3l10 7-4.5 1 2.5 5-2 1-2.5-5L5 15z" /></svg><span>Seleccionar</span>
        </button>
        <button className={tool === 'pan' ? 'on' : ''} aria-pressed={tool === 'pan'} title="Mover el plano (también con botón derecho)" onClick={() => st.set({ tool: tool === 'pan' ? 'select' : 'pan' })}>
          <svg viewBox="0 0 20 20" aria-hidden><path d="M10 2v16M2 10h16M10 2l-2.5 2.5M10 2l2.5 2.5M10 18l-2.5-2.5M10 18l2.5-2.5M2 10l2.5-2.5M2 10l2.5 2.5M18 10l-2.5-2.5M18 10l-2.5 2.5" /></svg><span>Mover</span>
        </button>
        {plan && config && config.catalog.length > 0 && (
          <button className={tool === 'placeDesk' ? 'on' : ''} aria-pressed={tool === 'placeDesk'}
            title="Agregar un escritorio a mano (el tipo se elige en el panel: Escritorios u Optimizar). R gira, F cambia el lado de la silla."
            onClick={() => st.set(tool === 'placeDesk' ? { tool: 'select' } : { tool: 'placeDesk', placeType: st.placeType || config.catalog[0].id, selection: emptySel() })}>
            <svg viewBox="0 0 20 20" aria-hidden><rect x="3" y="5" width="14" height="6" rx="1" /><path d="M7 14h6M10 11v3" /></svg><span>Agregar escritorio</span>
          </button>
        )}
        <hr />
        <button title="Acercar (rueda del mouse)" onClick={() => zoomBy(1.4)}>
          <svg viewBox="0 0 20 20" aria-hidden><path d="M10 4v12M4 10h12" /></svg><span>Acercar</span>
        </button>
        <button title="Alejar" onClick={() => zoomBy(1 / 1.4)}>
          <svg viewBox="0 0 20 20" aria-hidden><path d="M4 10h12" /></svg><span>Alejar</span>
        </button>
        <button title="Encuadrar todo el plano" onClick={fit}>
          <svg viewBox="0 0 20 20" aria-hidden><path d="M3 7V3h4M13 3h4v4M17 13v4h-4M7 17H3v-4" /></svg><span>Encuadrar</span>
        </button>
      </div>
      {editing && (
        <input className="clear-edit" autoFocus style={{ left: editing.x - 32, top: editing.y - 13 }} value={editing.text} aria-label="Separación en metros"
          onFocus={(e) => e.target.select()}
          onChange={(e) => setEditing({ ...editing, text: e.target.value })}
          onKeyDown={(e) => {
            if (e.key === 'Enter') { commitClear(editing.key, parseFloat(editing.text.replace(',', '.')) * 1000); setEditing(null); }
            if (e.key === 'Escape') setEditing(null);
            e.stopPropagation();
          }}
          onBlur={() => { commitClear(editing.key, parseFloat(editing.text.replace(',', '.')) * 1000); setEditing(null); }} />
      )}
      <div className="grid-ctl" role="group" aria-label="Cuadrícula">
        <label><input type="checkbox" checked={st.gridOn} onChange={(e) => st.set({ gridOn: e.target.checked })} /> Cuadrícula</label>
        <label title="El cursor se engancha a la cuadrícula (y siempre a vértices y bordes al dibujar o mover). Mantén la barra espaciadora para soltarlo un momento; Ctrl+Espacio lo activa o desactiva."><input type="checkbox" checked={st.snapGrid} onChange={(e) => st.set({ snapGrid: e.target.checked })} /> Snap</label>
        <select value={st.gridSize} aria-label="Tamaño de la cuadrícula" onChange={(e) => st.set({ gridSize: parseFloat(e.target.value) })}>
          {[100, 250, 500, 1000].map((g) => <option key={g} value={g}>{(g / 1000).toFixed(g < 1000 ? 2 : 0)} m</option>)}
        </select>
      </div>
      <div className="readout" aria-live="off">
        <span>x</span><b>{cursor ? (cursor.x / 1000).toFixed(2) : '—'}</b>
        <span>y</span><b>{cursor ? (cursor.y / 1000).toFixed(2) : '—'}</b>
        <span>m</span>
      </div>
      {tool === 'placeDesk' && cursor && placeType && (
        <div className={'fit-tip' + (fitWhy.length ? ' bad' : ' ok')} style={{ left: sx(cursor) + 18, top: sy(cursor) + 18 }}>
          {fitWhy.length ? <><b>No cabe aquí</b><ul>{fitWhy.map((w) => <li key={w}>{w}</li>)}</ul></> : <b>Cabe · clic para ubicarlo</b>}
        </div>
      )}
      {hover && <SeatTip layout={layout} seat={hover.seat} x={hover.sx} y={hover.sy} />}
    </div>
  );
}

function SeatTip({ seat, x, y }: { layout: Layout | null; seat: Seat; x: number; y: number }) {
  const config = useApp((s) => s.config);
  const t = config?.catalog.find((t) => t.id === seat.typeId);
  const r = config?.org.roles.find((r) => r.id === seat.roleId);
  const c = config?.org.clients.find((c) => c.id === seat.clientId);
  return (
    <div className="tip" style={{ left: x + 14, top: y + 14 }}>
      <strong>Puesto {seat.number}</strong>
      <div>{t?.name}</div>
      {(r || c) && <div>{[r?.name, c?.name].filter(Boolean).join(', ')}</div>}
      {seat.flags.length > 0 && <ul>{seat.flags.map((f, i) => <li key={i}>{f}</li>)}</ul>}
    </div>
  );
}
