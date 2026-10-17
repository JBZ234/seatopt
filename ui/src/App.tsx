import { useEffect } from 'react';
import { analyzePlan, ensureConfig, importPlan, openProject, saveProject, setScale } from './actions';
import { backendName } from './api';
import Canvas from './components/Canvas';
import { Bar } from './components/ui';
import { requestedSeats, useApp } from './store';
import DesksStep from './steps/DesksStep';
import OptimizeStep from './steps/OptimizeStep';
import OrgStep from './steps/OrgStep';
import PlanStep from './steps/PlanStep';
import RulesStep from './steps/RulesStep';
import type { Pt } from './types';

const STEPS = ['Plano', 'Escritorios', 'Reglas', 'Organización', 'Optimizar'];

export default function App() {
  const st = useApp();
  useEffect(() => {
    ensureConfig().catch((e) => st.say('No se pudo conectar con el motor: ' + e, 'error'));
    const esc = (e: KeyboardEvent) => { if (e.key === 'Escape') useApp.getState().set({ tool: 'pan', calib: [] }); };
    window.addEventListener('keydown', esc);
    return () => window.removeEventListener('keydown', esc);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const onSeed = (p: Pt) => { st.set({ tool: 'pan' }); analyzePlan(p); };
  const onCalibrated = (a: Pt, b: Pt) => {
    st.set({ tool: 'pan' });
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

  return (
    <div className="app">
      <header className="top">
        <div className="brand">Puestos<span>optimizador de distribución</span></div>
        <nav className="steps" aria-label="Pasos">
          {STEPS.map((s, i) => (
            <button key={s} className={'step' + (st.step === i ? ' cur' : '')} disabled={!canGo(i)} aria-current={st.step === i ? 'step' : undefined}
              onClick={() => st.set({ step: i, tool: 'pan' })}>
              <span className="n">{i + 1}</span>{s}
            </button>
          ))}
        </nav>
        <div className="file-actions">
          <button className="ghost small" onClick={importPlan}>Abrir plano</button>
          <button className="ghost small" onClick={openProject}>Abrir proyecto</button>
          <button className="ghost small" disabled={!st.plan} onClick={saveProject}>Guardar</button>
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
        {st.tool !== 'pan' && <div className="tool-banner">Herramienta activa · Esc para salir</div>}
      </main>
      <footer className="status">
        {job ? (
          <div className="job">
            <span>{job.progress?.phase || job.kind}{job.progress?.message ? ` · ${job.progress.message}` : ''}</span>
            <Bar value={job.progress?.fraction ?? 0} />
            {job.progress && job.progress.seatsRequested > 0 && <span className="mono">{job.progress.seatsPlaced} / {job.progress.seatsRequested}</span>}
          </div>
        ) : (
          <span className="muted">{st.plan ? `${st.areaM2.toFixed(0)} m² útiles` : 'Sin plano'}{req ? ` · ${req} puestos pedidos` : ''}</span>
        )}
        <span className="counter">
          {k ? <><b>{k.seatsPlaced}</b> / {k.seatsRequested} ubicados</> : null}
        </span>
        <span className="muted small">modo {backendName}</span>
      </footer>
      {st.toast && <div className={'toast ' + st.toast.tone} role="status" onClick={() => st.set({ toast: null })}>{st.toast.text}</div>}
    </div>
  );
}
