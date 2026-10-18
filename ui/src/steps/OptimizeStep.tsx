import { applySuggestion, chooseAlternative, exportCsv, exportDxf, exportPdf, optimize, reassign, saveProject, suggest } from '../actions';
import Thumb from '../components/Thumb';
import { Fold, Num, Slider } from '../components/ui';
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
        {req > 0 && <>Primero se ubica{req > 1 ? 'n los' : ''} {req} puesto{req > 1 ? 's' : ''} con cantidad fija. </>}
        {maxTypes.length > 0 ? <>{req > 0 ? 'Después se meten' : 'Se meten'} todos los que quepan de: {maxTypes.join(', ')}.</> : <>Busca la distribución que deja el mejor acceso con esas cantidades.</>}
        {' '}Busca hasta que deja de encontrar mejoras; puedes detenerla cuando quieras.
      </p>
      <div className="sliders">
        <Slider label="Orden de las filas" hint="0: mete escritorios en cualquier hueco (más puestos). 10: solo filas ordenadas y escritorios apoyados en muros o espalda con espalda." value={c.weights.order} onChange={(v) => st.updateConfig((cc) => { cc.weights.order = v; })} />
        <Slider label="Pegar a los muros" hint="Cuánto se prefiere que los escritorios de un lado queden con la espalda contra un muro." value={c.weights.wall ?? 5} onChange={(v) => st.updateConfig((cc) => { cc.weights.wall = v; })} />
        {hasPeople && <Slider label="Agrupación por rol y cliente" hint="Cuánto pesa sentar a cada equipo junto." value={c.weights.grouping} onChange={(v) => st.updateConfig((cc) => { cc.weights.grouping = v; })} />}
      </div>
      <div className="field-row">
        <span>Mostrar las mejores</span>
        <Num value={c.settings.alternatives ?? 10} unit="soluciones" width={48} min={1} max={30} label="Cantidad de soluciones a mostrar" onChange={(v) => st.updateConfig((cc) => { cc.settings.alternatives = Math.max(1, Math.min(30, Math.round(v))); })} />
      </div>

      <div className="row">
        {busy ? (
          <button className="danger" onClick={() => { if (st.job) st.job.signal.cancel = true; }}>Detener</button>
        ) : (
          <>
            <button className="primary" disabled={!st.plan || !anyDemand} onClick={() => optimize()}>{L ? 'Optimizar de nuevo' : 'Optimizar'}</button>
            {L?.genome != null && <button title="Continúa desde la solución que estás viendo, sin empezar de cero" onClick={() => optimize(true)}>Seguir buscando</button>}
          </>
        )}
      </div>

      {L && (
        <section className="manual-section">
          <h3>Agregar escritorios a mano</h3>
          <p className="hint">Elige el tipo y muévelo por el plano: en verde cabe, en rojo te dice qué regla incumple. Clic para ubicarlo · R gira · F cambia el lado de la silla · Esc termina.</p>
          <div className="field-row manual-add">
                        <select aria-label="Tipo de escritorio a agregar" value={st.placeType || c.catalog[0]?.id} onChange={(e) => st.set({ placeType: e.target.value })}>
              {c.catalog.map((t) => <option key={t.id} value={t.id}>{t.name}</option>)}
            </select>
            <button className={'small' + (st.tool === 'placeDesk' ? ' active-tool' : '')}
              title="Muévelo por el plano: en verde cabe, en rojo te dice qué regla incumple. R gira, F cambia el lado de la silla."
              onClick={() => st.set(st.tool === 'placeDesk' ? { tool: 'select' } : { tool: 'placeDesk', placeType: st.placeType || c.catalog[0]?.id, selection: { modules: [], obstacles: [], doors: [], windows: [] } })}>
              {st.tool === 'placeDesk' ? 'Terminar' : 'Colocar'}
            </button>
          </div>
        </section>
      )}

      {st.alternatives.length > 1 && st.plan && (
        <section className="alts">
          <h3>Mejores soluciones encontradas</h3>
          <div className="alt-list">
            {st.alternatives.map((a, i) => (
              <button key={i} className={'alt' + (a === L ? ' cur' : '')} onClick={() => chooseAlternative(i)} aria-pressed={a === L}>
                <Thumb plan={st.plan!} layout={a} config={c} />
                <span className="alt-n">{a.kpis.seatsPlaced}<small> puestos</small></span>
                <span className="alt-sub">#{i + 1} · {a.kpis.modulesPlaced} escritorios{a.kpis.unplacedByType.length ? ' · faltan fijos' : ''}</span>
                {a.kpis.hardViolations > 0 && <span className="alt-warn" title={a.violations.filter((v) => v.hard).map((v) => v.message).join('\n')}>Incumple: {a.violations.find((v) => v.hard)?.message ?? 'una regla'}</span>}
              </button>
            ))}
          </div>
        </section>
      )}

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
          <p className="hint">Para bloquear puestos: selecciona escritorios en el plano (arrastra de izquierda a derecha para una ventana azul o de derecha a izquierda para una captura verde) y usa «Fijar y bloquear». Se conservan con sus personas y se optimiza el resto. Una selección nueva reemplaza la anterior; con Ctrl se suma y con Shift se quita. Arrastra para mover, R gira, Supr borra, Ctrl+Z deshace y Ctrl+Y rehace.</p>
          <div className="show-row">
            <span>Mostrar</span>
            <label className="check"><input type="checkbox" checked={st.showCorridor} onChange={(e) => st.set({ showCorridor: e.target.checked })} /><i className="lg lg-corridor" /> Pasillo principal</label>
            <label className="check" title="Camino de referencia desde la entrada hasta cada puerta de oficina, por el piso ya amoblado. No cambia la optimización."><input type="checkbox" checked={st.showAccess} onChange={(e) => st.set({ showAccess: e.target.checked })} /><i className="lg lg-access" /> Recorridos a oficinas</label>
            <label className="check"><input type="checkbox" checked={st.showKeepouts} onChange={(e) => st.set({ showKeepouts: e.target.checked })} /><i className="lg lg-keep" /> Despejes de puertas</label>
          </div>
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

      {k && (
        <section className="better">
          <h3>Buscar mejores valores</h3>
          <p className="hint">Prueba bajar cada medida de <b>Espacios</b> a su mínimo razonable (pasillos, separación entre filas, espacio de silla, columnas…) y cambiar de tipo de escritorio. Cada prueba es una optimización completa: el número que ves es el de una distribución real que puedes usar.</p>
          {!busy && <button onClick={suggest}>{st.suggestions ? 'Buscar de nuevo' : 'Buscar mejores valores'}</button>}
          {st.suggestions && (st.suggestions.length === 0 ? (
            <p className="hint">Con estas medidas ningún cambio individual gana puestos: la distribución actual ya es la mejor que se encontró.</p>
          ) : (
            <ul className="better-list">
              {st.suggestions.map((s, i) => (
                <li key={i}>
                  {s.layout && st.plan && <Thumb plan={st.plan} layout={s.layout} config={c} width={96} height={64} />}
                  <div className="better-body">
                    <span className="gain">+{s.gainSeats}</span> <b>{s.seatsPlaced} puestos</b>
                    <span className="desc">{s.description}</span>
                  </div>
                  <button className="small primary" onClick={() => applySuggestion(s)}>Usar</button>
                </li>
              ))}
            </ul>
          ))}
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
