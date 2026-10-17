import { applySuggestion, explore, exportCsv, exportDxf, exportPdf, optimize, reassign, saveProject, suggest } from '../actions';
import { Fold, Slider } from '../components/ui';
import { requestedSeats, useApp, type ColorBy } from '../store';
import type { Variant, Weights } from '../types';

const SLIDERS: { k: keyof Weights; label: string; hint: string }[] = [
  { k: 'order', label: 'Orden visual', hint: 'Menos direcciones de bancadas y filas alineadas.' },
  { k: 'daylight', label: 'Luz natural', hint: 'Más puestos cerca de ventanas (A4).' },
  { k: 'windowSide', label: 'Ventana al costado', hint: 'Evita ventanas de frente o de espaldas al monitor (A1).' },
  { k: 'privacy', label: 'Privacidad', hint: 'Nadie de espaldas a la entrada o al pasillo; pantallas no visibles (A2, A3).' },
  { k: 'travel', label: 'Recorridos cortos', hint: 'Menor distancia promedio a las salidas.' },
  { k: 'compact', label: 'Cerca de la entrada', hint: 'Agrupa los puestos hacia la puerta principal.' },
  { k: 'reserve', label: 'Espacio para crecer', hint: 'Deja áreas aprovechables libres (A6).' },
  { k: 'grouping', label: 'Agrupación por rol y cliente', hint: 'Cuánto pesa sentar a cada equipo junto.' },
];

export default function OptimizeStep() {
  const st = useApp();
  const c = st.config;
  if (!c) return null;
  const L = st.layout;
  const k = L?.kpis;
  const req = requestedSeats(c);
  const busy = !!st.job;
  const hasPeople = c.org.teams.some((t) => t.count > 0);
  const typeName = (id: string) => c.catalog.find((t) => t.id === id)?.name ?? id;

  const useVariant = (v: Variant, adopt: boolean) => {
    st.set({ layout: v.layout });
    if (adopt) st.updateConfig((cc) => { cc.weights = v.layout.weights; });
  };

  return (
    <div className="step-body">
      <h2>Optimizar</h2>
      <p className="hint">La prioridad uno es ubicar los {req} puestos. Los pesos deciden cómo, entre las distribuciones que lo logran.</p>
      <div className="sliders">
        {SLIDERS.filter((s) => s.k !== 'grouping' || hasPeople).map((s) => (
          <Slider key={s.k} label={s.label} hint={s.hint} value={c.weights[s.k]} onChange={(v) => st.updateConfig((cc) => { cc.weights[s.k] = v; })} />
        ))}
      </div>
      <label className="slider" title="Más tiempo = más combinaciones probadas.">
        <span className="slider-label">Tiempo de búsqueda</span>
        <input type="range" min={3} max={90} step={1} value={c.settings.timeLimit} onChange={(e) => st.updateConfig((cc) => { cc.settings.timeLimit = parseFloat(e.target.value); })} />
        <span className="slider-val">{c.settings.timeLimit} s</span>
      </label>
      <label className="check"><input type="checkbox" checked={st.autoExplore} onChange={(e) => st.set({ autoExplore: e.target.checked })} /> Después de optimizar, probar otros pesos y mostrar variantes</label>

      <div className="row">
        {busy ? (
          <button className="danger" onClick={() => { if (st.job) st.job.signal.cancel = true; }}>Detener</button>
        ) : (
          <button className="primary" disabled={!st.plan || req === 0} onClick={optimize}>{L ? 'Optimizar de nuevo' : 'Optimizar'}</button>
        )}
        <button disabled={busy || !L} onClick={() => explore(10)} title="Corre el optimizador con otros pesos y muestra las mejores alternativas.">Explorar variantes</button>
      </div>

      {k && (
        <section className="result">
          <div className={'headline' + (k.seatsPlaced < k.seatsRequested ? ' short' : '')}>
            <span className="big">{k.seatsPlaced}</span><span className="of">/ {k.seatsRequested}</span>
            <span className="cap">{k.seatsPlaced < k.seatsRequested ? 'puestos ubicados: no caben todos' : 'puestos ubicados'}</span>
          </div>
          {k.unplacedByType.length > 0 && (
            <p className="warn">Sin ubicar: {k.unplacedByType.map(([t, n]) => `${n} × ${typeName(t)}`).join(', ')}</p>
          )}
          <dl className="kpis">
            <dt>Área por puesto</dt><dd>{k.m2PerSeat.toFixed(1)} m²</dd>
            <dt>Recorrido a salida</dt><dd>{k.avgTravelExitM.toFixed(1)} m prom. · {k.maxTravelExitM.toFixed(1)} m máx.</dd>
            <dt>Cerca de ventana</dt><dd>{k.daylightPct.toFixed(0)} %</dd>
            <dt>Ventana al costado</dt><dd>{k.windowSidePct.toFixed(0)} %</dd>
            <dt>Problemas de privacidad</dt><dd>{k.privacyIssues}</dd>
            <dt>Direcciones de bancadas</dt><dd>{k.orientations}</dd>
            <dt>Capacidad libre</dt><dd>~{k.reserveSeats} puestos ({k.reservePct.toFixed(0)} %)</dd>
            <dt>Accesibles en silla de ruedas</dt><dd>{k.accessiblePmr}</dd>
            {hasPeople && <><dt>Equipos juntos</dt><dd>{k.groupingScore.toFixed(0)} % · {k.misplacedPeople} fuera de lugar</dd></>}
          </dl>

          <div className="field-row">
            <span>Colorear por</span>
            {(['type', 'role', 'client'] as ColorBy[]).map((cb) => (
              <button key={cb} className={'chip' + (st.colorBy === cb ? ' active' : '')} disabled={cb !== 'type' && !hasPeople} onClick={() => st.set({ colorBy: cb })}>
                {cb === 'type' ? 'Tipo' : cb === 'role' ? 'Rol' : 'Cliente'}
              </button>
            ))}
          </div>
          {st.colorBy !== 'type' && <Legend />}
          <p className="hint">Para bloquear puestos: selecciona escritorios en el plano (arrastra de izquierda a derecha para una ventana azul o de derecha a izquierda para una captura verde) y usa «Fijar y bloquear». Se conservan con sus personas y se optimiza el resto. Arrastra para mover, R gira, Supr borra, Ctrl+Z deshace.</p>
          <label className="check"><input type="checkbox" checked={st.showKeepouts} onChange={(e) => st.set({ showKeepouts: e.target.checked })} /> Mostrar despejes de puertas</label>
          <div className="row wrap">
            {c.pinned.length > 0 && <button className="ghost small" onClick={() => { st.snapshot(); st.updateConfig((cc) => { cc.pinned = []; }); if (st.layout) st.set({ layout: { ...st.layout, modules: st.layout.modules.map((m) => ({ ...m, pinned: false })), seats: st.layout.seats.map((s) => ({ ...s, locked: false })) } }); }}>Soltar los {c.pinned.length} fijados</button>}
            {hasPeople && <button className="ghost small" onClick={reassign}>Volver a sentar a las personas</button>}
          </div>

          {L!.violations.length > 0 && (
            <Fold title={`Avisos (${L!.violations.length})`} open={L!.violations.some((v) => v.hard)}>
              <ul className="viol-list">
                {L!.violations.map((v, i) => <li key={i} className={v.hard ? 'hard' : ''}><span className="rid">{v.ruleId}</span>{v.message}</li>)}
              </ul>
            </Fold>
          )}
        </section>
      )}

      {k && k.seatsPlaced < k.seatsRequested && !st.suggestions && !busy && (
        <button onClick={suggest}>Sugerir qué reglas cambiar</button>
      )}
      {st.suggestions && (
        <section className="suggestions">
          <h3>Qué cambiar para que quepan</h3>
          {st.suggestions.length === 0 ? (
            <p className="hint">Ningún cambio de una sola regla relajable gana puestos. Prueba quitar escritorios, cambiar de tipo o desbloquear alguna regla.</p>
          ) : (
            <ul>
              {st.suggestions.map((s, i) => (
                <li key={i}>
                  <span className="gain">+{s.gainSeats}</span>
                  <span className="desc">{s.description}</span>
                  <button className="small" onClick={() => applySuggestion(s)}>Aplicar</button>
                </li>
              ))}
            </ul>
          )}
        </section>
      )}

      {st.variants.length > 0 && (
        <section className="variants">
          <h3>Variantes</h3>
          <p className="hint">Rango 1 = frente de Pareto: ninguna otra variante es mejor en todo a la vez. Haz clic para verla; «Usar pesos» copia sus sliders.</p>
          <div className="matrix-wrap">
            <table className="vtable">
              <thead><tr><th>Rango</th><th>Puestos</th><th>Recorr.</th><th>Luz</th><th>Priv.</th><th>Dir.</th><th>Libre</th>{hasPeople && <th>Equipos</th>}<th /></tr></thead>
              <tbody>
                {st.variants.map((v, i) => {
                  const kk = v.layout.kpis;
                  const cur = st.layout === v.layout;
                  return (
                    <tr key={i} className={cur ? 'cur' : ''} onClick={() => useVariant(v, false)}>
                      <td>{v.paretoRank}</td><td>{kk.seatsPlaced}</td><td>{kk.avgTravelExitM.toFixed(0)} m</td><td>{kk.daylightPct.toFixed(0)}%</td>
                      <td>{kk.privacyIssues}</td><td>{kk.orientations}</td><td>{kk.reservePct.toFixed(0)}%</td>{hasPeople && <td>{kk.groupingScore.toFixed(0)}%</td>}
                      <td><button className="ghost small" onClick={(e) => { e.stopPropagation(); useVariant(v, true); }}>Usar pesos</button></td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </section>
      )}

      {L && (
        <section>
          <h3>Exportar</h3>
          <div className="row wrap">
            <button onClick={() => exportDxf(false)} title="Solo escritorios, sillas, números y pasillos, en las coordenadas del plano original: se puede insertar como referencia externa.">DXF para superponer</button>
            <button onClick={() => exportDxf(true)}>DXF con el recinto</button>
            <button onClick={exportCsv}>Lista de puestos (CSV)</button>
          </div>
          <div className="field-row">
            <label>PDF
              <select value={st.pdfPaper} onChange={(e) => st.set({ pdfPaper: e.target.value })}>
                {['A4', 'A3', 'A2', 'A1', 'A0'].map((p) => <option key={p}>{p}</option>)}
              </select>
            </label>
            <label>Escala 1:
              <select value={st.pdfScale} onChange={(e) => st.set({ pdfScale: parseFloat(e.target.value) })}>
                <option value={0}>automática</option>
                {[50, 75, 100, 125, 150, 200, 250].map((n) => <option key={n} value={n}>{n}</option>)}
              </select>
            </label>
            <button className="primary" onClick={() => exportPdf({})}>Exportar PDF</button>
          </div>
          <div className="row wrap">
            <button onClick={saveProject}>Guardar proyecto</button>
          </div>
        </section>
      )}
    </div>
  );
}

function Legend() {
  const st = useApp();
  const c = st.config!;
  const items = st.colorBy === 'role' ? c.org.roles : c.org.clients;
  return (
    <div className="legend">
      {items.map((x) => <span key={x.id}><i className="sw" style={{ background: x.color }} />{x.name}</span>)}
      <span><i className="sw" style={{ background: 'var(--paper)' }} />Libre</span>
    </div>
  );
}
