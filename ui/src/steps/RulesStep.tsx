import type { CSSProperties, ReactNode } from 'react';
import { applySuggestion, loadDefaults, suggest } from '../actions';
import { Fold, Num, Slider, Switch } from '../components/ui';
import { STEP, useApp } from '../store';
import type { Rule, Suggestion } from '../types';

/** Small drawings of each measure (the dimension in the accent color). */
const D = {
  desk: (x: number, y: number, w: number, h: number) => <rect x={x} y={y} width={w} height={h} className="dg-desk" />,
  chair: (x: number, y: number) => <rect x={x} y={y} width={9} height={7} rx={1.5} className="dg-chair" />,
  dim: (x1: number, y1: number, x2: number, y2: number) => (
    <g className="dg-dim">
      <line x1={x1} y1={y1} x2={x2} y2={y2} />
      <circle cx={x1} cy={y1} r={1.6} />
      <circle cx={x2} cy={y2} r={1.6} />
    </g>
  ),
};

const DIAGRAMS: Record<string, ReactNode> = {
  C1: (
    <>
      {D.desk(6, 8, 40, 14)}{D.desk(6, 48, 40, 14)}{D.desk(74, 8, 40, 14)}{D.desk(74, 48, 40, 14)}
      <rect x={0} y={28} width={120} height={14} className="dg-zone" />
      {D.dim(60, 28, 60, 42)}
    </>
  ),
  C2: (
    <>
      {D.desk(10, 10, 100, 12)}{[18, 38, 58, 78, 98].map((x) => <g key={x}>{D.chair(x - 4, 24)}</g>)}
      {D.desk(10, 52, 100, 12)}
      {D.dim(60, 33, 60, 52)}
    </>
  ),
  E1: (
    <>
      <line x1={8} y1={62} x2={112} y2={62} className="dg-wall" />
      {D.desk(20, 12, 80, 14)}{[34, 56, 78].map((x) => <g key={x}>{D.chair(x, 30)}</g>)}
      {D.dim(60, 26, 60, 62)}
    </>
  ),
  E2: (
    <>
      {D.desk(14, 6, 92, 12)}{[24, 52, 80].map((x) => <g key={'a' + x}>{D.chair(x, 20)}</g>)}
      {D.desk(14, 52, 92, 12)}{[24, 52, 80].map((x) => <g key={'b' + x}>{D.chair(x, 43)}</g>)}
      {D.dim(104, 18, 104, 52)}
    </>
  ),
  E3: (
    <>
      {D.desk(4, 26, 46, 16)}{D.desk(70, 26, 46, 16)}
      {[8, 20, 32, 74, 86, 98].map((x) => <g key={x}>{D.chair(x, 17)}{D.chair(x, 44)}</g>)}
      {D.dim(50, 34, 70, 34)}
    </>
  ),
  C3: (
    <>
      <line x1={4} y1={64} x2={116} y2={64} className="dg-wall" />
      <line x1={46} y1={64} x2={74} y2={64} className="dg-door" />
      <rect x={34} y={30} width={52} height={34} className="dg-zone" />
      {D.desk(4, 8, 28, 12)}{D.desk(88, 8, 28, 12)}
      {D.dim(30, 30, 30, 64)}
    </>
  ),
  C4: (
    <>
      <line x1={60} y1={4} x2={60} y2={66} className="dg-wall" />
      <line x1={60} y1={20} x2={60} y2={42} className="dg-door" />
      <path d="M60 20 A22 22 0 0 0 38 42" className="dg-swing" />
      <rect x={16} y={18} width={44} height={26} className="dg-zone" />
      {D.dim(16, 50, 38, 50)}
    </>
  ),
  E8: (
    <>
      <line x1={8} y1={8} x2={8} y2={64} className="dg-wall" />
      <rect x={8} y={8} width={14} height={56} className="dg-zone" />
      {D.desk(22, 14, 14, 44)}{[18, 38].map((y) => <g key={y}>{D.chair(40, y)}</g>)}
      {D.desk(70, 14, 14, 44)}{[18, 38].map((y) => <g key={'b' + y}>{D.chair(88, y)}</g>)}
      {D.dim(8, 66, 22, 66)}
    </>
  ),
  P1: (
    <>
      <line x1={4} y1={64} x2={116} y2={64} className="dg-wall" />
      <line x1={52} y1={64} x2={70} y2={64} className="dg-door" />
      <path d="M61 64 V36 H112" className="dg-path" />
      {D.desk(6, 8, 40, 12)}{D.desk(70, 8, 40, 12)}
      {D.dim(92, 29, 92, 43)}
    </>
  ),
  E7: (
    <>
      {[8, 30, 52].map((y) => <g key={y}>{D.desk(8, y, 48, 10)}{D.desk(64, y, 48, 10)}</g>)}
    </>
  ),
  O13: (
    <>
      <rect x={20} y={22} width={14} height={14} className="dg-col" />
      <rect x={14} y={16} width={26} height={26} className="dg-zone" />
      <rect x={70} y={6} width={44} height={36} className="dg-office" />
      {D.desk(60, 52, 54, 12)}
      {D.dim(90, 42, 90, 52)}
    </>
  ),
};

type Field = [ruleId: string, itemId: string, label: string];
interface Card { key: string; title: string; desc: string; toggle?: string; fields: Field[]; extra?: ReactNode }

/** Rules shown in the cards; the rest go to “More rules”. */
const SHOWN = ['C2', 'C7', 'E1', 'E2', 'E8', 'O1', 'O3', 'C4', 'C3', 'P1', 'C1', 'E3', 'E7', 'O4'];

const PRIORITY: { id: string; label: string; tidy: number; desc: string }[] = [
  { id: 'max', label: 'Most seats', tidy: 0, desc: 'Every seat counts: desks go into any gap that fits, even alone.' },
  { id: 'bal', label: 'Balanced', tidy: 1.2, desc: 'A lone desk or an extra row direction must be worth more than one seat. Cleaner layouts that give up a seat only when it really pays.' },
  { id: 'tidy', label: 'Tidy', tidy: 3, desc: 'Clean rows first: no lone desks or pairs, few row directions. May give up a few seats.' },
];

const fmtVal = (v: number | null, unit: string) => (v === null ? '' : unit === 'mm' ? `${(v / 1000).toFixed(2)} m` : `${v} ${unit}`);

export default function RulesStep() {
  const st = useApp();
  const c = st.config;
  if (!c) return null;
  const rule = (id: string) => c.rules.find((r) => r.id === id);
  const upd = (id: string, f: (r: Rule) => void) => { st.updateConfig((cc) => { const r = cc.rules.find((x) => x.id === id); if (r) f(r); }); st.set({ suggestions: null }); };
  const itemOn = (rid: string, iid: string) => !!rule(rid)?.items.find((i) => i.id === iid)?.enabled;
  const setItemOn = (rid: string, iid: string, v: boolean) => upd(rid, (r) => { const it = r.items.find((i) => i.id === iid); if (it) it.enabled = v; });
  const tidy = c.weights.tidy ?? 1.2;
  const prio = PRIORITY.reduce((b, p) => (Math.abs(p.tidy - tidy) < Math.abs(b.tidy - tidy) ? p : b), PRIORITY[1]);

  const field = ([rid, iid, label]: Field) => {
    const it = rule(rid)?.items.find((i) => i.id === iid);
    if (!it || it.value === null) return null;
    return (
      <label key={rid + iid} className="space-field">
        <span>{label}</span>
        <Num value={it.value} unit={it.unit} step={it.step || 50} min={it.min} max={it.max} label={label}
          onChange={(v) => upd(rid, (r) => { const x = r.items.find((i) => i.id === iid)!; x.value = v; x.enabled = true; r.enabled = true; })} />
      </label>
    );
  };
  const sugFor = (cd: Card): Suggestion[] => {
    const ids = new Set(cd.fields.map((f) => f[0]).concat(cd.toggle ? [cd.toggle] : []));
    return (st.suggestions ?? []).filter((s) => ids.has(s.ruleId));
  };
  const card = (cd: Card, i: number) => {
    const on = cd.toggle ? rule(cd.toggle)?.enabled ?? false : true;
    return (
      <article key={cd.key} className={'space' + (on ? '' : ' off')} style={{ '--i': i } as CSSProperties}>
        <svg viewBox="0 0 120 70" className="space-dg" aria-hidden>{DIAGRAMS[cd.key]}</svg>
        <div className="space-body">
          <div className="space-head">
            <h3>{cd.title}</h3>
            {cd.toggle && <Switch on={on} label={`Use: ${cd.title}`} onChange={(v) => upd(cd.toggle!, (r) => { r.enabled = v; })} />}
          </div>
          <p className="hint">{cd.desc}</p>
          {on && cd.fields.map(field)}
          {on && cd.extra}
          {sugFor(cd).map((s, k) => (
            <div key={k} className="gain-row">
              <span className="gain">+{s.gainSeats}</span>
              <span>{s.newValue !== null ? `with ${fmtVal(s.newValue, rule(s.ruleId)?.items.find((x) => x.id === s.itemId)?.unit ?? 'mm')}` : 'if turned off'}</span>
              <button className="small" onClick={() => applySuggestion(s)}>Apply</button>
            </div>
          ))}
        </div>
      </article>
    );
  };

  const people: Card[] = [
    { key: 'E1', title: 'Chair space', desc: 'From the desk edge to a wall or obstacle, on the side where the person sits.', fields: [['E1', 'muro', 'Distance']] },
    { key: 'E2', title: 'Space between rows', desc: 'Between the edges of desks in neighbouring rows.', fields: [['E2', 'silla_silla', 'Chairs back to back'], ['E2', 'silla_espalda', 'Chair to the back of a desk']] },
    { key: 'C2', title: 'Narrowest gap', desc: 'No seat may be boxed in: every chair is reached without squeezing through gaps narrower than this (chairs do not count, they move).', fields: [['C2', 'paso_sillas', 'Gap between desks']] },
  ];
  const clear: Card[] = [
    {
      key: 'E8', title: 'Walls', desc: 'Free band along the walls of the area. Each wall can have its own value: select the area outline on the plan.', fields: [],
      extra: (
        <>
          <label className="space-field"><span>Desks may touch walls</span><Switch small on={itemOn('E8', 'tocar')} label="Desks may touch walls on sides without chairs" onChange={(v) => { setItemOn('E8', 'tocar', v); upd('E8', (r) => { r.enabled = true; }); }} /></label>
          {!itemOn('E8', 'tocar') && field(['E8', 'separacion', 'Distance to walls'])}
        </>
      ),
    },
    { key: 'O13', title: 'Columns and offices', desc: 'Distance from desks and chairs to columns and to the walls of fixed offices.', fields: [['O1', 'dist', 'To columns'], ['O3', 'dist', 'To fixed offices']] },
    {
      key: 'C4', title: 'Doors', toggle: 'C4', desc: 'Free area in front of every door: the leaf swing plus some extra space.', fields: [['C4', 'extra', 'Extra space in front']],
      extra: (
        <>
          <label className="space-field"><span>Include the leaf swing</span><Switch small on={itemOn('C4', 'barrido')} label="Include the leaf swing" onChange={(v) => setItemOn('C4', 'barrido', v)} /></label>
          <label className="space-field"><span>Office doors too</span><Switch small on={itemOn('C4', 'oficinas')} label="Apply to office doors" onChange={(v) => setItemOn('C4', 'oficinas', v)} /></label>
          <label className="space-field"><span>Emergency exits too</span><Switch small on={itemOn('C4', 'salidas')} label="Apply to emergency exits" onChange={(v) => setItemOn('C4', 'salidas', v)} /></label>
        </>
      ),
    },
    { key: 'C3', title: 'Entrance lobby', toggle: 'C3', desc: 'An area without desks in front of the main entrance.', fields: [['C3', 'profundidad', 'Depth'], ['C3', 'ancho', 'Width']] },
  ];
  const circ: Card[] = [
    {
      key: 'P1', title: 'Circulation paths', toggle: 'P1', desc: 'The paths from step 2 stay clear of desks and chairs.', fields: [['P1', 'primary', 'Entrance paths'], ['P1', 'secondary', 'Office-to-office']],
      extra: <button className="small" onClick={() => st.set({ step: STEP.paths, tool: 'select' })}>Edit the paths</button>,
    },
    { key: 'C1', title: 'Main corridor', toggle: 'C1', desc: 'An extra straight corridor through the floor, laid out by the optimizer from the entrance to the exits.', fields: [['C1', 'ancho', 'Width']] },
  ];
  const style: Card[] = [
    { key: 'E7', title: 'One direction', toggle: 'E7', desc: 'All rows parallel, like a classic open office.', fields: [] },
    { key: 'E3', title: 'Break long rows', toggle: 'E3', desc: 'After this many seats in a row, leave a gap to walk through.', fields: [['E3', 'max', 'Seats in a row'], ['E3', 'corte', 'Gap width']] },
  ];
  let ci = 0;
  const others = c.rules.filter((r) => !SHOWN.includes(r.id));
  const groups = [...new Set(others.map((r) => r.group))];
  const otherSug = (st.suggestions ?? []).filter((s) => !SHOWN.includes(s.ruleId));

  return (
    <div className="step-body">
      <h2>Rules</h2>
      <p className="hint">What people need to sit and walk, and how tidy the layout should be. Always true: every seat can be reached and no door is blocked.</p>

      <section className="card prio">
        <h3>Priority</h3>
        <div className="seg prio-seg" role="radiogroup" aria-label="Priority">
          {PRIORITY.map((p) => (
            <button key={p.id} role="radio" aria-checked={prio.id === p.id} className={prio.id === p.id ? 'on' : ''}
              onClick={() => st.updateConfig((cc) => { cc.weights.tidy = p.tidy; })}>{p.label}</button>
          ))}
        </div>
        <p className="hint small" key={prio.id}>{prio.desc}</p>
      </section>

      <h3 className="section-label">Space for people</h3>
      <div className="spaces">{people.map((cd) => card(cd, ci++))}</div>

      <h3 className="section-label">Clearances</h3>
      <div className="spaces">{clear.map((cd) => card(cd, ci++))}</div>

      <h3 className="section-label">Circulation</h3>
      <div className="spaces">{circ.map((cd) => card(cd, ci++))}</div>

      <h3 className="section-label">Layout style</h3>
      <div className="spaces">{style.map((cd) => card(cd, ci++))}</div>
      <div className="sliders">
        <Slider label="Desks against walls" hint="How much single-sided desks are preferred with their back to a wall, an office or a column." value={c.weights.wall ?? 5} onChange={(v) => st.updateConfig((cc) => { cc.weights.wall = v; })} />
        <Slider label="Row order" hint="Prefer few row directions and aligned rows (a tie-breaker between layouts with the same seats)." value={c.weights.order} onChange={(v) => st.updateConfig((cc) => { cc.weights.order = v; })} />
      </div>

      <div className="row wrap">
        <button disabled={!!st.job || !st.plan} onClick={suggest}>Find values that gain seats</button>
      </div>
      {st.suggestions && st.suggestions.length === 0 && <p className="hint">With these measures no single change gains seats.</p>}
      {otherSug.length > 0 && (
        <section className="suggestions">
          <h3>Other changes that gain seats</h3>
          <ul>
            {otherSug.map((s, i) => (
              <li key={i}><span className="gain">+{s.gainSeats}</span><span className="desc">{s.description}</span><button className="small" onClick={() => applySuggestion(s)}>Apply</button></li>
            ))}
          </ul>
        </section>
      )}

      <Fold title={`More rules (${others.length})`}>
        {groups.map((g) => (
          <div key={g} className="more-group">
            <h4>{g}</h4>
            {others.filter((r) => r.group === g).map((r) => (
              <div key={r.id} className={'more-rule' + (r.enabled ? '' : ' off')}>
                <div className="space-head">
                  <b>{r.name}</b>
                  {!r.locked && <Switch small on={r.enabled} label={`Use: ${r.name}`} onChange={(v) => upd(r.id, (x) => { x.enabled = v; })} />}
                </div>
                <p className="hint small">{r.description}</p>
                {r.enabled && r.items.map((it) => (it.value === null
                  ? <label key={it.id} className="space-field"><span>{it.label}</span><Switch small on={it.enabled} label={it.label} onChange={(v) => setItemOn(r.id, it.id, v)} /></label>
                  : field([r.id, it.id, it.label])))}
              </div>
            ))}
          </div>
        ))}
      </Fold>

      <div className="next">
        <button className="ghost" onClick={async () => { const d = await loadDefaults(); st.updateConfig((cc) => { cc.rules = d.rules; cc.weights = { ...cc.weights, order: d.weights.order, wall: d.weights.wall, tidy: d.weights.tidy }; }); st.set({ suggestions: null }); }}>Restore defaults</button>
        <button className="primary" onClick={() => st.set({ step: STEP.org, tool: 'select' })}>Continue: organization</button>
      </div>
    </div>
  );
}
