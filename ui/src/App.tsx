import { useEffect, type CSSProperties } from 'react';
import { applyPalette, PALETTES } from './palette';
import { addArea, analyzePlan, closePlan, ensureConfig, importPlan, openProject, removeArea, removePathNode, saveProject, setScale, startBlank } from './actions';
import { planReady } from './draw';
import { backendName, isWeb } from './api';
import Canvas from './components/Canvas';
import DeskStudio from './components/DeskStudio';
import { Bar } from './components/ui';
import { deleteSelection, evaluateSoon, moveSelection, pinSelection, rotateSelection, selectAll } from './edit';
import { emptySel, requestedSeats, savePlanMode, selCount, STEP, useApp, type Theme, type Tool } from './store';
import DesksStep from './steps/DesksStep';
import OptimizeStep from './steps/OptimizeStep';
import OrgStep from './steps/OrgStep';
import PathsStep from './steps/PathsStep';
import PlanStep from './steps/PlanStep';
import RulesStep from './steps/RulesStep';
import type { Pt } from './types';

const STEPS = ['Plan', 'Circulation', 'Desks', 'Rules', 'Organization', 'Optimize'];

/** Qué hacer con cada herramienta: se muestra sobre el plano. */
const TOOL_HELP: Partial<Record<Tool, string>> = {
  seed: 'Click inside the area you are going to furnish (Shift+click adds another area)',
  seedAdd: 'Click inside each area you want to add · Esc finishes',
  seedRemove: 'Click inside a shaded area to leave it out · Esc finishes',
  mainDoor: 'Click the staff entrance door (colored lines on the edge)',
  exitDoor: 'Click the emergency exits',
  door: 'Each click on a door changes its type: office → exit → main → ignored',
  calibrate: 'Click both ends of a length you know (a wall, a door)',
  column: 'Drag to draw the column',
  fixedOffice: 'Drag to draw the office or fixed room',
  restricted: 'Drag over the panel or cabinet',
  reserved: 'Drag to reserve the area',
  window: 'Drag along the window',
  erase: 'Click what you want to delete',
  pan: 'Drag to move the plan',
  placeDesk: 'Click to place the desk · R rotates 90° · F flips the chair side · Esc finishes',
};

export default function App() {
  const st = useApp();
  useEffect(() => {
    ensureConfig().catch((e) => st.say('Could not connect to the engine: ' + e, 'error'));
    const esc = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement;
      if (t && (t.tagName === 'INPUT' || t.tagName === 'SELECT' || t.tagName === 'TEXTAREA')) return;
      const s = useApp.getState();
      const ctrl = e.ctrlKey || e.metaKey;
      if (e.key === 'Escape') s.set({ tool: 'select', calib: [], selection: emptySel(), pathSel: null });
      else if ((e.key === 'Delete' || e.key === 'Backspace') && s.pathSel?.node != null) { e.preventDefault(); removePathNode(); }
      else if ((e.key === 'Delete' || e.key === 'Backspace') && selCount(s.selection)) { e.preventDefault(); deleteSelection(); }
      else if (ctrl && (e.key.toLowerCase() === 'y' || (e.shiftKey && e.key.toLowerCase() === 'z'))) { e.preventDefault(); s.redo(); evaluateSoon(); }
      else if (ctrl && e.key.toLowerCase() === 'z') { e.preventDefault(); s.undo(); evaluateSoon(); }
      else if (ctrl && e.key.toLowerCase() === 'a') { e.preventDefault(); selectAll(); }
      else if (!ctrl && s.tool === 'placeDesk' && e.key.toLowerCase() === 'r') s.set({ placeAngle: (s.placeAngle + 90) % 360 });
      else if (!ctrl && s.tool === 'placeDesk' && e.key.toLowerCase() === 'f') s.set({ placeUp: !s.placeUp });
      else if (!ctrl && e.key.toLowerCase() === 'r' && selCount(s.selection)) rotateSelection(90);
      else if (!ctrl && e.key.toLowerCase() === 'p' && s.selection.modules.length) pinSelection(true);
      else if (!ctrl && e.key.startsWith('Arrow') && selCount(s.selection)) {
        e.preventDefault();
        const step = e.shiftKey ? 500 : 100;
        const d = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, step], ArrowDown: [0, -step] }[e.key] ?? [0, 0];
        moveSelection(d[0], d[1]);
      }
    };
    window.addEventListener('keydown', esc);
    return () => window.removeEventListener('keydown', esc);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // si se edita el plano con un resultado en pantalla, recalcular sus avisos
  const planRef = st.plan;
  useEffect(() => {
    if (useApp.getState().layout && planRef) evaluateSoon();
  }, [planRef]);

  // tema claro / oscuro
  useEffect(() => {
    const apply = () => {
      const dark = st.theme === 'dark' || (st.theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);
      document.documentElement.dataset.theme = dark ? 'dark' : 'light';
      applyPalette(st.palette, dark);
    };
    apply();
    try { localStorage.setItem('seatopt-theme', st.theme); localStorage.setItem('seatopt-palette', st.palette); } catch { /* sin almacenamiento */ }
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    mq.addEventListener('change', apply);
    return () => mq.removeEventListener('change', apply);
  }, [st.theme, st.palette]);

  const onSeed = (p: Pt, mode: 'set' | 'add' | 'remove' = 'set') => {
    if (mode === 'set') { st.set({ tool: 'select' }); analyzePlan(p); } else if (mode === 'add') addArea(p); else removeArea(p);
  };
  const onCalibrated = (a: Pt, b: Pt) => {
    st.set({ tool: 'select', scaleConfirmed: true });
    const measured = Math.hypot(a.x - b.x, a.y - b.y);
    const ans = prompt(`Real length between the two points, in meters (it now measures ${(measured / 1000).toFixed(3)} m):`);
    const real = parseFloat((ans ?? '').replace(',', '.')) * 1000;
    if (!(real > 0) || measured <= 0) return;
    setScale(st.mmPerUnit * real / measured);
  };

  const ready = planReady(st.plan);
  const canGo = (i: number) => i === 0 || ready;
  const job = st.job;
  const req = st.config ? requestedSeats(st.config) : 0;
  const k = st.layout?.kpis;

  if (!st.drawing && !st.plan) {
    return (
      <div className="welcome">
        <div className="welcome-card">
          <div className="brand big">Seats<span>office layout optimizer</span></div>
          <h1>Choose the office plan</h1>
          <p>{isWeb ? 'DXF or vector PDF' : 'DXF, DWG or vector PDF'}. Then I will guide you to set the scale, the area and the main entrance.</p>
          <button className="primary huge" onClick={importPlan}>Choose file…</button>
          <button className="ghost" onClick={openProject}>Open a saved project (.json)</button>
          <button className="ghost" onClick={startBlank}>Draw without a plan (blank grid)</button>
          <fieldset className="welcome-mode">
            <legend>After opening a plan</legend>
            <label><input type="radio" name="pm" checked={st.planModePref === 'draw'} onChange={() => { savePlanMode('draw'); st.set({ planModePref: 'draw' }); }} /> I draw the areas and doors on it <span className="muted">(best for real plans)</span></label>
            <label><input type="radio" name="pm" checked={st.planModePref === 'detect'} onChange={() => { savePlanMode('detect'); st.set({ planModePref: 'detect' }); }} /> Detect them automatically</label>
          </fieldset>
          {isWeb ? (
            <p className="hint">Web version: DXF and vector PDF. Save DWG files as DXF from AutoCAD (or use the desktop app). The plan never leaves your computer.</p>
          ) : <details className="welcome-cfg">
            <summary>DWG files</summary>
            <p className="hint">They are converted with ODA File Converter (free) or LibreDWG. If it is installed elsewhere, enter the path:</p>
            <input value={st.converter} placeholder="C:\Program Files\ODA\ODAFileConverter 25.x\ODAFileConverter.exe" onChange={(e) => st.set({ converter: e.target.value })} />
          </details>}
          <ThemeSwitch />
        </div>
        {st.toast && <div className={'toast ' + st.toast.tone} role="status" onClick={() => st.set({ toast: null })}>{st.toast.text}</div>}
      </div>
    );
  }

  return (
    <div className="app">
      <header className="top">
        <div className="brand">Seats<span>office layout optimizer</span></div>
        <nav className="steps" aria-label="Steps">
          {STEPS.map((s, i) => (
            <button key={s} className={'step' + (st.step === i ? ' cur' : '')} disabled={!canGo(i)} aria-current={st.step === i ? 'step' : undefined}
              onClick={() => st.set({ step: i, tool: 'select' })}>
              <span className="n">{i + 1}</span>{s}
            </button>
          ))}
        </nav>
        <div className="file-actions">
          <button className="ghost small" disabled={!st.plan} onClick={saveProject}>Save</button>
          <button className="ghost small" onClick={closePlan}>Close plan</button>
          <ThemeSwitch collapsible />
        </div>
      </header>
      <aside className="panel" key={'step' + st.step}>
        {st.step === STEP.plan && <PlanStep />}
        {st.step === STEP.paths && <PathsStep />}
        {st.step === STEP.desks && <DesksStep />}
        {st.step === STEP.rules && <RulesStep />}
        {st.step === STEP.org && <OrgStep />}
        {st.step === STEP.optimize && <OptimizeStep />}
      </aside>
      <main className="stage">
        {st.background || st.plan ? (
          <Canvas onSeed={onSeed} onCalibrated={onCalibrated} />
        ) : (
          <div className="stage-empty">
            <p>The plan appears here.</p>
          </div>
        )}
        {st.tool !== 'select' && TOOL_HELP[st.tool] && (st.background || st.plan) && (
          <div className="tool-banner" role="status">
            <span>{TOOL_HELP[st.tool]}</span>
            <button className="banner-x" onClick={() => st.set({ tool: 'select', calib: [] })}>Cancel (Esc)</button>
          </div>
        )}
      </main>
      <footer className="status">
        {job ? (
          <div className="job">
            <span>{job.progress?.phase || job.kind}{job.progress?.message ? ` · ${job.progress.message}` : ''}</span>
            <Bar value={job.progress?.fraction ?? 0} />
            {job.progress && job.progress.seatsPlaced > 0 && !job.progress.message && <span className="mono">{job.progress.seatsRequested > 0 ? `${job.progress.seatsPlaced} / ${job.progress.seatsRequested}` : `${job.progress.seatsPlaced} seats`}</span>}
          </div>
        ) : (
          <span className="muted">{st.plan ? `${st.areaM2.toFixed(0)} m² usable` : 'No plan'}{req ? ` · ${req} fixed seats` : ''}{st.config?.demand.some((d) => d.max) ? ' · + as many as fit' : ''}</span>
        )}
        <span className="counter">
          {k ? (k.maxMode ? <><b key={k.seatsPlaced}>{k.seatsPlaced}</b> seats</> : <><b key={k.seatsPlaced}>{k.seatsPlaced}</b> / {k.seatsRequested} placed</>) : null}
        </span>
        <span className="muted small">{backendName} mode</span>
      </footer>
      {st.studio?.open && <DeskStudio key={st.studio.editId ?? 'new'} />}
      {st.toast && <div className={'toast ' + st.toast.tone} role="status" onClick={() => st.set({ toast: null })}>{st.toast.text}</div>}
    </div>
  );
}

/**
 * Paleta + claro/oscuro/sistema. `collapsible`: muestra solo el color y el tema actuales; al pasar el
 * mouse (o con foco de teclado) se despliega sobre lo que tiene alrededor, sin mover nada.
 */
function ThemeSwitch({ collapsible = false }: { collapsible?: boolean }) {
  const theme = useApp((s) => s.theme);
  const palette = useApp((s) => s.palette);
  const set = useApp((s) => s.set);
  const body = (live: boolean) => (
    <div className="theme-row">
      <div className="palette-switch" role={live ? 'radiogroup' : undefined} aria-label={live ? 'Color palette' : undefined}>
        {PALETTES.map((p, i) => (
          <button key={p.id} role="radio" aria-checked={palette === p.id} aria-label={p.name} title={p.name} tabIndex={live ? 0 : -1}
            className={'swatch opt' + (palette === p.id ? ' on cur' : '')} style={{ '--sw': p.dark, '--sw2': p.light, '--i': i } as CSSProperties}
            onClick={() => set({ palette: p.id })} />
        ))}
      </div>
      <div className="theme-switch" role={live ? 'radiogroup' : undefined} aria-label={live ? 'Theme' : undefined}>
        {([['light', 'Light'], ['dark', 'Dark'], ['system', 'System']] as [Theme, string][]).map(([t, l], i) => (
          <button key={t} role="radio" aria-checked={theme === t} tabIndex={live ? 0 : -1} style={{ '--i': i } as CSSProperties}
            className={'opt' + (theme === t ? ' on cur' : '')} onClick={() => set({ theme: t })}>{l}</button>
        ))}
      </div>
    </div>
  );
  if (!collapsible) return body(true);
  return (
    <div className="ap">
      <div className="ap-ghost" aria-hidden="true">{body(false)}</div>
      <div className="ap-live">{body(true)}</div>
    </div>
  );
}
