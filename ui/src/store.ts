import { create } from 'zustand';
import type {
  BgLayer, Capacity, Config, ImportOut, LayerRole, Layout, Plan, Progress, Pt, Suggestion, Variant,
} from './types';

export type Tool =
  | 'pan' | 'seed' | 'door' | 'mainDoor' | 'exitDoor'
  | 'column' | 'fixedOffice' | 'restricted' | 'reserved' | 'window' | 'calibrate' | 'pin' | 'erase';

export type ColorBy = 'type' | 'role' | 'client';

export interface Selection { kind: 'door' | 'obstacle' | 'seat' | 'module'; id: string }

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
  variants: Variant[];
  suggestions: Suggestion[] | null;
  capacity: Capacity[] | null;
  job: JobUi | null;
  tool: Tool;
  colorBy: ColorBy;
  showKeepouts: boolean;
  showBackground: boolean;
  selection: Selection | null;
  toast: { text: string; tone: 'info' | 'error' } | null;
  calib: Pt[];
  autoExplore: boolean;
  gapClose: number;
  converter: string;
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
  variants: [],
  suggestions: null,
  capacity: null,
  job: null,
  tool: 'pan',
  colorBy: 'type',
  showKeepouts: true,
  showBackground: true,
  selection: null,
  toast: null,
  calib: [],
  autoExplore: false,
  gapClose: 1200,
  converter: '',
  set: (p) => set(p),
  updateConfig: (f) => {
    const c = get().config;
    if (!c) return;
    const n = clone(c);
    f(n);
    set({ config: n });
  },
  updatePlan: (f) => {
    const p = get().plan;
    if (!p) return;
    const n = clone(p);
    f(n);
    set({ plan: n, layout: null, capacity: null, suggestions: null });
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
export function requestedSeats(c: Config): number {
  let s = 0;
  for (const d of c.demand) {
    const t = c.catalog.find((t) => t.id === d.typeId);
    if (!t) continue;
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
