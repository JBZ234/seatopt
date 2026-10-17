import { applySuggestion, exportCsv, exportDxf, exportPdf, optimize, reassign, saveProject, suggest } from '../actions';
import { Fold, Slider } from '../components/ui';
import { requestedSeats, useApp, type ColorBy } from '../store';

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

  const maxTypes = c.demand.filter((d) => d.max).map((d) => typeName(d.typeId));
  const anyDemand = req > 0 || maxTypes.length > 0;

  return (
    <div className="step-body">
      <h2>Optimizar</h2>
      <p className="hint">
        {req > 0 && <>Primero se ubican los {req} puestos con cantidad fija. </>}
        {maxTypes.length > 0 ? <>Después se meten todos los que quepan de: {maxTypes.join(', ')}.</> : <>Busca la distribución que deja el mejor acceso con esas cantidades.</>}
      </p>
      {hasPeople && (
        <div className="sliders">
          <Slider label="Agrupación por rol y cliente" hint="Cuánto pesa sentar a cada equipo junto." value={c.weights.grouping} onChange={(v) => st.updateConfig((cc) => { cc.weights.grouping = v; })} />
        </div>
      )}
      <label className="slider" title="Más tiempo = más combinaciones probadas.">
        <span className="slider-label">Tiempo de búsqueda</span>
        <input type="range" min={3} max={90} step={1} value={c.settings.timeLimit} onChange={(e) => st.updateConfig((cc) => { cc.settings.timeLimit = parseFloat(e.target.value); })} />
        <span className="slider-val">{c.settings.timeLimit} s</span>
      </label>

      <div className="row">
        {busy ? (
          <button className="danger" onClick={() => { if (st.job) st.job.signal.cancel = true; }}>Detener</button>
        ) : (
          <button className="primary" disabled={!st.plan || !anyDemand} onClick={optimize}>{L ? 'Optimizar de nuevo' : 'Optimizar'}</button>
        )}
      </div>

      {k && (
        <section className="result">
          {k.maxMode ? (
            <div className={'headline' + (k.unplacedByType.length ? ' short' : '')}>
              <span className="big">{k.seatsPlaced}</span><span className="of">puestos</span>
              <span className="cap">
                {k.seatsRequested > 0 ? `${k.seatsPlaced - k.maxSeats} de cantidad fija (${k.unplacedByType.length ? 'faltan algunos' : 'todos ubicados'}) + ${k.maxSeats} en «máximo posible»` : 'máximo posible con estas medidas'}
              </span>
            </div>
          ) : (
            <div className={'headline' + (k.seatsPlaced < k.seatsRequested ? ' short' : '')}>
              <span className="big">{k.seatsPlaced}</span><span className="of">/ {k.seatsRequested}</span>
              <span className="cap">{k.seatsPlaced < k.seatsRequested ? 'puestos ubicados: no caben todos' : 'puestos ubicados'}</span>
            </div>
          )}
          <SeatsByType />
          {k.unplacedByType.length > 0 && (
            <p className="warn">Sin ubicar: {k.unplacedByType.map(([t, n]) => `${n} × ${typeName(t)}`).join(', ')}</p>
          )}
          <dl className="kpis">
            <dt>Área por puesto</dt><dd>{k.m2PerSeat.toFixed(1)} m²</dd>
            <dt>Escritorios</dt><dd>{k.modulesPlaced}</dd>
            <dt>Recorrido a salida</dt><dd>{k.avgTravelExitM.toFixed(1)} m prom. · {k.maxTravelExitM.toFixed(1)} m máx.</dd>
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

      {k && !st.suggestions && !busy && (
        <button onClick={suggest}>{k.seatsPlaced < k.seatsRequested ? 'Sugerir qué medidas cambiar para que quepan' : '¿Qué medida me da más puestos?'}</button>
      )}
      {st.suggestions && (
        <section className="suggestions">
          <h3>Cambios que ganan puestos</h3>
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
          {st.drawing?.drawingId && (
            <label className="check"><input type="checkbox" checked={st.pdfBackground} onChange={(e) => st.set({ pdfBackground: e.target.checked })} /> Incluir el dibujo original de fondo en el PDF</label>
          )}
          <div className="row wrap">
            <button onClick={saveProject}>Guardar proyecto</button>
          </div>
        </section>
      )}
    </div>
  );
}

function SeatsByType() {
  const st = useApp();
  const c = st.config!;
  const L = st.layout!;
  const rows = c.catalog
    .map((t) => ({ t, mods: L.modules.filter((m) => m.typeId === t.id).length, seats: L.seats.filter((s) => s.typeId === t.id).length, d: c.demand.find((d) => d.typeId === t.id) }))
    .filter((r) => r.mods > 0 || r.d);
  if (!rows.length) return null;
  return (
    <table className="by-type">
      <tbody>
        {rows.map(({ t, mods, seats, d }) => (
          <tr key={t.id}>
            <td><i className="sw" style={{ background: t.color }} /></td>
            <td>{t.name}</td>
            <td className="n">{mods} esc.</td>
            <td className="n">{seats} puestos</td>
            <td className="muted">{d?.max ? 'máximo' : d ? `pedidos ${d.quantity}` : ''}</td>
          </tr>
        ))}
      </tbody>
    </table>
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
