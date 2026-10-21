import type { CSSProperties } from 'react';
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
      <h2>Optimize</h2>
      <p className="hint">
        {req > 0 && <>First the {req} fixed seat{req > 1 ? 's are' : ' is'} placed. </>}
        {maxTypes.length > 0 ? <>{req > 0 ? 'Then' : 'It places'} as many as fit of: {maxTypes.join(', ')}.</> : <>It looks for the layout with the best access for those numbers.</>}
        {' '}It searches until it stops finding improvements; you can stop it at any time.
      </p>
      <div className="sliders">
        {hasPeople && <Slider label="Group by role and client" hint="How much seating each team together matters." value={c.weights.grouping} onChange={(v) => st.updateConfig((cc) => { cc.weights.grouping = v; })} />}
      </div>
      <div className="field-row">
        <span>Show the best</span>
        <Num value={c.settings.alternatives ?? 10} unit="layouts" width={48} min={1} max={30} label="Number of layouts to show" onChange={(v) => st.updateConfig((cc) => { cc.settings.alternatives = Math.max(1, Math.min(30, Math.round(v))); })} />
      </div>

      <div className="row">
        {busy ? (
          <button className="danger" onClick={() => { if (st.job) st.job.signal.cancel = true; }}>Stop</button>
        ) : (
          <>
            <button className="primary" disabled={!st.plan || !anyDemand} onClick={() => optimize()}>{L ? 'Optimize again' : 'Optimize'}</button>
            {L?.genome != null && <button title="Continue from the layout you are looking at, without starting over" onClick={() => optimize(true)}>Keep searching</button>}
          </>
        )}
      </div>

      {L && (
        <section className="manual-section">
          <h3>Add desks by hand</h3>
          <p className="hint">Choose the type and move it over the plan: green means it fits, red tells you which rule it breaks. Click to place it · R rotates · F flips the chair side · Esc finishes.</p>
          <div className="field-row manual-add">
                        <select aria-label="Desk type to add" value={st.placeType || c.catalog[0]?.id} onChange={(e) => st.set({ placeType: e.target.value })}>
              {c.catalog.map((t) => <option key={t.id} value={t.id}>{t.name}</option>)}
            </select>
            <button className={'small' + (st.tool === 'placeDesk' ? ' active-tool' : '')}
              title="Move it over the plan: green means it fits, red tells you which rule it breaks. R rotates, F flips the chair side."
              onClick={() => st.set(st.tool === 'placeDesk' ? { tool: 'select' } : { tool: 'placeDesk', placeType: st.placeType || c.catalog[0]?.id, selection: { modules: [], obstacles: [], doors: [], windows: [] } })}>
              {st.tool === 'placeDesk' ? 'Done' : 'Place'}
            </button>
          </div>
        </section>
      )}

      {st.alternatives.length > 1 && st.plan && (
        <section className="alts">
          <h3>Best layouts found</h3>
          <div className="alt-list">
            {st.alternatives.map((a, i) => (
              <button key={i} className={'alt' + (a === L ? ' cur' : '')} style={{ '--i': i } as CSSProperties} onClick={() => chooseAlternative(i)} aria-pressed={a === L}>
                <Thumb plan={st.plan!} layout={a} config={c} />
                <span className="alt-n">{a.kpis.seatsPlaced}<small> seats</small></span>
                <span className="alt-sub">#{i + 1} · {a.kpis.modulesPlaced} desks{a.kpis.unplacedByType.length ? ' · fixed ones missing' : ''}</span>
                {a.kpis.hardViolations > 0 && <span className="alt-warn" title={a.violations.filter((v) => v.hard).map((v) => v.message).join('\n')}>Breaks: {a.violations.find((v) => v.hard)?.message ?? 'a rule'}</span>}
              </button>
            ))}
          </div>
        </section>
      )}

      {k && (
        <section className="result">
          {k.maxMode ? (
            <div className={'headline' + (k.unplacedByType.length ? ' short' : '')}>
              <span className="big">{k.seatsPlaced}</span><span className="of">seats</span>
              <span className="cap">
                {k.seatsRequested > 0 ? `${k.seatsPlaced - k.maxSeats} fixed (${k.unplacedByType.length ? 'some missing' : 'all placed'}) + ${k.maxSeats} “as many as fit”` : 'as many as fit with these rules'}
              </span>
            </div>
          ) : (
            <div className={'headline' + (k.seatsPlaced < k.seatsRequested ? ' short' : '')}>
              <span className="big">{k.seatsPlaced}</span><span className="of">/ {k.seatsRequested}</span>
              <span className="cap">{k.seatsPlaced < k.seatsRequested ? 'seats placed: not all fit' : 'seats placed'}</span>
            </div>
          )}
          <SeatsByType />

          {k.unplacedByType.length > 0 && (
            <p className="warn">Not placed: {k.unplacedByType.map(([t, n]) => `${n} × ${typeName(t)}`).join(', ')}</p>
          )}
          <dl className="kpis">
            <dt>Area per seat</dt><dd>{k.m2PerSeat.toFixed(1)} m²</dd>
            <dt>Desks</dt><dd>{k.modulesPlaced}</dd>
            <dt>Walk to an exit</dt><dd>{k.avgTravelExitM.toFixed(1)} m avg · {k.maxTravelExitM.toFixed(1)} m max</dd>
            {hasPeople && <><dt>Teams together</dt><dd>{k.groupingScore.toFixed(0)} % · {k.misplacedPeople} out of place</dd></>}
          </dl>

          <div className="field-row">
            <span>Color by</span>
            {(['type', 'role', 'client'] as ColorBy[]).map((cb) => (
              <button key={cb} className={'chip' + (st.colorBy === cb ? ' active' : '')} disabled={cb !== 'type' && !hasPeople} onClick={() => st.set({ colorBy: cb })}>
                {cb === 'type' ? 'Type' : cb === 'role' ? 'Role' : 'Client'}
              </button>
            ))}
          </div>
          {st.colorBy !== 'type' && <Legend />}
          <p className="hint">To lock seats: select desks on the plan (drag left to right for a blue window or right to left for a green crossing selection) and use “Pin and lock”. They stay with their people and the rest is optimized. A new selection replaces the previous one; Ctrl adds and Shift removes. Drag to move, R rotates, Delete removes, Ctrl+Z undoes and Ctrl+Y redoes.</p>
          <div className="show-row">
            <span>Show</span>
            <label className="check"><input type="checkbox" checked={st.showCorridor} onChange={(e) => st.set({ showCorridor: e.target.checked })} /><i className="lg lg-corridor" /> Main corridor</label>
            <label className="check" title="Reference route from the entrance to each office door over the furnished floor (when there are no circulation paths). It does not change the optimization."><input type="checkbox" checked={st.showAccess} onChange={(e) => st.set({ showAccess: e.target.checked })} /><i className="lg lg-access" /> Routes to offices</label>
            <label className="check"><input type="checkbox" checked={st.showKeepouts} onChange={(e) => st.set({ showKeepouts: e.target.checked })} /><i className="lg lg-keep" /> Door clearances</label>
          </div>
          <div className="row wrap">
            {c.pinned.length > 0 && <button className="ghost small" onClick={() => { st.snapshot(); st.updateConfig((cc) => { cc.pinned = []; }); if (st.layout) st.set({ layout: { ...st.layout, modules: st.layout.modules.map((m) => ({ ...m, pinned: false })), seats: st.layout.seats.map((s) => ({ ...s, locked: false })) } }); }}>Release the {c.pinned.length} pinned</button>}
            {hasPeople && <button className="ghost small" onClick={reassign}>Seat the people again</button>}
          </div>

          {L!.violations.length > 0 && (
            <Fold title={`Warnings (${L!.violations.length})`} open={L!.violations.some((v) => v.hard)}>
              <ul className="viol-list">
                {L!.violations.map((v, i) => <li key={i} className={v.hard ? 'hard' : ''}><span className="rid">{v.ruleId}</span>{v.message}</li>)}
              </ul>
            </Fold>
          )}
        </section>
      )}

      {k && (
        <section className="better">
          <h3>Find better values</h3>
          <p className="hint">Tries lowering each measure in <b>Rules</b> to its reasonable minimum (paths, space between rows, chair space, columns…) and switching desk types. Each try is a full optimization: the number you see is a real layout you can use.</p>
          {!busy && <button onClick={suggest}>{st.suggestions ? 'Search again' : 'Find better values'}</button>}
          {st.suggestions && (st.suggestions.length === 0 ? (
            <p className="hint">With these measures no single change gains seats: the current layout is already the best one found.</p>
          ) : (
            <ul className="better-list">
              {st.suggestions.map((s, i) => (
                <li key={i}>
                  {s.layout && st.plan && <Thumb plan={st.plan} layout={s.layout} config={c} width={96} height={64} />}
                  <div className="better-body">
                    <span className="gain">+{s.gainSeats}</span> <b>{s.seatsPlaced} seats</b>
                    <span className="desc">{s.description}</span>
                  </div>
                  <button className="small primary" onClick={() => applySuggestion(s)}>Use</button>
                </li>
              ))}
            </ul>
          ))}
        </section>
      )}

      {L && (
        <section>
          <h3>Export</h3>
          <div className="row wrap">
            <button onClick={() => exportDxf(false)} title="Only desks, chairs, numbers and corridors, in the coordinates of the original plan: it can be inserted as an external reference.">DXF overlay</button>
            <button onClick={() => exportDxf(true)}>DXF with the area</button>
            <button onClick={exportCsv}>Seat list (CSV)</button>
          </div>
          <div className="field-row">
            <label>PDF
              <select value={st.pdfPaper} onChange={(e) => st.set({ pdfPaper: e.target.value })}>
                {['A4', 'A3', 'A2', 'A1', 'A0'].map((p) => <option key={p}>{p}</option>)}
              </select>
            </label>
            <label>Scale 1:
              <select value={st.pdfScale} onChange={(e) => st.set({ pdfScale: parseFloat(e.target.value) })}>
                <option value={0}>automatic</option>
                {[50, 75, 100, 125, 150, 200, 250].map((n) => <option key={n} value={n}>{n}</option>)}
              </select>
            </label>
            <button className="primary" onClick={() => exportPdf({})}>Export PDF</button>
          </div>
          {st.drawing?.drawingId && (
            <label className="check"><input type="checkbox" checked={st.pdfBackground} onChange={(e) => st.set({ pdfBackground: e.target.checked })} /> Include the original drawing as background in the PDF</label>
          )}
          <div className="row wrap">
            <button onClick={saveProject}>Save project</button>
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
            <td className="n">{mods} desks</td>
            <td className="n">{seats} seats</td>
            <td className="muted">{d?.max ? 'as many as fit' : d ? `${d.quantity} requested` : ''}</td>
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
      <span><i className="sw" style={{ background: 'var(--paper)' }} />Free</span>
    </div>
  );
}
