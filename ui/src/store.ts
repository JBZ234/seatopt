import { create } from 'zustand';
import { regionArea } from './draw';
import type {
  BgLayer, Capacity, Config, DeskType, ImportOut, LayerRole, Layout, Module, ObstacleKind, Plan, Progress, Pt, Suggestion, Variant,
} from './types';

export type Tool =
  | 'select' | 'pan' | 'seed' | 'door' | 'mainDoor' | 'exitDoor'
  | 'seedAdd' | 'seedRemove'
  | 'column' | 'fixedOffice' | 'restricted' | 'reserved' | 'window' | 'calibrate' | 'erase' | 'outline' | 'drawDoor' | 'placeDesk'
  | 'drawArea' | 'drawObs' | 'drawMain' | 'drawExit';

/** Tools that draw point by point (like AutoCAD): click the points, Enter or right-click finishes. */
export const DRAFT_TOOLS: Tool[] = ['drawArea', 'drawObs', 'drawMain', 'drawDoor', 'drawExit', 'outline'];
export type PlanMode = 'draw' | 'detect';

/** New plans always start empty, to draw by hand (detecting is a button). */
function loadPlanMode(): PlanMode { return 'draw'; }
export function savePlanMode(_m: PlanMode) { /* not remembered: drawing is always the start */ }

/** Units shown on the plan (coordinates, lengths) and used for typed lengths. */
export type DispUnit = 'mm' | 'cm' | 'm';
export const UNIT_MM: Record<DispUnit, number> = { mm: 1, cm: 10, m: 1000 };
function loadDispUnit(): DispUnit {
  try { const u = localStorage.getItem('seatopt-unit'); if (u === 'mm' || u === 'cm' || u === 'm') return u; } catch { /* no storage */ }
  return 'm';
}
/** A length in the units shown, with a sensible number of decimals. */
export function fmtLen(mm: number, u: DispUnit, withUnit = true): string {
  const v = mm / UNIT_MM[u];
  const t = u === 'm' ? v.toFixed(2) : u === 'cm' ? v.toFixed(1) : v.toFixed(0);
  return withUnit ? `${t} ${u}` : t;
}

/** Workflow steps (index in the top bar). */
export const STEP = { plan: 0, paths: 1, desks: 2, rules: 3, org: 4, optimize: 5 } as const;

export type ColorBy = 'type' | 'role' | 'client';
export type Theme = 'system' | 'light' | 'dark';

function loadPalette(): string {
  try { return localStorage.getItem('seatopt-palette') || 'verde'; } catch { return 'verde'; }
}

function loadTheme(): Theme {
  try {
    const t = localStorage.getItem('seatopt-theme');
    if (t === 'light' || t === 'dark' || t === 'system') return t;
  } catch { /* sin almacenamiento */ }
  return 'system';
}

/** Selección múltiple (como AutoCAD). Ventanas por índice. */
export interface Selection { modules: string[]; obstacles: string[]; doors: string[]; windows: number[]; /** Anillos del contorno del recinto. */ region?: number[]; /** Lines of the drawing ("layerIndex:pathIndex"). */ bg?: string[] }
export const emptySel = (): Selection => ({ modules: [], obstacles: [], doors: [], windows: [], region: [], bg: [] });
export const selCount = (s: Selection) => s.modules.length + s.obstacles.length + s.doors.length + s.windows.length + (s.region?.length ?? 0) + (s.bg?.length ?? 0);

interface Snapshot { plan: Plan | null; layout: Layout | null; pinned: Module[]; catalog: DeskType[]; hiddenPaths?: string[] }

export interface JobUi { kind: string; progress: Progress | null; signal: { cancel: boolean } }

export interface AppState {
  step: number;
  drawing: ImportOut | null;
  roles: Record<string, LayerRole>;
  mmPerUnit: number;
  background: BgLayer[] | null;
  hiddenLayers: string[];
  plan: Plan | null;
  notes: string[];
  areaM2: number;
  config: Config | null;
  layout: Layout | null;
  /** Mejores soluciones distintas de la última optimización (la elegida es `layout`). */
  alternatives: Layout[];
  variants: Variant[];
  suggestions: Suggestion[] | null;
  capacity: Capacity[] | null;
  job: JobUi | null;
  tool: Tool;
  colorBy: ColorBy;
  showKeepouts: boolean;
  showCorridor: boolean;
  showAccess: boolean;
  showBackground: boolean;
  selection: Selection;
  history: Snapshot[];
  /** Estados deshechos (para rehacer). */
  future: Snapshot[];
  /** Guarda el estado actual para poder deshacer. */
  snapshot: () => void;
  undo: () => void;
  redo: () => void;
  toast: { text: string; tone: 'info' | 'error' } | null;
  calib: Pt[];
  autoExplore: boolean;
  gapClose: number;
 converter: string;
 pdfPaper: string;
  pdfScale: number;
  pdfBackground: boolean;
  scaleConfirmed: boolean;
  mainConfirmed: boolean;
  theme: Theme;
  /** Paleta de color (ver palette.ts). */
  palette: string;
  /** Sube cada vez que llega un resultado nuevo: anima la aparición de los escritorios. */
  reveal: number;
  /** Estudio de escritorios (crear o editar un tipo propio). */
  studio: { open: boolean; editId?: string } | null;
  /** Cuadrícula del lienzo. */
  gridOn: boolean;
  snapGrid: boolean;
  gridSize: number;
  /** Escritorio a colocar a mano (herramienta placeDesk). */
  placeType: string;
  placeAngle: number;
  placeUp: boolean;
  /** Points inside each chosen area to furnish (empty = detected automatically). */
  areaSeeds: Pt[];
  /** Circulation path being edited (and its selected node). */
  pathSel: { id: string; node: number | null } | null;
  /** Show office-to-office (secondary) paths. */
  showSecondary: boolean;
  /** How the plan is made: drawn by hand or detected from the drawing. */
  planMode: PlanMode;
  /** What opening a plan does (remembered): start empty to draw, or detect. */
  planModePref: PlanMode;
  /** Drawing tools: outline by points or rectangle; kind of obstacle; orthogonal lines (F8). */
  drawShape: 'poly' | 'rect';
  obsKind: ObstacleKind;
  ortho: boolean;
  /** Point to show on the plan (from the check list). */
  focus: { p: Pt; n: number } | null;
  /** Drawing unit chosen (mm per unit, before the scale): mm, cm, m, in, ft. */
  unitMm: number;
  /** What the file's doors suggest, when it does not match the declared units. */
  scaleHint: { mmPerUnit: number; reason: string } | null;
  dispUnit: DispUnit;
  /** Lines of the drawing hidden one by one ("layerIndex:pathIndex"). */
  hiddenPaths: string[];
  set: (p: Partial<AppState>) => void;
  updateConfig: (f: (c: Config) => void) => void;
  updatePlan: (f: (p: Plan) => void) => void;
  say: (text: string, tone?: 'info' | 'error') => void;
}

const clone = <T,>(x: T): T => structuredClone(x);

export const useApp = create<AppState>((set, get) => ({
  step: 0,
  drawing: null,
  roles: {},
  mmPerUnit: 1,
  background: null,
  hiddenLayers: [],
  plan: null,
  notes: [],
  areaM2: 0,
  config: null,
  layout: null,
  alternatives: [],
  variants: [],
  suggestions: null,
  capacity: null,
  job: null,
  tool: 'select',
  colorBy: 'type',
  showKeepouts: true,
  showCorridor: true,
  showAccess: true,
  showBackground: true,
  selection: emptySel(),
  history: [],
  future: [],
  snapshot: () => {
    const { plan, layout, config, history, hiddenPaths } = get();
    set({ history: [...history.slice(-49), clone({ plan, layout, pinned: config?.pinned ?? [], catalog: config?.catalog ?? [], hiddenPaths })], future: [] });
  },
  undo: () => {
    const h = get().history;
    if (!h.length) return;
    const last = h[h.length - 1];
    const { plan, layout, config, future, hiddenPaths } = get();
    const now = clone({ plan, layout, pinned: config?.pinned ?? [], catalog: config?.catalog ?? [], hiddenPaths });
    set({ history: h.slice(0, -1), future: [...future.slice(-49), now], plan: last.plan, hiddenPaths: last.hiddenPaths ?? get().hiddenPaths, areaM2: last.plan ? regionArea(last.plan.region) / 1e6 : 0, layout: last.layout, config: config ? { ...config, pinned: last.pinned, catalog: last.catalog.length ? last.catalog : config.catalog } : config, selection: emptySel() });
  },
  redo: () => {
    const f = get().future;
    if (!f.length) return;
    const next = f[f.length - 1];
    const { plan, layout, config, history, hiddenPaths } = get();
    const now = clone({ plan, layout, pinned: config?.pinned ?? [], catalog: config?.catalog ?? [], hiddenPaths });
    set({ future: f.slice(0, -1), history: [...history.slice(-49), now], plan: next.plan, hiddenPaths: next.hiddenPaths ?? get().hiddenPaths, areaM2: next.plan ? regionArea(next.plan.region) / 1e6 : 0, layout: next.layout, config: config ? { ...config, pinned: next.pinned, catalog: next.catalog.length ? next.catalog : config.catalog } : config, selection: emptySel() });
  },
  toast: null,
  calib: [],
  autoExplore: false,
  gapClose: 1200,
  converter: '',
  pdfPaper: 'A3',
  pdfScale: 0,
  pdfBackground: false,
  scaleConfirmed: false,
  mainConfirmed: true,
  theme: loadTheme(),
  palette: loadPalette(),
  reveal: 0,
  studio: null,
  gridOn: true,
  snapGrid: false,
  gridSize: 500,
  placeType: '',
  placeAngle: 0,
  placeUp: false,
  pathSel: null,
  showSecondary: false,
  areaSeeds: [],
  planMode: loadPlanMode(),
  planModePref: loadPlanMode(),
  drawShape: 'poly',
  obsKind: 'column',
  ortho: true,
  focus: null,
  unitMm: 1,
  scaleHint: null,
  dispUnit: loadDispUnit(),
  hiddenPaths: [],
  set: (p) => set(p),
  updateConfig: (f) => {
    const c = get().config;
    if (!c) return;
    const n = clone(c);
    f(n);
    set({ config: n });
  },
  updatePlan: (f) => {
    const p = get().plan ?? { region: [], obstacles: [], doors: [], windows: [] };
    get().snapshot();
    const n = clone(p);
    f(n);
    set({ plan: n, capacity: null, suggestions: null });
  },
  say: (text, tone = 'info') => {
    set({ toast: { text, tone } });
    const mine = get().toast;
    setTimeout(() => {
      if (get().toast === mine) set({ toast: null });
    }, tone === 'error' ? 9000 : 4500);
  },
}));

export const uid = (p: string) => `${p}${Math.random().toString(36).slice(2, 8)}`;

/** Puestos que pide la demanda actual. */
export const hasDemand = (c: Config) => c.demand.some((d) => d.max || d.quantity > 0);

export function requestedSeats(c: Config): number {
  let s = 0;
  for (const d of c.demand) {
    const t = c.catalog.find((t) => t.id === d.typeId);
    if (!t || d.max) continue;
    s += t.flexible ? d.quantity : d.quantity * (t.shape === 'custom' ? Math.max(1, t.seats?.length ?? 1) : seatsPerModule(t.shape, t.sides, t.perSide));
  }
  return s;
}

export function seatsPerModule(shape: string, sides: number, perSide: number) {
  return shape === 'l' ? 1 : sides * perSide;
}

export interface ProjectFile {
  version: 1;
  drawingName: string;
  mmPerUnit: number;
  roles: Record<string, LayerRole>;
  plan: Plan | null;
  config: Config | null;
  layout: Layout | null;
  background: BgLayer[] | null;
}
