import { deskDims } from './clear';
import { call, openPlan, openText, runJob, saveBase64, saveText } from './api';
import { hasDemand, uid, useApp, type ProjectFile } from './store';
import type {
  AnalyzeResult, BgLayer, Capacity, Config, Defaults, Layout, Module, Problem, Pt, Suggestion, Variant,
} from './types';

const S = () => useApp.getState();

export async function loadDefaults(): Promise<Config> {
  const d = await call<Defaults>('defaults');
  return { catalog: d.catalog, demand: [], rules: d.rules, customRules: [], weights: d.weights, org: { roles: [], clients: [], teams: [], affinities: [] }, settings: { ...d.settings, timeLimit: 10 }, pinned: [] };
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
    S().set({ scaleConfirmed: false, tool: 'select', drawing: out, roles, mmPerUnit: out.scale.mmPerUnit, plan: null, layout: null, alternatives: [], variants: [], suggestions: null, capacity: null, background: null, hiddenLayers: [], step: 0, notes: [out.scale.reason] });
    await ensureConfig();
    await loadBackground();
    await analyzePlan(null);
  } catch (e) {
    S().say(String(e instanceof Error ? e.message : e), 'error');
  }
}

/** Cierra el plano actual y vuelve a la pantalla de inicio (conserva el catálogo y las medidas). */
export function closePlan() {
  const s = S();
  if (s.layout && !confirm('¿Cerrar el plano? La distribución que no hayas guardado se pierde.')) return;
  s.set({
    drawing: null, plan: null, layout: null, background: null, hiddenLayers: [], notes: [], areaM2: 0, roles: {},
    variants: [], suggestions: null, capacity: null, selection: { modules: [], obstacles: [], doors: [], windows: [] },
    history: [], future: [], step: 0, tool: 'select', calib: [], scaleConfirmed: false, mainConfirmed: true, alternatives: [],
  });
  s.updateConfig((c) => { c.pinned = []; });
}

/** Borra del dibujo todas las líneas y objetos de una capa. */
export async function deleteLayer(name: string) {
  const s = S();
  const d = s.drawing;
  if (!d?.drawingId) return;
  const l = d.layers.find((x) => x.name === name);
  if (!l || !confirm(`¿Borrar la capa «${name}» (${l.paths} trazos) del dibujo? No se puede deshacer (vuelve a abrir el archivo para recuperarla).`)) return;
  try {
    const n = await call<number>('delete_layer', { drawingId: d.drawingId, layer: name });
    const role = s.roles[name];
    const roles = { ...s.roles };
    delete roles[name];
    S().set({ drawing: { ...d, layers: d.layers.filter((x) => x.name !== name) }, roles, hiddenLayers: s.hiddenLayers.filter((h) => h !== name) });
    await loadBackground();
    // si la capa influía en el área (muros, puertas, columnas…), volver a detectar
    if (!['ignore', 'furniture', 'room', 'building'].includes(role)) await analyzePlan(null);
    S().say(`Capa «${name}» borrada (${n} elementos).`);
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

export async function analyzePlan(seed: Pt | null) {
  const { drawing, mmPerUnit, roles, gapClose } = S();
  if (!drawing) return;
  try {
    const r = await call<AnalyzeResult>('analyze', { drawingId: drawing.drawingId, options: { mmPerUnit, roles, seed, gapClose } });
    const auto = r.notes.some((n) => n.includes('más ancha'));
    S().set({ plan: r.plan, notes: [drawing.scale.reason, ...r.notes], areaM2: r.areaM2, layout: null, alternatives: [], capacity: null, suggestions: null, variants: [], mainConfirmed: !auto });
  } catch (e) {
    S().say(String(e instanceof Error ? e.message : e), 'error');
    S().set({ notes: [drawing.scale.reason, String(e instanceof Error ? e.message : e)] });
  }
}

export async function setScale(mm: number) {
  if (!(mm > 0)) return;
  S().set({ mmPerUnit: mm });
  await loadBackground();
  await analyzePlan(null);
}

export function problem(): Problem | null {
  const { plan, config } = S();
  if (!plan || !config) return null;
  return { plan, config };
}

function startJobUi(kind: string) {
  const signal = { cancel: false };
  S().set({ job: { kind, progress: null, signal } });
  return signal;
}

export async function optimize(resume = false) {
  const p0 = problem();
  if (!p0) return;
  // tiempo automático (0) y, con «Seguir buscando», el genoma de la solución actual
  const p = { ...p0, config: { ...p0.config, settings: { ...p0.config.settings, timeLimit: 0, resume: resume ? S().layout?.genome ?? null : null } } };
  if (!hasDemand(p.config)) {
    S().say('Indica cuántos escritorios ubicar, o marca «Máximo posible», en el paso «Escritorios».', 'error');
    S().set({ step: 1 });
    return;
  }
  const signal = startJobUi('Optimizando');
  try {
    const res = await runJob<Layout>('optimize', p, (st) => S().set({ job: { kind: 'Optimizando', progress: st.progress, signal } }), { signal });
    const alts = (res.alternatives && res.alternatives.length ? res.alternatives : [res]).map((l) => ({ ...l, alternatives: undefined }));
    const layout = alts[0];
    S().set({ layout, alternatives: alts, suggestions: null, selection: { modules: [], obstacles: [], doors: [], windows: [] }, reveal: S().reveal + 1 });
    const k = layout.kpis;
    if (k.unplacedByType.length > 0) {
      S().say('No caben todos los escritorios con cantidad fija. Prueba «Buscar mejores valores».', 'error');
      S().set({ job: null });
    } else {
      S().say(k.maxMode ? `Listo: caben ${k.seatsPlaced} puestos.` : `Listo: ${k.seatsPlaced} puestos ubicados.`);
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
  const signal = startJobUi('Explorando pesos');
  try {
    const vars = await runJob<Variant[]>('explore', p, (st) => S().set({ job: { kind: 'Explorando pesos', progress: st.progress, signal } }), { n, signal });
    S().set({ variants: vars });
    S().say(`${vars.length} variantes; las del primer frente de Pareto no se pueden mejorar en un indicador sin empeorar otro.`);
  } catch (e) {
    S().say(String(e instanceof Error ? e.message : e), 'error');
  } finally {
    S().set({ job: null });
  }
}

export async function suggest() {
  const p = problem();
  if (!p) return;
  const signal = startJobUi('Buscando sugerencias');
  try {
    const sg = await runJob<Suggestion[]>('suggest', p, (st) => S().set({ job: { kind: 'Buscando sugerencias', progress: st.progress, signal } }), { signal });
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
  const signal = startJobUi('Estimando capacidad');
  try {
    const c = await runJob<Capacity[]>('capacity', p, (st) => S().set({ job: { kind: 'Estimando capacidad', progress: st.progress, signal } }), { signal });
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
    S().say(`Medida aplicada: ${l.kpis.seatsPlaced} puestos. Puedes optimizar de nuevo con más tiempo para afinar.`);
  } else {
    S().say('Medida aplicada. Vuelve a optimizar para ver el resultado.');
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
  const a = (s.placeAngle * Math.PI) / 180;
  const u = { x: Math.cos(a), y: Math.sin(a) }, v = { x: -u.y, y: u.x };
  const w = (p: number, q: number) => ({ x: center.x + u.x * p + v.x * q, y: center.y + u.y * p + v.y * q });
  const m: Module = {
    id: uid('mano'), typeId: t.id, center, angle: s.placeAngle, length, depth, perSide: per, seatsUp: s.placeUp,
    body: [w(-length / 2, -depth / 2), w(length / 2, -depth / 2), w(length / 2, depth / 2), w(-length / 2, depth / 2)], pinned: true, people: [],
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
    if (flags.length) S().say(`Ubicado, pero no cumple: ${flags.join('; ')}. Ctrl+Z para quitarlo.`, 'error');
    else S().say(`Puesto agregado: ${nl.kpis.seatsPlaced} puestos.`);
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
  const path = await saveText(`${baseName()}_puestos.dxf`, txt, 'dxf');
  if (path) S().say(`Guardado: ${path}`);
}

export async function exportCsv() {
  const p = problem();
  const l = S().layout;
  if (!p || !l) return;
  const txt = await call<string>('export_csv', { problem: p, layout: l, mmPerUnit: 1 });
  const path = await saveText(`${baseName()}_puestos.csv`, '﻿' + txt, 'csv');
  if (path) S().say(`Guardado: ${path}`);
}

export async function saveProject() {
  const s = S();
  const pf: ProjectFile = { version: 1, drawingName: s.drawing?.name ?? '', mmPerUnit: s.mmPerUnit, roles: s.roles, plan: s.plan, config: s.config, layout: s.layout, background: s.background };
  const path = await saveText(`${baseName()}.puestos.json`, JSON.stringify(pf), 'json');
  if (path) S().say(`Proyecto guardado: ${path}`);
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
    }
    S().set({
      drawing: pf.drawingName ? { drawingId: '', name: pf.drawingName, source: 'dxf', layers: [], scale: { mmPerUnit: pf.mmPerUnit, paperScale: null, reason: '' }, bounds: [0, 0, 0, 0], unitNote: '' } : null,
      mmPerUnit: pf.mmPerUnit, roles: pf.roles ?? {}, plan: pf.plan, config, layout: pf.layout, background: pf.background, hiddenLayers: [],
      step: pf.layout ? 4 : pf.plan ? 1 : 0, variants: [], suggestions: null, capacity: null,
      notes: ['Proyecto abierto. Para volver a detectar el recinto o cambiar capas, vuelve a importar el plano.'],
    });
  } catch (e) {
    S().say('No se pudo abrir el proyecto: ' + String(e instanceof Error ? e.message : e), 'error');
  }
}

/** PDF del layout, del plano simplificado (clean) o del dibujo completo sin mobiliario (clean + full). */
export async function exportPdf(o: { clean?: boolean; full?: boolean }) {
  const s = S();
  const clean = !!o.clean;
  const full = !!o.full;
  if (!clean && !s.layout) return;
  const title = full ? `${baseName()} — planta sin mobiliario` : clean ? `${baseName()} — planta para optimización` : `${baseName()} — distribución de puestos`;
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
    const suffix = full ? 'sin_mobiliario' : clean ? 'plano_base' : 'puestos';
    const path = await saveBase64(`${baseName()}_${suffix}_${s.pdfPaper}.pdf`, b64, 'pdf');
    if (path) S().say(`Guardado: ${path}`);
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
    const path = await saveText(`${baseName()}_plano_base.dxf`, txt, 'dxf');
    if (path) S().say(`Guardado: ${path}`);
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
    const path = await saveText(`${baseName()}_sin_mobiliario.dxf`, txt, 'dxf');
    if (path) S().say(`Guardado: ${path} (${exclude.length} capas de mobiliario quitadas)`);
  } catch (e) {
    S().say(String(e instanceof Error ? e.message : e), 'error');
  }
}

/** Plano de líneas simples del dibujo original: muros a eje, columnas, puertas y oficinas. */
export async function exportSimple(format: 'dxf' | 'pdf') {
  const s = S();
  if (!s.drawing?.drawingId || !s.plan) return;
  const args = { drawingId: s.drawing.drawingId, options: { mmPerUnit: s.mmPerUnit, roles: s.roles, seed: null, gapClose: s.gapClose }, plan: s.plan, format, paper: s.pdfPaper, title: `${baseName()} — líneas simples` };
  const job = startJobUi('Simplificando el dibujo');
  try {
    const out = await call<string>('export_simple', args);
    S().set({ job: null });
    const path = format === 'dxf'
      ? await saveText(`${baseName()}_lineas_simples.dxf`, out, 'dxf')
      : await saveBase64(`${baseName()}_lineas_simples_${s.pdfPaper}.pdf`, out, 'pdf');
    if (path) S().say(`Guardado: ${path}`);
  } catch (e) {
    S().set({ job: null });
    S().say(String(e instanceof Error ? e.message : e), 'error');
  }
  void job;
}
