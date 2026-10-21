import { capacity } from '../actions';
import { Num, Switch } from '../components/ui';
import { ruleVal } from '../clear';
import { requestedSeats, STEP, uid, useApp } from '../store';
import type { DeskType } from '../types';

function CustomPreview({ t }: { t: DeskType }) {
  const body = t.body ?? [];
  const seats = t.seats ?? [];
  const pts = [...body, ...seats.map((s) => ({ x: s.x, y: s.y }))];
  const x0 = Math.min(...pts.map((p) => p.x)) - 300, x1 = Math.max(...pts.map((p) => p.x)) + 300;
  const y0 = Math.min(...pts.map((p) => p.y)) - 300, y1 = Math.max(...pts.map((p) => p.y)) + 300;
  const s = Math.min(160 / (x1 - x0), 92 / (y1 - y0));
  const ox = (170 - (x1 - x0) * s) / 2, oy = (100 - (y1 - y0) * s) / 2;
  const X = (x: number) => ox + (x - x0) * s, Y = (y: number) => oy + (y1 - y) * s;
  return (
    <svg viewBox="0 0 170 100" className="desk-prev" aria-hidden>
      <path className="desk-shape" fill={t.color} d={body.map((p, i) => `${i ? 'L' : 'M'}${X(p.x)} ${Y(p.y)}`).join('') + 'Z'} />
      {seats.map((c, i) => {
        const sd = { x: -c.facing.y, y: c.facing.x };
        const q = [[-1, -1], [1, -1], [1, 1], [-1, 1]].map(([a, b]) => ({ x: c.x + sd.x * a * 220 + c.facing.x * b * 200, y: c.y + sd.y * a * 220 + c.facing.y * b * 200 }));
        return <path key={i} className="chair-shape" d={q.map((p, j) => `${j ? 'L' : 'M'}${X(p.x)} ${Y(p.y)}`).join('') + 'Z'} />;
      })}
    </svg>
  );
}

function Preview({ t }: { t: DeskType }) {
  if (t.shape === 'custom' && t.body?.length) return <CustomPreview t={t} />;
  const k = t.flexible ? t.maxPerSide : t.perSide;
  const L = t.shape === 'l' ? t.seatWidth : t.seatWidth * k;
  const D = t.shape === 'l' ? Math.max(t.returnLength, t.depth) : t.depth * t.sides;
  const chair = 420;
  const W = L + 200, H = D + chair * 2 + 200;
  const s = Math.min(150 / W, 92 / H);
  const ox = (170 - W * s) / 2 + 100 * s, oy = (100 - H * s) / 2 + (chair + 100) * s;
  const r = (x: number, y: number, w: number, h: number, cls: string, fill?: string) => (
    <rect x={ox + x * s} y={oy + y * s} width={w * s} height={h * s} className={cls} fill={fill} />
  );
  const chairs: JSX.Element[] = [];
  if (t.shape === 'l') {
    const notchX = t.returnDepth, cw = 480;
    return (
      <svg viewBox="0 0 170 100" className="desk-prev" aria-hidden>
        <path className="desk-shape" fill={t.color} d={`M${ox} ${oy}h${L * s}v${t.depth * s}h${-(L - notchX) * s}v${(D - t.depth) * s}h${-notchX * s}Z`} />
        {r(notchX + (L - notchX - cw) / 2, t.depth + 120, cw, chair, 'chair-shape')}
      </svg>
    );
  }
  for (let i = 0; i < k; i++) {
    const x = i * t.seatWidth + (t.seatWidth - 480) / 2;
    chairs.push(<g key={'b' + i}>{r(x, D + 60, 480, chair - 60, 'chair-shape')}</g>);
    if (t.sides >= 2) chairs.push(<g key={'t' + i}>{r(x, -chair, 480, chair - 60, 'chair-shape')}</g>);
  }
  return (
    <svg viewBox="0 0 170 100" className="desk-prev" aria-hidden>
      {r(0, 0, L, D, 'desk-shape', t.color)}
      {t.sides >= 2 && <line x1={ox} x2={ox + L * s} y1={oy + t.depth * s} y2={oy + t.depth * s} className="desk-split" />}
      {chairs}
    </svg>
  );
}

export default function DesksStep() {
  const st = useApp();
  const c = st.config;
  if (!c) return null;
  const qty = (id: string) => c.demand.find((d) => d.typeId === id)?.quantity ?? 0;
  const isMax = (id: string) => !!c.demand.find((d) => d.typeId === id)?.max;
  const setQty = (id: string, q: number) => st.updateConfig((cc) => {
    const d = cc.demand.find((d) => d.typeId === id);
    if (d) d.quantity = Math.max(0, Math.round(q)); else cc.demand.push({ typeId: id, quantity: Math.max(0, Math.round(q)) });
    cc.demand = cc.demand.filter((d) => d.quantity > 0 || d.max);
  });
  const setMax = (id: string, on: boolean) => st.updateConfig((cc) => {
    const d = cc.demand.find((d) => d.typeId === id);
    if (d) d.max = on; else if (on) cc.demand.push({ typeId: id, quantity: 0, max: true });
    cc.demand = cc.demand.filter((d) => d.quantity > 0 || d.max);
  });
  const maxNames = c.catalog.filter((t) => isMax(t.id)).map((t) => t.name);
  const manual = (id: string) => c.pinned.filter((m) => m.typeId === id).length;
  const removeType = (id: string, name: string) => {
    const used = (st.layout?.modules.filter((m) => m.typeId === id).length ?? 0) + c.pinned.filter((m) => m.typeId === id).length;
    if (used && !confirm(`“${name}” has ${used} desk${used > 1 ? 's' : ''} on the plan. Delete the type and those desks?`)) return;
    st.snapshot();
    st.updateConfig((cc) => { cc.catalog = cc.catalog.filter((x) => x.id !== id); cc.demand = cc.demand.filter((d) => d.typeId !== id); cc.pinned = cc.pinned.filter((m) => m.typeId !== id); });
    if (st.layout) {
      const ids = new Set(st.layout.modules.filter((m) => m.typeId === id).map((m) => m.id));
      st.set({ layout: { ...st.layout, modules: st.layout.modules.filter((m) => !ids.has(m.id)), seats: st.layout.seats.filter((x) => !ids.has(x.moduleId)) } });
    }
    if (st.placeType === id && st.tool === 'placeDesk') st.set({ tool: 'select' });
  };
  const chairDefault = ruleVal(c, 'E1', 'muro') ?? 1000;
  // fondo de la silla y paso mínimo detrás (último tramo de “Pasillos hacia los puestos”)
  const chairDepth = c.rules.find((r) => r.id === 'E1')?.items.find((i) => i.id === 'silla')?.value ?? 500;
  const squeeze = ruleVal(c, 'C2', 'paso_sillas') ?? ruleVal(c, 'C2', 'ancho') ?? 600;
  const upd = (id: string, f: (t: DeskType) => void) => st.updateConfig((cc) => { const t = cc.catalog.find((t) => t.id === id); if (t) f(t); });
  const req = requestedSeats(c);
  const cap = (id: string) => st.capacity?.find((x) => x.typeId === id);

  return (
    <div className="step-body">
      <h2>How many desks?</h2>
      <p className="hint">Enter the number of the ones you know must go in (for example 5 L-shaped desks). For the others turn on <b>As many as fit</b>: the optimizer places all that fit. If you turn it on for a flexible type (islands, benches) and a single one (individual, pair), the flexible one fills the large areas and the single one fills the small gaps.</p>

      <div className="totals">
        <span className="big">{req}</span> fixed seats
        {maxNames.length > 0 && <span className="sub"> + as many {maxNames.join(', ')} as fit</span>}
        {st.areaM2 > 0 && req > 0 && !maxNames.length && <span className="sub"> · {(st.areaM2 / req).toFixed(1)} m² per seat</span>}
      </div>
      <div className="row">
        <button onClick={capacity} disabled={!!st.job || !st.plan}>Estimate capacity per type</button>
      </div>

      <div className="catalog">
        {c.catalog.map((t) => {
          const q = qty(t.id);
          const mx = isMax(t.id);
          const unit = t.flexible ? 'seats' : t.shape === 'l' || t.shape === 'custom' || t.sides * t.perSide === 1 ? 'desks' : `modules of ${t.sides * t.perSide}`;
          const cp = cap(t.id);
          return (
            <article key={t.id} className={'desk-card' + (q > 0 || mx ? ' used' : '') + (mx ? ' maxed' : '')}>
              <Preview t={t} />
              <div className="desk-main">
                <div className="desk-head">
                  <input className="desk-name" value={t.name} aria-label="Type name" onChange={(e) => upd(t.id, (x) => { x.name = e.target.value; })} />
                  <button className="ghost small danger icon-btn" title="Delete this desk type" aria-label={`Delete type ${t.name}`} onClick={() => removeType(t.id, t.name)}>
                    <svg viewBox="0 0 16 16" aria-hidden><path d="M3 4h10M6 4V2.5h4V4M4.5 4l.7 9.5h5.6l.7-9.5" /></svg>
                  </button>
                </div>
                <div className="manual-row">
                  <button className={'small' + (st.tool === 'placeDesk' && st.placeType === t.id ? ' active-tool' : '')}
                    title="Click on the plan to place it. It stays pinned: the optimization respects it and places the rest."
                    onClick={() => st.set(st.tool === 'placeDesk' && st.placeType === t.id ? { tool: 'select' } : { tool: 'placeDesk', placeType: t.id, selection: { modules: [], obstacles: [], doors: [], windows: [] } })}>
                    {st.tool === 'placeDesk' && st.placeType === t.id ? 'Done placing' : 'Place by hand'}
                  </button>
                  {manual(t.id) > 0 && <span className="cap">{manual(t.id)} placed by hand</span>}
                </div>
                <div className="qty">
                  {mx ? <span className="max-badge">{t.flexible || !c.catalog.some((o) => o.flexible && isMax(o.id)) ? 'as many as fit' : 'as many as fit · filler'}</span> : <Num value={q} unit={unit} width={56} min={0} label={`Number of ${t.name}`} onChange={(v) => setQty(t.id, v)} />}
                  <label className="inline max-toggle">As many as fit <Switch small on={mx} label={`As many ${t.name} as fit`} onChange={(v) => setMax(t.id, v)} /></label>
                  {cp && <span className="cap" title="Estimated capacity using only this type">~{cp.maxUnits} fit{t.flexible ? '' : ` (${cp.maxSeats} seats)`}</span>}
                </div>
                {t.shape === 'custom' ? (
                  <div className="row wrap">
                    <button className="small" onClick={() => st.set({ studio: { open: true, editId: t.id } })}>Edit in Studio</button>
                    <span className="cap">{(t.seatWidth / 1000).toFixed(2)} × {(t.depth / 1000).toFixed(2)} m · {t.seats?.length ?? 0} seat{(t.seats?.length ?? 0) === 1 ? '' : 's'}</span>
                  </div>
                ) : (
                <details>
                  <summary>Dimensions</summary>
                  <div className="dims">
                    <label>{t.shape === 'l' ? 'Width' : 'Width per seat'}<Num value={t.seatWidth} unit="mm" onChange={(v) => upd(t.id, (x) => { x.seatWidth = v; })} /></label>
                    <label>Depth<Num value={t.depth} unit="mm" onChange={(v) => upd(t.id, (x) => { x.depth = v; })} /></label>
                    {t.shape === 'l' ? (
                      <>
                        <label>Return length<Num value={t.returnLength} unit="mm" onChange={(v) => upd(t.id, (x) => { x.returnLength = v; })} /></label>
                        <label>Return depth<Num value={t.returnDepth} unit="mm" onChange={(v) => upd(t.id, (x) => { x.returnDepth = v; })} /></label>
                      </>
                    ) : (
                      <>
                        <label>Sides with seats
                          <select value={t.sides} onChange={(e) => upd(t.id, (x) => { x.sides = parseInt(e.target.value); })}>
                            <option value={1}>one</option><option value={2}>two (face to face)</option>
                          </select>
                        </label>
                        <label className="inline">Flexible length <Switch small on={t.flexible} label="Flexible length" onChange={(v) => upd(t.id, (x) => { x.flexible = v; })} /></label>
                        {t.flexible ? (
                          <>
                            <label>Min per side<Num value={t.minPerSide} unit="" min={1} onChange={(v) => upd(t.id, (x) => { x.minPerSide = Math.max(1, Math.round(v)); })} /></label>
                            <label>Max per side<Num value={t.maxPerSide} unit="" min={1} onChange={(v) => upd(t.id, (x) => { x.maxPerSide = Math.max(x.minPerSide, Math.round(v)); })} /></label>
                          </>
                        ) : (
                          <label>Seats per side<Num value={t.perSide} unit="" min={1} onChange={(v) => upd(t.id, (x) => { x.perSide = Math.max(1, Math.round(v)); })} /></label>
                        )}
                      </>
                    )}
                    <label>Color<input type="color" value={t.color} onChange={(e) => upd(t.id, (x) => { x.color = e.target.value; })} /></label>
                  </div>
                  <div className="dims clears">
                    <span className="dims-title">Free space around</span>
                    <label title="Space to get up from the chair. Empty = the one in “Rules”.">Chair side<Num value={t.clearChair ?? chairDefault} unit="mm" onChange={(v) => upd(t.id, (x) => { x.clearChair = v; })} /></label>
                    {(() => {
                      const left = (t.clearChair ?? chairDefault) - chairDepth;
                      const ok = left >= squeeze - 1;
                      return (
                        <span className={'clear-note' + (ok ? ' ok' : '')}>
                          {ok
                            ? `${(left / 1000).toFixed(2)} m remain behind the chair: people can walk behind it without moving it.`
                            : `${(Math.max(0, left) / 1000).toFixed(2)} m remain behind the chair: to walk behind it the chair has to move (allowed). To walk behind without moving it use ${((chairDepth + squeeze) / 1000).toFixed(2)} m.`}
                        </span>
                      );
                    })()}
                    {!(t.shape === 'straight' && t.sides >= 2) && <label>Back<Num value={t.clearBack ?? 0} unit="mm" min={0} onChange={(v) => upd(t.id, (x) => { x.clearBack = v; })} /></label>}
                    <label>Left<Num value={t.clearLeft ?? 0} unit="mm" min={0} onChange={(v) => upd(t.id, (x) => { x.clearLeft = v; })} /></label>
                    <label>Right<Num value={t.clearRight ?? 0} unit="mm" min={0} onChange={(v) => upd(t.id, (x) => { x.clearRight = v; })} /></label>
                  </div>
                </details>
                )}
              </div>
            </article>
          );
        })}
      </div>
      <div className="row wrap">
        <button onClick={() => st.updateConfig((cc) => { cc.catalog.push({ id: uid('t'), name: 'New desk', color: '#5a7d8c', shape: 'straight', seatWidth: 1200, depth: 600, sides: 1, perSide: 1, flexible: false, minPerSide: 1, maxPerSide: 1, returnLength: 0, returnDepth: 0 }); })}>Add straight type</button>
        <button onClick={() => st.updateConfig((cc) => { cc.catalog.push({ id: uid('t'), name: 'New L desk', color: '#a0613f', shape: 'l', seatWidth: 1600, depth: 600, sides: 1, perSide: 1, flexible: false, minPerSide: 1, maxPerSide: 1, returnLength: 1200, returnDepth: 600 }); })}>Add L type</button>
        <button className="studio-btn" onClick={() => st.set({ studio: { open: true } })}>Design in Studio…</button>
      </div>
      {c.pinned.length > 0 && (
        <p className="hint">{c.pinned.length} desk{c.pinned.length > 1 ? 's are' : ' is'} pinned on the plan: they count toward the numbers and the optimization places the rest around them.
          {' '}<button className="ghost small" onClick={() => { st.snapshot(); st.updateConfig((cc) => { cc.pinned = []; }); if (st.layout) st.set({ layout: { ...st.layout, modules: st.layout.modules.filter((m) => !m.pinned), seats: st.layout.seats.filter((s) => st.layout!.modules.some((m) => m.id === s.moduleId && !m.pinned)) } }); }}>Release the pinned ones</button>
        </p>
      )}
      <div className="next">
        <button className="primary" onClick={() => st.set({ step: STEP.rules, tool: 'select' })}>Continue: rules</button>
      </div>
    </div>
  );
}
