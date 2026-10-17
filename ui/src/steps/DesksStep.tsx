import { capacity } from '../actions';
import { Num, Switch } from '../components/ui';
import { requestedSeats, uid, useApp } from '../store';
import type { DeskType } from '../types';

function Preview({ t }: { t: DeskType }) {
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
  const upd = (id: string, f: (t: DeskType) => void) => st.updateConfig((cc) => { const t = cc.catalog.find((t) => t.id === id); if (t) f(t); });
  const req = requestedSeats(c);
  const cap = (id: string) => st.capacity?.find((x) => x.typeId === id);

  return (
    <div className="step-body">
      <h2>¿Cuántos escritorios ubicamos?</h2>
      <p className="hint">Pon la cantidad de los que ya sabes que van (por ejemplo 5 en L). En los demás marca <b>Máximo posible</b>: el optimizador mete todos los que quepan. Si marcas un tipo flexible (islas, bench) y uno suelto (individual, pareja), el flexible llena las áreas grandes y el suelto rellena los huecos pequeños.</p>

      <div className="totals">
        <span className="big">{req}</span> puestos fijos
        {maxNames.length > 0 && <span className="sub"> + el máximo posible de {maxNames.join(', ')}</span>}
        {st.areaM2 > 0 && req > 0 && !maxNames.length && <span className="sub"> · {(st.areaM2 / req).toFixed(1)} m² por puesto</span>}
      </div>
      <div className="row">
        <button onClick={capacity} disabled={!!st.job || !st.plan}>Estimar capacidad por tipo</button>
      </div>

      <div className="catalog">
        {c.catalog.map((t) => {
          const q = qty(t.id);
          const mx = isMax(t.id);
          const unit = t.flexible ? 'puestos' : t.shape === 'l' || t.sides * t.perSide === 1 ? 'escritorios' : `módulos de ${t.sides * t.perSide}`;
          const cp = cap(t.id);
          return (
            <article key={t.id} className={'desk-card' + (q > 0 || mx ? ' used' : '') + (mx ? ' maxed' : '')}>
              <Preview t={t} />
              <div className="desk-main">
                <input className="desk-name" value={t.name} aria-label="Nombre del tipo" onChange={(e) => upd(t.id, (x) => { x.name = e.target.value; })} />
                <div className="qty">
                  {mx ? <span className="max-badge">{t.flexible || !c.catalog.some((o) => o.flexible && isMax(o.id)) ? 'máximo posible' : 'máximo · relleno'}</span> : <Num value={q} unit={unit} width={56} min={0} label={`Cantidad de ${t.name}`} onChange={(v) => setQty(t.id, v)} />}
                  <label className="inline max-toggle">Máximo posible <Switch small on={mx} label={`Máximo posible de ${t.name}`} onChange={(v) => setMax(t.id, v)} /></label>
                  {cp && <span className="cap" title="Capacidad estimada si se usara solo este tipo">caben ~{cp.maxUnits}{t.flexible ? '' : ` (${cp.maxSeats} puestos)`}</span>}
                </div>
                <details>
                  <summary>Medidas</summary>
                  <div className="dims">
                    <label>{t.shape === 'l' ? 'Ancho' : 'Ancho por puesto'}<Num value={t.seatWidth} unit="mm" onChange={(v) => upd(t.id, (x) => { x.seatWidth = v; })} /></label>
                    <label>Fondo<Num value={t.depth} unit="mm" onChange={(v) => upd(t.id, (x) => { x.depth = v; })} /></label>
                    {t.shape === 'l' ? (
                      <>
                        <label>Largo del retorno<Num value={t.returnLength} unit="mm" onChange={(v) => upd(t.id, (x) => { x.returnLength = v; })} /></label>
                        <label>Fondo del retorno<Num value={t.returnDepth} unit="mm" onChange={(v) => upd(t.id, (x) => { x.returnDepth = v; })} /></label>
                      </>
                    ) : (
                      <>
                        <label>Lados con puestos
                          <select value={t.sides} onChange={(e) => upd(t.id, (x) => { x.sides = parseInt(e.target.value); })}>
                            <option value={1}>uno</option><option value={2}>dos (cara a cara)</option>
                          </select>
                        </label>
                        <label className="inline">Largo flexible <Switch small on={t.flexible} label="Largo flexible" onChange={(v) => upd(t.id, (x) => { x.flexible = v; })} /></label>
                        {t.flexible ? (
                          <>
                            <label>Mín. por lado<Num value={t.minPerSide} unit="" min={1} onChange={(v) => upd(t.id, (x) => { x.minPerSide = Math.max(1, Math.round(v)); })} /></label>
                            <label>Máx. por lado<Num value={t.maxPerSide} unit="" min={1} onChange={(v) => upd(t.id, (x) => { x.maxPerSide = Math.max(x.minPerSide, Math.round(v)); })} /></label>
                          </>
                        ) : (
                          <label>Puestos por lado<Num value={t.perSide} unit="" min={1} onChange={(v) => upd(t.id, (x) => { x.perSide = Math.max(1, Math.round(v)); })} /></label>
                        )}
                      </>
                    )}
                    <label>Color<input type="color" value={t.color} onChange={(e) => upd(t.id, (x) => { x.color = e.target.value; })} /></label>
                  </div>
                  <button className="ghost small danger" onClick={() => st.updateConfig((cc) => { cc.catalog = cc.catalog.filter((x) => x.id !== t.id); cc.demand = cc.demand.filter((d) => d.typeId !== t.id); })}>Quitar este tipo</button>
                </details>
              </div>
            </article>
          );
        })}
      </div>
      <div className="row wrap">
        <button onClick={() => st.updateConfig((cc) => { cc.catalog.push({ id: uid('t'), name: 'Nuevo escritorio', color: '#5a7d8c', shape: 'straight', seatWidth: 1200, depth: 600, sides: 1, perSide: 1, flexible: false, minPerSide: 1, maxPerSide: 1, returnLength: 0, returnDepth: 0 }); })}>Agregar tipo recto</button>
        <button onClick={() => st.updateConfig((cc) => { cc.catalog.push({ id: uid('t'), name: 'Nuevo escritorio en L', color: '#a0613f', shape: 'l', seatWidth: 1600, depth: 600, sides: 1, perSide: 1, flexible: false, minPerSide: 1, maxPerSide: 1, returnLength: 1200, returnDepth: 600 }); })}>Agregar tipo en L</button>
      </div>
      <div className="next">
        <button onClick={() => st.set({ step: 2 })}>Revisar espacios</button>
        <button className="primary" disabled={req === 0 && !maxNames.length} onClick={() => st.set({ step: 4 })}>Ir a optimizar</button>
      </div>
    </div>
  );
}
