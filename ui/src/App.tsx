import { useEffect } from 'react';
import { analyzePlan, closePlan, ensureConfig, importPlan, openProject, saveProject, setScale } from './actions';
import { backendName } from './api';
import Canvas from './components/Canvas';
import { Bar } from './components/ui';
import { deleteSelection, evaluateSoon, moveSelection, pinSelection, rotateSelection, selectAll } from './edit';
import { emptySel, requestedSeats, selCount, useApp, type Theme, type Tool } from './store';
import DesksStep from './steps/DesksStep';
import OptimizeStep from './steps/OptimizeStep';
import OrgStep from './steps/OrgStep';
import PlanStep from './steps/PlanStep';
import RulesStep from './steps/RulesStep';
import type { Pt } from './types';

const STEPS = ['Plano', 'Escritorios', 'Espacios', 'Organización', 'Optimizar'];

/** Qué hacer con cada herramienta: se muestra sobre el plano. */
const TOOL_HELP: Partial<Record<Tool, string>> = {
  seed: 'Haz clic dentro del área que vas a amoblar',
  outline: 'Clic en cada esquina del área · clic en el primer punto, doble clic o Enter para cerrar · Retroceso borra el último punto',
  mainDoor: 'Haz clic sobre la puerta de acceso del personal (líneas de color en el borde)',
  exitDoor: 'Haz clic sobre las salidas de emergencia',
  door: 'Cada clic sobre una puerta cambia su tipo: oficina → salida → principal → ignorada',
  drawDoor: 'Arrastra de un lado al otro del vano de la puerta',
  calibrate: 'Haz clic en los dos extremos de una medida que conozcas (un muro, una puerta)',
  column: 'Arrastra para dibujar la columna',
  fixedOffice: 'Arrastra para dibujar la oficina o sala fija',
  restricted: 'Arrastra sobre el tablero o gabinete',
  reserved: 'Arrastra para reservar el área',
  window: 'Arrastra a lo largo de la ventana',
  erase: 'Haz clic sobre lo que quieras borrar',
  pan: 'Arrastra para mover el plano',
  placeDesk: 'Clic para colocar el escritorio · R gira 90° · F cambia el lado de la silla · Esc termina',
};

export default function App() {
  const st = useApp();
  useEffect(() => {
    ensureConfig().catch((e) => st.say('No se pudo conectar con el motor: ' + e, 'error'));
    const esc = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement;
      if (t && (t.tagName === 'INPUT' || t.tagName === 'SELECT' || t.tagName === 'TEXTAREA')) return;
      const s = useApp.getState();
      const ctrl = e.ctrlKey || e.metaKey;
      if (e.key === 'Escape') s.set({ tool: 'select', calib: [], selection: emptySel() });
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
    };
    apply();
    try { localStorage.setItem('seatopt-theme', st.theme); } catch { /* sin almacenamiento */ }
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    mq.addEventListener('change', apply);
    return () => mq.removeEventListener('change', apply);
  }, [st.theme]);

  const onSeed = (p: Pt) => { st.set({ tool: 'select' }); analyzePlan(p); };
  const onCalibrated = (a: Pt, b: Pt) => {
    st.set({ tool: 'select', scaleConfirmed: true });
    const measured = Math.hypot(a.x - b.x, a.y - b.y);
    const ans = prompt(`Medida real entre los dos puntos, en metros (ahora mide ${(measured / 1000).toFixed(3)} m):`);
    const real = parseFloat((ans ?? '').replace(',', '.')) * 1000;
    if (!(real > 0) || measured <= 0) return;
    setScale(st.mmPerUnit * real / measured);
  };

  const canGo = (i: number) => i === 0 || !!st.plan;
  const job = st.job;
  const req = st.config ? requestedSeats(st.config) : 0;
  const k = st.layout?.kpis;

  if (!st.drawing && !st.plan) {
    return (
      <div className="welcome">
        <div className="welcome-card">
          <div className="brand big">Puestos<span>optimizador de distribución</span></div>
          <h1>Selecciona el plano de la oficina</h1>
          <p>DXF, DWG o PDF vectorial. Después te guío para marcar la escala, el área y la puerta principal.</p>
          <button className="primary huge" onClick={importPlan}>Seleccionar archivo…</button>
          <button className="ghost" onClick={openProject}>Abrir un proyecto guardado (.puestos.json)</button>
          <details className="welcome-cfg">
            <summary>Archivos DWG</summary>
            <p className="hint">Se convierten con ODA File Converter (gratuito) o LibreDWG. Si está en otra carpeta, indica la ruta:</p>
            <input value={st.converter} placeholder="C:\Program Files\ODA\ODAFileConverter 25.x\ODAFileConverter.exe" onChange={(e) => st.set({ converter: e.target.value })} />
          </details>
          <ThemeSwitch />
        </div>
        {st.toast && <div className={'toast ' + st.toast.tone} role="status" onClick={() => st.set({ toast: null })}>{st.toast.text}</div>}
      </div>
    );
  }

  return (
    <div className="app">
      <header className="top">
        <div className="brand">Puestos<span>optimizador de distribución</span></div>
        <nav className="steps" aria-label="Pasos">
          {STEPS.map((s, i) => (
            <button key={s} className={'step' + (st.step === i ? ' cur' : '')} disabled={!canGo(i)} aria-current={st.step === i ? 'step' : undefined}
              onClick={() => st.set({ step: i, tool: 'select' })}>
              <span className="n">{i + 1}</span>{s}
            </button>
          ))}
        </nav>
        <div className="file-actions">
          <button className="ghost small" disabled={!st.plan} onClick={saveProject}>Guardar</button>
          <button className="ghost small" onClick={closePlan}>Cerrar plano</button>
          <ThemeSwitch />
        </div>
      </header>
      <aside className="panel">
        {st.step === 0 && <PlanStep />}
        {st.step === 1 && <DesksStep />}
        {st.step === 2 && <RulesStep />}
        {st.step === 3 && <OrgStep />}
        {st.step === 4 && <OptimizeStep />}
      </aside>
      <main className="stage">
        {st.background || st.plan ? (
          <Canvas onSeed={onSeed} onCalibrated={onCalibrated} />
        ) : (
          <div className="stage-empty">
            <p>El plano aparece aquí.</p>
          </div>
        )}
        {st.tool !== 'select' && TOOL_HELP[st.tool] && (st.background || st.plan) && (
          <div className="tool-banner" role="status">
            <span>{TOOL_HELP[st.tool]}</span>
            <button className="banner-x" onClick={() => st.set({ tool: 'select', calib: [] })}>Cancelar (Esc)</button>
          </div>
        )}
      </main>
      <footer className="status">
        {job ? (
          <div className="job">
            <span>{job.progress?.phase || job.kind}{job.progress?.message ? ` · ${job.progress.message}` : ''}</span>
            <Bar value={job.progress?.fraction ?? 0} />
            {job.progress && job.progress.seatsPlaced > 0 && !job.progress.message && <span className="mono">{job.progress.seatsRequested > 0 ? `${job.progress.seatsPlaced} / ${job.progress.seatsRequested}` : `${job.progress.seatsPlaced} puestos`}</span>}
          </div>
        ) : (
          <span className="muted">{st.plan ? `${st.areaM2.toFixed(0)} m² útiles` : 'Sin plano'}{req ? ` · ${req} puestos fijos` : ''}{st.config?.demand.some((d) => d.max) ? ' · + máximo posible' : ''}</span>
        )}
        <span className="counter">
          {k ? (k.maxMode ? <><b>{k.seatsPlaced}</b> puestos</> : <><b>{k.seatsPlaced}</b> / {k.seatsRequested} ubicados</>) : null}
        </span>
        <span className="muted small">modo {backendName}</span>
      </footer>
      {st.toast && <div className={'toast ' + st.toast.tone} role="status" onClick={() => st.set({ toast: null })}>{st.toast.text}</div>}
    </div>
  );
}

function ThemeSwitch() {
  const theme = useApp((s) => s.theme);
  const set = useApp((s) => s.set);
  return (
    <div className="theme-switch" role="radiogroup" aria-label="Tema">
      {([['light', 'Claro'], ['dark', 'Oscuro'], ['system', 'Sistema']] as [Theme, string][]).map(([t, l]) => (
        <button key={t} role="radio" aria-checked={theme === t} className={theme === t ? 'on' : ''} onClick={() => set({ theme: t })}>{l}</button>
      ))}
    </div>
  );
}
