import { deskBody, deskDims } from './clear';
import { call, openPlan, openText, runJob, saveBase64, saveText } from './api';
import { hasDemand, savePlanMode, STEP, uid, useApp, type PlanMode, type ProjectFile } from './store';
import { addAreaRing, areaGroups, emptyPlan, regionArea, removeAreaGroup, ringArea, scalePlan, selfCross, tidyRing } from './draw';
import type {
  AnalyzeResult, BgLayer, CircPath, Config, Capacity, Defaults, DoorKind, Layout, Module, ObstacleKind, Plan, Problem, Pt, Suggestion, Variant,
} from './types';

const S = () => useApp.getState();

export async function loadDefaults(): Promise<Config> {
  const d = await call<Defaults>('defaults');
  // por defecto: el escritorio recto básico, todos los que quepan
  const basic = d.catalog.find((t) => t.id === 'recto_120') ?? d.catalog[0];
  return { catalog: d.catalog, demand: basic ? [{ typeId: basic.id, quantity: 0, max: true }] : [], rules: d.rules, customRules: [], weights: d.weights, org: { roles: [], clients: [], teams: [], affinities: [] }, settings: { ...d.settings, timeLimit: 0 }, pinned: [] };
}

export async function ensureConfig() {
  if (!S().config) S().set({ config: await loadDefaults() });
}

export async function importPlan() {
  try {
    const out = await openPlan(S().converter);
    if (!out) return;
    const roles: Record<string, typeof out.layers[number]['role']> = {};
    out.layers.forEach((l) => (roles[l.name] = l.role));
    S().set({ scaleConfirmed: false, tool: 'select', drawing: out, roles, mmPerUnit: out.scale.mmPerUnit, plan: null, layout: null, alternatives: [], variants: [], suggestions: null, capacity: null, background: null, hiddenLayers: [], step: 0, areaSeeds: [], notes: [out.scale.reason] });
    // plano nuevo: reglas y ajustes por defecto (los que dan más puestos); se conservan los
    // escritorios propios del catálogo (los del Studio y los editados)
    const def = await loadDefaults();
    const prev = S().config;
    const extra = prev ? prev.catalog.filter((t) => !def.catalog.some((d) => d.id === t.id)) : [];
    S().set({ config: { ...def, catalog: [...def.catalog, ...extra] } });
    await loadBackground();
    if (S().planModePref === 'draw') {
      S().set({ plan: emptyPlan(), planMode: 'draw', areaM2: 0, mainConfirmed: true, tool: 'drawArea', drawShape: 'poly' });
    } else {
      await analyzePlan(null);
    }
  } catch (e) {
    S().say(String(e instanceof Error ? e.message : e), 'error');
  }
}

/** Start drawing on an empty grid, with no plan file. */
export async function startBlank() {
  const def = await loadDefaults();
  const prev = S().config;
  const extra = prev ? prev.catalog.filter((t) => !def.catalog.some((d) => d.id === t.id)) : [];
  S().set({
    drawing: null, background: null, hiddenLayers: [], roles: {}, mmPerUnit: 1, notes: [], step: 0, areaSeeds: [],
    plan: emptyPlan(), planMode: 'draw', areaM2: 0, layout: null, alternatives: [], variants: [], suggestions: null, capacity: null,
    scaleConfirmed: true, mainConfirmed: true, tool: 'drawArea', drawShape: 'poly', history: [], future: [], snapGrid: true,
    config: { ...def, catalog: [...def.catalog, ...extra] },
  });
}

/** Switch between drawing the plan by hand and detecting it from the drawing (remembered for the
 *  next plans). Drawing keeps what is there, to fix it by hand; `clear` starts empty. */
export async function setPlanMode(m: PlanMode, clear = false) {
  const s = S();
  savePlanMode(m);
  s.set({ planModePref: m });
  if (m === 'draw') {
    if (clear) {
      const p = s.plan;
      const has = !!p && (p.region.length > 0 || p.doors.length > 0 || p.obstacles.length > 0);
      if (has && !confirm('Delete the areas, doors and obstacles and start drawing from scratch? (Undo brings them back.)')) return;
      if (has) s.snapshot();
      s.set({ plan: emptyPlan(), areaM2: 0, layout: null, mainConfirmed: true, areaSeeds: [] });
    }
    s.set({ planMode: 'draw', tool: 'drawArea', mainConfirmed: true });
    return;
  }
  if (s.planMode === 'draw' && s.plan && (s.plan.region.length || s.plan.doors.length) &&
      !confirm('Detect the area from the drawing? What you drew will be replaced. (Undo brings it back.)')) return;
  s.snapshot();
  s.set({ planMode: 'detect', tool: 'select' });
  await analyzePlan(null);
}

/** After a change to the outlines: area, paths and layout out of date. */
function regionChanged(p: Plan) {
  p.paths = [];
  useApp.setState({ areaM2: regionArea(p.region) / 1e6, layout: null, alternatives: [], planMode: 'draw' });
}

/** Adds a closed outline drawn by hand to the areas to furnish. */
export function addDrawnArea(pts: Pt[]): boolean {
  const ring = tidyRing(pts);
  if (ring.length < 3) { S().say('An area needs at least 3 corners.', 'error'); return false; }
  if (selfCross(ring)) { S().say('The lines of the outline cross: move the last points (Backspace removes them).', 'error'); return false; }
  if (ringArea(ring) < 1e6) { S().say('That area is smaller than 1 m²: check the scale of the drawing or draw it bigger.', 'error'); return false; }
  let joined = 0, count = 0;
  S().updatePlan((p) => {
    const r = addAreaRing(p, ring);
    p.region = r.region;
    p.regionClear = r.regionClear;
    joined = r.joined;
    count = areaGroups(p.region).length;
    regionChanged(p);
  });
  S().say(joined ? `Joined with the area it overlaps · ${S().areaM2.toFixed(1)} m² in total.` : `Area ${count} added · ${(ringArea(ring) / 1e6).toFixed(1)} m². Draw another one, or the doors.`);
  return true;
}

/** Removes the area whose outline is `outer` (an index into plan.region). */
export function deleteArea(outer: number) {
  S().updatePlan((p) => {
    const r = removeAreaGroup(p, outer);
    p.region = r.region;
    p.regionClear = r.regionClear;
    regionChanged(p);
  });
}

/** New shape of one outline of the area (dragging a corner, adding or removing one). */
export function setAreaRing(i: number, pts: Pt[]): boolean {
  const ring = tidyRing(pts);
  if (ring.length < 3) { S().say('An area needs at least 3 corners.', 'error'); return false; }
  if (selfCross(ring)) { S().say('The outline would cross itself: the corner was not moved.', 'error'); return false; }
  S().updatePlan((p) => {
    p.region[i] = ring;
    if (p.regionClear?.[i]) p.regionClear[i] = [];
    regionChanged(p);
  });
  return true;
}

export function addDrawnObstacle(pts: Pt[], kind: ObstacleKind): boolean {
  const ring = tidyRing(pts);
  if (ring.length < 3) { S().say('An obstacle needs at least 3 corners.', 'error'); return false; }
  if (selfCross(ring)) { S().say('The lines cross: move the last points (Backspace removes them).', 'error'); return false; }
  if (ringArea(ring) < 1e4) { S().say('That obstacle is smaller than 10 × 10 cm.', 'error'); return false; }
  const label = kind === 'reserved' ? (prompt('Name of the reserved area (e.g. Printers):') ?? '') : '';
  S().updatePlan((p) => { p.obstacles.push({ id: uid('ob'), kind, label, poly: ring }); p.paths = []; });
  S().set({ layout: null });
  return true;
}

export function addDrawnDoor(kind: DoorKind, a: Pt, b: Pt): boolean {
  if (Math.hypot(b.x - a.x, b.y - a.y) < 200) { S().say('That door is shorter than 20 cm: click both ends of the opening.', 'error'); return false; }
  S().updatePlan((p) => {
    if (kind === 'main') p.doors.forEach((d) => { if (d.kind === 'main') d.kind = 'office'; });
    p.doors.push({ id: uid('d'), kind, a, b, label: '' });
    p.paths = [];
  });
  S().set({ layout: null, ...(kind === 'main' ? { mainConfirmed: true } : {}) });
  const w = (Math.hypot(b.x - a.x, b.y - a.y) / 1000).toFixed(2);
  S().say(kind === 'main' ? `Main entrance drawn (${w} m). Now the other doors: offices and exits.` : kind === 'exit' ? `Emergency exit drawn (${w} m).` : `Door drawn (${w} m).`);
  return true;
}

/** Cierra el plano actual y vuelve a la pantalla de inicio (conserva el catálogo y las medidas). */
export function closePlan() {
  const s = S();
  if (s.layout && !confirm('Close the plan? Any layout you have not saved will be lost.')) return;
  s.set({
    drawing: null, plan: null, layout: null, background: null, hiddenLayers: [], notes: [], areaM2: 0, roles: {},
    variants: [], suggestions: null, capacity: null, selection: { modules: [], obstacles: [], doors: [], windows: [] },
    history: [], future: [], step: 0, tool: 'select', calib: [], scaleConfirmed: false, mainConfirmed: true, alternatives: [], areaSeeds: [],
  });
  s.updateConfig((c) => { c.pinned = []; });
}

/** Borra del dibujo todas las líneas y objetos de una capa. */
export async function deleteLayer(name: string) {
  const s = S();
  const d = s.drawing;
  if (!d?.drawingId) return;
  const l = d.layers.find((x) => x.name === name);
  if (!l || !confirm(`Delete layer “${name}” (${l.paths} strokes) from the drawing? This cannot be undone (open the file again to get it back).`)) return;
  try {
    const n = await call<number>('delete_layer', { drawingId: d.drawingId, layer: name });
    const role = s.roles[name];
    const roles = { ...s.roles };
    delete roles[name];
    S().set({ drawing: { ...d, layers: d.layers.filter((x) => x.name !== name) }, roles, hiddenLayers: s.hiddenLayers.filter((h) => h !== name) });
    await loadBackground();
    // si la capa influía en el área (muros, puertas, columnas…), volver a detectar
    if (S().planMode === 'detect' && !['ignore', 'furniture', 'room', 'building'].includes(role)) await analyzePlan(null);
    S().say(`Layer “${name}” deleted (${n} items).`);
  } catch (e) {
    S().say(String(e instanceof Error ? e.message : e), 'error');
  }
}

export async function loadBackground() {
  const d = S().drawing;
  if (!d) return;
  const bg = await call<BgLayer[]>('background', { drawingId: d.drawingId, mmPerUnit: S().mmPerUnit });
  // capas ignoradas o de mobiliario, ocultas al inicio para que el plano se lea mejor
  let hidden = bg.filter((l) => S().roles[l.name] === 'ignore').map((l) => l.name);
  // nunca ocultar todo el dibujo (PDF sin capas)
  if (hidden.length >= bg.length) hidden = [];
  S().set({ background: bg, hiddenLayers: hidden });
}

/** Detects the area to furnish: automatically (no point), from one point, or from several
 *  points (one inside each area to use; areas that touch through a door are joined). */
// detections run one after another (on a large drawing each takes a few seconds): a click made
// while one runs is applied on top of its result, never on a stale one
let analyzeChain: Promise<unknown> = Promise.resolve();
function queued<T>(f: () => Promise<T>): Promise<T> {
  const next = analyzeChain.then(f, f);
  analyzeChain = next.catch(() => undefined);
  return next;
}
export function analyzePlan(seed: Pt | null, more?: Pt[] | (() => Pt[] | null)) {
  return queued(() => runAnalyze(seed, typeof more === 'function' ? more() : more));
}
async function runAnalyze(seed: Pt | null, more?: Pt[] | null) {
  const { drawing, mmPerUnit, roles, gapClose } = S();
  if (!drawing || more === null) return;
  const seeds = more ?? (seed ? [seed] : []);
  try {
    const r = await call<AnalyzeResult>('analyze', { drawingId: drawing.drawingId, options: { mmPerUnit, roles, seed: null, seeds, gapClose } });
    const auto = r.notes.some((n) => n.includes('widest'));
    // automatic: remember a point inside it, so more areas can be added to it
    const kept = seeds.length ? seeds : [interiorPoint(r.plan)].filter((p): p is Pt => !!p);
    S().set({ planMode: 'detect', plan: r.plan, areaSeeds: kept, notes: [drawing.scale.reason, ...r.notes], areaM2: r.areaM2, layout: null, alternatives: [], capacity: null, suggestions: null, variants: [], mainConfirmed: !auto });
  } catch (e) {
    S().say(String(e instanceof Error ? e.message : e), 'error');
    S().set({ notes: [drawing.scale.reason, String(e instanceof Error ? e.message : e)] });
  }
}

const pipA = (p: Pt, poly: Pt[]) => {
  let c = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const a = poly[i], b = poly[j];
    if ((a.y > p.y) !== (b.y > p.y) && p.x < ((b.x - a.x) * (p.y - a.y)) / (b.y - a.y) + a.x) c = !c;
  }
  return c;
};
/** Which outline of the area contains the point (-1 = none). */
export function areaRingAt(plan: Plan, p: Pt): number {
  const inObst = plan.obstacles.some((o) => pipA(p, o.poly));
  if (inObst) return -1;
  return plan.region.findIndex((r) => pipA(p, r));
}
function interiorPoint(plan: Plan): Pt | null {
  const r = plan.region[0];
  if (!r) return null;
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const q of r) { x0 = Math.min(x0, q.x); y0 = Math.min(y0, q.y); x1 = Math.max(x1, q.x); y1 = Math.max(y1, q.y); }
  // the point deepest inside (farthest from the outline), on a 24×24 grid
  const segD = (p: Pt, a: Pt, b: Pt) => {
    const dx = b.x - a.x, dy = b.y - a.y, l2 = dx * dx + dy * dy || 1;
    const t = Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / l2));
    return Math.hypot(p.x - a.x - dx * t, p.y - a.y - dy * t);
  };
  let best: Pt | null = null, bd = -1;
  for (let i = 1; i < 24; i++) for (let j = 1; j < 24; j++) {
    const p = { x: x0 + ((x1 - x0) * i) / 24, y: y0 + ((y1 - y0) * j) / 24 };
    if (areaRingAt(plan, p) !== 0) continue;
    const d = Math.min(...r.map((q, k) => segD(p, q, r[(k + 1) % r.length])));
    if (d > bd) { bd = d; best = p; }
  }
  return best;
}

/** Adds the area under the point to the areas to furnish. */
export function addArea(p: Pt) {
  return analyzePlan(null, () => {
    const s = S();
    if (s.plan && areaRingAt(s.plan, p) >= 0) { s.say('That area is already included.'); return null; }
    return [...s.areaSeeds, p];
  });
}

/** Leaves out the area under the point. */
export function removeArea(p: Pt) {
  return analyzePlan(null, () => seedsWithout(p));
}
function seedsWithout(p: Pt): Pt[] | null {
  const s = S();
  if (!s.plan) return null;
  const ri = areaRingAt(s.plan, p);
  if (ri < 0) { s.say('Click inside one of the chosen areas (shaded) to leave it out.'); return null; }
  const ring = s.plan.region[ri];
  // the chosen point closest to the click, among those inside that outline (two rooms joined by a
  // door share one outline: only the clicked one goes)
  const inRing = s.areaSeeds.filter((q) => pipA(q, ring));
  if (!inRing.length) { s.say('Choose the areas one by one with “Click inside the area” and “Add area” to be able to leave some out.'); return null; }
  const gone = inRing.reduce((b, q) => (Math.hypot(q.x - p.x, q.y - p.y) < Math.hypot(b.x - p.x, b.y - p.y) ? q : b));
  const left = s.areaSeeds.filter((q) => q !== gone);
  if (!left.length) { s.say('At least one area must stay. Add another one first.', 'error'); return null; }
  return left;
}

export async function setScale(mm: number) {
  if (!(mm > 0)) return;
  const s = S();
  const old = s.mmPerUnit;
  s.set({ mmPerUnit: mm });
  await loadBackground();
  if (s.planMode === 'draw') {
    // what was drawn stays on the same lines of the drawing
    const p = S().plan;
    if (p && old > 0 && Math.abs(mm / old - 1) > 1e-9) {
      const np = scalePlan(p, mm / old);
      S().set({ plan: np, areaM2: regionArea(np.region) / 1e6, layout: null });
    }
    return;
  }
  await analyzePlan(null);
}

export function problem(): Problem | null {
  const { plan, config } = S();
  if (!plan || !config) return null;
  return { plan, config };
}

/** Calculates the circulation paths on the empty floor. Paths edited by hand are kept as they are;
 *  the others are replaced, keeping whether they were kept and their own width. */
export async function computePaths(onlyId?: string) {
  const p = problem();
  if (!p) return;
  if (!p.plan.doors.some((d) => d.kind === 'main')) { S().say('Mark the main entrance first (step 1).', 'error'); return; }
  try {
    const fresh = await call<CircPath[]>('circulation', { problem: p });
    const old = new Map((S().plan?.paths ?? []).map((x) => [x.id, x]));
    let merged: CircPath[];
    if (onlyId) {
      const f = fresh.find((x) => x.id === onlyId);
      merged = (S().plan?.paths ?? []).map((x) => (x.id === onlyId && f ? { ...f, keep: x.keep, width: x.width } : x));
    } else {
      merged = fresh.map((f) => { const o = old.get(f.id); return !o ? f : o.edited ? o : { ...f, keep: o.keep, width: o.width }; });
    }
    S().updatePlan((pl) => { pl.paths = merged; });
    S().set({ pathSel: onlyId ? { id: onlyId, node: null } : null, layout: null, alternatives: [] });
    if (!onlyId) {
      const np = merged.filter((x) => x.kind === 'primary').length, ns = merged.filter((x) => x.kind === 'secondary').length;
      S().say(`${np} entrance path${np === 1 ? '' : 's'} and ${ns} office-to-office path${ns === 1 ? '' : 's'} found.`);
    }
  } catch (e) {
    S().say('Could not calculate the paths: ' + e, 'error');
  }
}

function startJobUi(kind: string) {
  const signal = { cancel: false };
  S().set({ job: { kind, progress: null, signal } });
  return signal;
}

export async function optimize(resume = false) {
  const p0 = problem();
  if (!p0) return;
  // tiempo automático (0) y, con “Seguir buscando”, el genoma de la solución actual
  const p = { ...p0, config: { ...p0.config, settings: { ...p0.config.settings, timeLimit: 0, resume: resume ? S().layout?.genome ?? null : null } } };
  if (!hasDemand(p.config)) {
    S().say('Say how many desks to place, or turn on “As many as fit”, in the “Desks” step.', 'error');
    S().set({ step: STEP.desks });
    return;
  }
  const signal = startJobUi('Optimizing');
  try {
    const res = await runJob<Layout>('optimize', p, (st) => S().set({ job: { kind: 'Optimizing', progress: st.progress, signal } }), { signal });
    const alts = (res.alternatives && res.alternatives.length ? res.alternatives : [res]).map((l) => ({ ...l, alternatives: undefined }));
    const layout = alts[0];
    S().set({ layout, alternatives: alts, suggestions: null, selection: { modules: [], obstacles: [], doors: [], windows: [] }, reveal: S().reveal + 1 });
    const k = layout.kpis;
    if (k.unplacedByType.length > 0) {
      S().say('Not all the fixed desks fit. Try “Find better values”.', 'error');
      S().set({ job: null });
    } else {
      S().say(k.maxMode ? `Done: ${k.seatsPlaced} seats fit.` : `Done: ${k.seatsPlaced} seats placed.`);
      S().set({ job: null });
    }
  } catch (e) {
    S().say(String(e instanceof Error ? e.message : e), 'error');
  } finally {
    if (S().job?.signal === signal) S().set({ job: null });
  }
}

export async function explore(n = 10) {
  const p = problem();
  if (!p) return;
  const signal = startJobUi('Exploring weights');
  try {
    const vars = await runJob<Variant[]>('explore', p, (st) => S().set({ job: { kind: 'Exploring weights', progress: st.progress, signal } }), { n, signal });
    S().set({ variants: vars });
    S().say(`${vars.length} variants; those on the first Pareto front cannot improve one indicator without worsening another.`);
  } catch (e) {
    S().say(String(e instanceof Error ? e.message : e), 'error');
  } finally {
    S().set({ job: null });
  }
}

export async function suggest() {
  const p = problem();
  if (!p) return;
  const signal = startJobUi('Looking for suggestions');
  try {
    const sg = await runJob<Suggestion[]>('suggest', p, (st) => S().set({ job: { kind: 'Looking for suggestions', progress: st.progress, signal } }), { signal });
    S().set({ suggestions: sg });
  } catch (e) {
    S().say(String(e instanceof Error ? e.message : e), 'error');
  } finally {
    S().set({ job: null });
  }
}

export async function capacity() {
  const p = problem();
  if (!p) return;
  const signal = startJobUi('Estimating capacity');
  try {
    const c = await runJob<Capacity[]>('capacity', p, (st) => S().set({ job: { kind: 'Estimating capacity', progress: st.progress, signal } }), { signal });
    S().set({ capacity: c });
  } catch (e) {
    S().say(String(e instanceof Error ? e.message : e), 'error');
  } finally {
    S().set({ job: null });
  }
}

export function applySuggestion(s: Suggestion) {
  S().updateConfig((c) => {
    if (s.ruleId === 'TIPO') {
      const [from, to] = s.itemId.split('>');
      const tf = c.catalog.find((t) => t.id === from)!;
      const tt = c.catalog.find((t) => t.id === to)!;
      const d = c.demand.find((d) => d.typeId === from);
      if (!d || !tf || !tt) return;
      const seats = tf.flexible ? d.quantity : d.quantity * (tf.shape === 'l' ? 1 : tf.sides * tf.perSide);
      const per = tt.flexible ? 1 : tt.shape === 'l' ? 1 : tt.sides * tt.perSide;
      const q = Math.ceil(seats / per);
      c.demand = c.demand.filter((x) => x.typeId !== from);
      const ex = c.demand.find((x) => x.typeId === to);
      if (ex) ex.quantity += q; else c.demand.push({ typeId: to, quantity: q });
      return;
    }
    const r = c.rules.find((r) => r.id === s.ruleId);
    if (!r) return;
    if (!s.itemId) {
      r.enabled = !s.disable;
      return;
    }
    const it = r.items.find((i) => i.id === s.itemId);
    if (!it) return;
    if (s.newValue !== null) it.value = s.newValue;
    else it.enabled = !s.disable;
  });
  S().set({ suggestions: (S().suggestions ?? []).filter((x) => x !== s) });
  if (s.layout) {
    const l = { ...s.layout, alternatives: undefined };
    S().set({ layout: l, alternatives: [l], selection: { modules: [], obstacles: [], doors: [], windows: [] }, reveal: S().reveal + 1 });
    S().say(`Value applied: ${l.kpis.seatsPlaced} seats. You can optimize again with more time to refine.`);
  } else {
    S().say('Value applied. Optimize again to see the result.');
  }
}

/** Muestra una de las mejores soluciones de la última optimización. */
export function chooseAlternative(i: number) {
  const a = S().alternatives[i];
  if (!a) return;
  S().set({ layout: a, selection: { modules: [], obstacles: [], doors: [], windows: [] }, reveal: S().reveal + 1 });
  if (S().config?.org.teams.some((t) => t.count > 0)) reassign();
}

/** Coloca un escritorio a mano (queda fijado: la optimización lo respeta y descuenta de la cantidad). */
export async function placeDesk(center: Pt, forced: string[] = []) {
  const s = S();
  const c = s.config;
  const t = c?.catalog.find((x) => x.id === s.placeType);
  if (!c || !t || !s.plan) return;
  const { per, length, depth } = deskDims(t);
  const m: Module = {
    id: uid('mano'), typeId: t.id, center, angle: s.placeAngle, length, depth, perSide: per, seatsUp: s.placeUp,
    body: deskBody(t, center, s.placeAngle, s.placeUp).body, pinned: true, people: [],
  };
  s.snapshot();
  const base: Layout = s.layout ?? {
    modules: [], seats: [], corridors: [], keepouts: [], violations: [], weights: c.weights,
    kpis: { maxSeats: 0, seatsRequested: 0, seatsPlaced: 0, modulesRequested: 0, modulesPlaced: 0, unplacedByType: [], areaM2: s.areaM2, m2PerSeat: 0, avgTravelExitM: 0, maxTravelExitM: 0, daylightPct: 0, windowSidePct: 0, privacyIssues: 0, orientations: 0, reservePct: 0, reserveSeats: 0, accessiblePmr: 0, hardViolations: 0, groupingScore: 0, misplacedPeople: 0, corridorAreaM2: 0, score: 0 },
  };
  const layout: Layout = { ...base, modules: [...base.modules, m] };
  S().updateConfig((cc) => { cc.pinned = [...cc.pinned, m]; });
  try {
    const nl = await call<Layout>('evaluate', { problem: { plan: s.plan, config: S().config }, layout });
    S().set({ layout: nl });
    const flags = [...new Set([...forced, ...nl.seats.filter((x) => x.moduleId === m.id).flatMap((x) => x.flags)])];
    if (flags.length) S().say(`Placed, but it breaks: ${flags.join('; ')}. Ctrl+Z removes it.`, 'error');
    else S().say(`Seat added: ${nl.kpis.seatsPlaced} seats.`);
  } catch (e) {
    S().say(String(e instanceof Error ? e.message : e), 'error');
  }
}

export async function reassign() {
  const p = problem();
  const l = S().layout;
  if (!p || !l) return;
  try {
    const nl = await call<Layout>('reassign', { problem: p, layout: l });
    S().set({ layout: nl, colorBy: S().colorBy === 'type' ? 'client' : S().colorBy });
  } catch (e) {
    S().say(String(e instanceof Error ? e.message : e), 'error');
  }
}

const baseName = () => (S().drawing?.name ?? 'oficina').replace(/\.[a-z0-9]+$/i, '');

export async function exportDxf(includePlan: boolean) {
  const p = problem();
  const l = S().layout;
  if (!p || !l) return;
  // en DXF/DWG se exporta en las unidades del archivo original para poder superponerlo
  const src = S().drawing?.source;
  const mm = src === 'pdf' ? 1 : S().mmPerUnit;
  const txt = await call<string>('export_dxf', { problem: p, layout: l, mmPerUnit: mm, includePlan });
  const path = await saveText(`${baseName()}_seats.dxf`, txt, 'dxf');
  if (path) S().say(`Saved: ${path}`);
}

export async function exportCsv() {
  const p = problem();
  const l = S().layout;
  if (!p || !l) return;
  const txt = await call<string>('export_csv', { problem: p, layout: l, mmPerUnit: 1 });
  const path = await saveText(`${baseName()}_seats.csv`, '﻿' + txt, 'csv');
  if (path) S().say(`Saved: ${path}`);
}

export async function saveProject() {
  const s = S();
  const pf: ProjectFile = { version: 1, drawingName: s.drawing?.name ?? '', mmPerUnit: s.mmPerUnit, roles: s.roles, plan: s.plan, config: s.config, layout: s.layout, background: s.background };
  const path = await saveText(`${baseName()}.seats.json`, JSON.stringify(pf), 'json');
  if (path) S().say(`Project saved: ${path}`);
}

export async function openProject() {
  try {
    const txt = await openText('json');
    if (!txt) return;
    const pf = JSON.parse(txt) as ProjectFile;
    const def = await loadDefaults();
    const config = pf.config ? { ...def, ...pf.config } : def;
    // textos de las reglas siempre los de esta versión (los valores son los del proyecto)
    if (pf.config) {
      config.rules = config.rules.map((r) => {
        const d = def.rules.find((x) => x.id === r.id);
        if (!d) return r;
        return { ...r, name: d.name, description: d.description, items: r.items.map((it) => { const di = d.items.find((x) => x.id === it.id); return di ? { ...it, label: di.label, help: di.help } : it; }) };
      });
      // rules added in newer versions (e.g. circulation paths) come with their defaults
      for (const d of def.rules) if (!config.rules.some((r) => r.id === d.id)) config.rules.push(d);
      config.weights = { ...def.weights, ...config.weights };
    }
    S().set({
      drawing: pf.drawingName ? { drawingId: '', name: pf.drawingName, source: 'dxf', layers: [], scale: { mmPerUnit: pf.mmPerUnit, paperScale: null, reason: '' }, bounds: [0, 0, 0, 0], unitNote: '' } : null,
      mmPerUnit: pf.mmPerUnit, roles: pf.roles ?? {}, plan: pf.plan, config, layout: pf.layout, background: pf.background, hiddenLayers: [],
      step: pf.layout ? 4 : pf.plan ? 1 : 0, variants: [], suggestions: null, capacity: null,
      notes: ['Project opened. To detect the area again or change layers, import the plan again.'],
      planMode: 'draw', areaM2: pf.plan ? regionArea(pf.plan.region) / 1e6 : 0,
    });
  } catch (e) {
    S().say('Could not open the project: ' + String(e instanceof Error ? e.message : e), 'error');
  }
}

/** PDF del layout, del plano simplificado (clean) o del dibujo completo sin mobiliario (clean + full). */
export async function exportPdf(o: { clean?: boolean; full?: boolean }) {
  const s = S();
  const clean = !!o.clean;
  const full = !!o.full;
  if (!clean && !s.layout) return;
  const title = full ? `${baseName()} — plan without furniture` : clean ? `${baseName()} — plan for optimization` : `${baseName()} — seating layout`;
  const furniture = (s.drawing?.layers ?? []).filter((l) => s.roles[l.name] === 'furniture').map((l) => l.name);
  const hidden = Array.from(new Set([...s.hiddenLayers, ...furniture]));
  const p = problem();
  try {
    const b64 = await call<string>('export_pdf', {
      problem: full ? null : p,
      layout: clean ? null : s.layout,
      drawingId: s.drawing?.drawingId ?? '',
      mmPerUnit: s.mmPerUnit,
      opts: {
        paper: s.pdfPaper, scale: s.pdfScale > 0 ? s.pdfScale : null, title, colorBy: s.colorBy,
        background: full || (!clean && s.pdfBackground), hiddenLayers: full ? furniture : hidden, clean, numbers: true,
      },
    });
    const suffix = full ? 'no_furniture' : clean ? 'base_plan' : 'seats';
    const path = await saveBase64(`${baseName()}_${suffix}_${s.pdfPaper}.pdf`, b64, 'pdf');
    if (path) S().say(`Saved: ${path}`);
  } catch (e) {
    S().say(String(e instanceof Error ? e.message : e), 'error');
  }
}

/** DXF del plano simplificado, con capas estándar (en mm, o en las unidades del DXF original). */
export async function exportPlanDxf() {
  const s = S();
  if (!s.plan) return;
  const outMm = s.drawing?.source === 'pdf' || !s.drawing ? 1 : s.mmPerUnit;
  try {
    const txt = await call<string>('export_plan_dxf', { plan: s.plan, outMm });
    const path = await saveText(`${baseName()}_base_plan.dxf`, txt, 'dxf');
    if (path) S().say(`Saved: ${path}`);
  } catch (e) {
    S().say(String(e instanceof Error ? e.message : e), 'error');
  }
}

export async function exportCleanDxf() {
  const s = S();
  if (!s.drawing?.drawingId) return;
  const exclude = s.drawing.layers.filter((l) => s.roles[l.name] === 'furniture').map((l) => l.name);
  try {
    const txt = await call<string>('export_clean_dxf', { drawingId: s.drawing.drawingId, mmPerUnit: s.mmPerUnit, exclude });
    const path = await saveText(`${baseName()}_no_furniture.dxf`, txt, 'dxf');
    if (path) S().say(`Saved: ${path} (${exclude.length} furniture layers removed)`);
  } catch (e) {
    S().say(String(e instanceof Error ? e.message : e), 'error');
  }
}

/** Plano de líneas simples del dibujo original: muros a eje, columnas, puertas y oficinas. */
export async function exportSimple(format: 'dxf' | 'pdf') {
  const s = S();
  if (!s.drawing?.drawingId || !s.plan) return;
  const args = { drawingId: s.drawing.drawingId, options: { mmPerUnit: s.mmPerUnit, roles: s.roles, seed: null, gapClose: s.gapClose }, plan: s.plan, format, paper: s.pdfPaper, title: `${baseName()} — single lines` };
  const job = startJobUi('Simplifying the drawing');
  try {
    const out = await call<string>('export_simple', args);
    S().set({ job: null });
    const path = format === 'dxf'
      ? await saveText(`${baseName()}_single_lines.dxf`, out, 'dxf')
      : await saveBase64(`${baseName()}_single_lines_${s.pdfPaper}.pdf`, out, 'pdf');
    if (path) S().say(`Saved: ${path}`);
  } catch (e) {
    S().set({ job: null });
    S().say(String(e instanceof Error ? e.message : e), 'error');
  }
  void job;
}

/** Removes the selected interior node of the path being edited. */
export function removePathNode() {
  const sel = S().pathSel;
  if (!sel || sel.node === null) return;
  const p = S().plan?.paths?.find((x) => x.id === sel.id);
  if (!p || sel.node <= 0 || sel.node >= p.pts.length - 1) { S().say('The ends of a path stay on their doors.'); return; }
  const k = sel.node;
  S().updatePlan((pl) => { const q = pl.paths?.find((x) => x.id === sel.id); if (q) { q.pts.splice(k, 1); q.edited = true; } });
  S().set({ pathSel: { id: sel.id, node: null }, layout: null });
}
