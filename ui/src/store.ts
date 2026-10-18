import { create } from 'zustand';
import type {
  BgLayer, Capacity, Config, DeskType, ImportOut, LayerRole, Layout, Module, Plan, Progress, Pt, Suggestion, Variant,
} from './types';

export type Tool =
  | 'select' | 'pan' | 'seed' | 'door' | 'mainDoor' | 'exitDoor'
  | 'column' | 'fixedOffice' | 'restricted' | 'reserved' | 'window' | 'calibrate' | 'erase' | 'outline' | 'drawDoor' | 'placeDesk';

export type ColorBy = 'type' | 'role' | 'client';
export type Theme = 'system' | 'light' | 'dark';

function loadTheme(): Theme {
  try {
    const t = localStorage.getItem('seatopt-theme');
    if (t === 'light' || t === 'dark' || t === 'system') return t;
  } catch { /* sin almacenamiento */ }
  return 'system';
}

/** Selección múltiple (como AutoCAD). Ventanas por índice. */
export interface Selection { modules: string[]; obstacles: string[]; doors: string[]; windows: number[]; /** Anillos del contorno del recinto. */ region?: number[] }
export const emptySel = (): Selection => ({ modules: [], obstacles: [], doors: [], windows: [], region: [] });
export const selCount = (s: Selection) => s.modules.length + s.obstacles.length + s.doors.length + s.windows.length + (s.region?.length ?? 0);

interface Snapshot { plan: Plan | null; layout: Layout | null; pinned: Module[]; catalog: DeskType[] }

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
  /** Cuadrícula del lienzo. */
  gridOn: boolean;
  snapGrid: boolean;
  gridSize: number;
  /** Escritorio a colocar a mano (herramienta placeDesk). */
  placeType: string;
  placeAngle: number;
  placeUp: boolean;
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
  showBackground: true,
  selection: emptySel(),
  history: [],
  future: [],
  snapshot: () => {
    const { plan, layout, config, history } = get();
    set({ history: [...history.slice(-49), clone({ plan, layout, pinned: config?.pinned ?? [], catalog: config?.catalog ?? [] })], future: [] });
  },
  undo: () => {
    const h = get().history;
    if (!h.length) return;
    const last = h[h.length - 1];
    const { plan, layout, config, future } = get();
    const now = clone({ plan, layout, pinned: config?.pinned ?? [], catalog: config?.catalog ?? [] });
    set({ history: h.slice(0, -1), future: [...future.slice(-49), now], plan: last.plan, layout: last.layout, config: config ? { ...config, pinned: last.pinned, catalog: last.catalog.length ? last.catalog : config.catalog } : config, selection: emptySel() });
  },
  redo: () => {
    const f = get().future;
    if (!f.length) return;
    const next = f[f.length - 1];
    const { plan, layout, config, history } = get();
    const now = clone({ plan, layout, pinned: config?.pinned ?? [], catalog: config?.catalog ?? [] });
    set({ future: f.slice(0, -1), history: [...history.slice(-49), now], plan: next.plan, layout: next.layout, config: config ? { ...config, pinned: next.pinned, catalog: next.catalog.length ? next.catalog : config.catalog } : config, selection: emptySel() });
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
  gridOn: true,
  snapGrid: false,
  gridSize: 500,
  placeType: '',
  placeAngle: 0,
  placeUp: false,
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
    s += t.flexible ? d.quantity : d.quantity * seatsPerModule(t.shape, t.sides, t.perSide);
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
