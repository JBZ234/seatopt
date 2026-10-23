import { useEffect, useRef, type CSSProperties } from 'react';
import { computePaths, mergePaths, removePathNode } from '../actions';
import { circWidth } from '../clear';
import { Num, Switch } from '../components/ui';
import { STEP, useApp } from '../store';
import type { CircPath, Door, Plan } from '../types';

/** Readable name of a door: its label, or "Office 3" / "Exit 1" by order of appearance. */
export function doorName(plan: Plan, id: string): string {
  if (!id) return 'Entrance';
  const d = plan.doors.find((x) => x.id === id);
  if (!d) return 'Door';
  // labels picked up from nearby text are only used if they read like a name
  if (d.label && /[A-Za-zÀ-ÿ]{3,}/.test(d.label)) return d.label;
  const same = plan.doors.filter((x) => x.kind === d.kind);
  const n = same.findIndex((x) => x.id === id) + 1;
  return `${d.kind === 'exit' ? 'Exit' : d.kind === 'main' ? 'Entrance' : 'Office'} ${n}`;
}

export function pathName(plan: Plan, p: CircPath) {
  return `${doorName(plan, p.from)} → ${doorName(plan, p.to)}`;
}

export default function PathsStep() {
  const st = useApp();
  const { plan, config } = st;
  const asked = useRef(false);
  const paths = plan?.paths ?? [];
  // first visit: calculate them
  useEffect(() => {
    if (!asked.current && plan && config && !paths.length && plan.doors.some((d: Door) => d.kind === 'main')) {
      asked.current = true;
      computePaths();
    }
  }, [plan, config, paths.length]);
  if (!plan || !config) return null;
  const p1 = config.rules.find((r) => r.id === 'P1');
  const on = p1?.enabled ?? false;
  const item = (id: string) => p1?.items.find((i) => i.id === id);
  const setItem = (id: string, v: number) => st.updateConfig((c) => { const r = c.rules.find((x) => x.id === 'P1'); const it = r?.items.find((i) => i.id === id); if (it) { it.value = v; it.enabled = true; } });
  const prim = paths.filter((p) => p.kind === 'primary');
  const sec = paths.filter((p) => p.kind === 'secondary');
  const sel = paths.find((p) => p.id === st.pathSel?.id) ?? null;
  const upd = (id: string, f: (p: CircPath) => void) => { st.updatePlan((pl) => { const q = pl.paths?.find((x) => x.id === id); if (q) f(q); }); st.set({ layout: null }); };
  const setKeep = (ids: string[], v: boolean) => { st.updatePlan((pl) => { pl.paths?.forEach((q) => { if (ids.includes(q.id)) q.keep = v; }); }); st.set({ layout: null }); };

  const row = (p: CircPath, i: number) => (
    <li key={p.id} className={'path-row' + (sel?.id === p.id ? ' cur' : '') + (p.keep ? '' : ' off')} style={{ '--i': i } as CSSProperties}>
      <input type="checkbox" checked={p.keep} aria-label={`Keep ${pathName(plan, p)}`} title="Kept paths stay clear of desks" onChange={(e) => setKeep([p.id], e.target.checked)} />
      <button className="link path-name" onClick={() => st.set({ pathSel: sel?.id === p.id ? null : { id: p.id, node: null } })}>
        {pathName(plan, p)}
        {p.edited && <span className="badge">edited</span>}
      </button>
      <span className="muted small">{(circWidth(config, p) / 1000).toFixed(2)} m</span>
    </li>
  );

  return (
    <div className="step-body">
      <h2>Circulation</h2>
      <p className="hint">Walking routes from the entrance to every office door and exit, drawn before the desks. The optimizer keeps the checked ones clear: no desk or chair goes on them.</p>

      <div className="row wrap">
        <button className="primary" onClick={() => computePaths()} disabled={!plan.doors.some((d) => d.kind === 'main')}>{paths.length ? 'Recalculate' : 'Calculate paths'}</button>
        {paths.length > 1 && <button className="ghost" title="Paths that run side by side become one band up to where they part" onClick={() => mergePaths()}>Merge similar paths</button>}
        <label className="inline-switch">
          <Switch on={st.showSecondary} label="Show office-to-office paths" onChange={(v) => st.set({ showSecondary: v, pathSel: v || sel?.kind !== 'secondary' ? st.pathSel : null })} />
          Office-to-office paths
        </label>
      </div>
      {paths.some((p) => p.edited) && <p className="hint small">Recalculate keeps the paths you edited by hand.</p>}

      <section className="card">
        <div className="space-head">
          <h3>Widths</h3>
          <Switch on={on} label="The optimizer keeps the paths clear" onChange={(v) => st.updateConfig((c) => { const r = c.rules.find((x) => x.id === 'P1'); if (r) r.enabled = v; })} />
        </div>
        {!on && <p className="hint small">Off: the paths are only drawn; desks may go on them.</p>}
        {item('primary')?.value != null && (
          <label className="space-field"><span>Entrance paths</span>
            <Num value={item('primary')!.value!} unit="mm" min={600} max={3000} step={50} label="Entrance paths width" onChange={(v) => setItem('primary', v)} /></label>
        )}
        {item('secondary')?.value != null && (
          <label className="space-field"><span>Office-to-office</span>
            <Num value={item('secondary')!.value!} unit="mm" min={600} max={3000} step={50} label="Office-to-office width" onChange={(v) => setItem('secondary', v)} /></label>
        )}
      </section>

      {sel && (
        <section className="card path-edit">
          <div className="space-head">
            <h3>{pathName(plan, sel)}</h3>
            <button className="ghost small" title="Esc" onClick={() => st.set({ pathSel: null })}>Done</button>
          </div>
          <p className="hint small">On the plan: drag a <b>node</b> to move it, drag the <b>bar</b> in the middle of a run to shift the whole run, double-click a run to add a node. Select a node and press Delete to remove it.</p>
          <label className="space-field"><span>Width of this path</span>
            <Num value={circWidth(config, sel)} unit="mm" min={600} max={3000} step={50} label="Width of this path" onChange={(v) => upd(sel.id, (q) => { q.width = v; })} />
          </label>
          <div className="row wrap">
            {sel.width ? <button className="small" onClick={() => upd(sel.id, (q) => { q.width = null; })}>Use the general width</button> : null}
            {st.pathSel?.node != null && st.pathSel.node > 0 && st.pathSel.node < sel.pts.length - 1 && <button className="small" onClick={removePathNode}>Delete node</button>}
            {sel.edited && <button className="small" onClick={() => computePaths(sel.id)}>Reset to calculated</button>}
            <button className="small" title={st.pathSel?.node != null ? 'The paths next to this one follow it up to the selected node, then part' : 'The paths next to this one follow it while they run alongside (select a node to stop there)'}
              onClick={() => mergePaths(sel.id, st.pathSel?.node ?? undefined)}>{st.pathSel?.node != null ? 'Merge others up to this node' : 'Merge others into this one'}</button>
            <button className="small" onClick={() => setKeep([sel.id], !sel.keep)}>{sel.keep ? 'Do not keep' : 'Keep'}</button>
          </div>
        </section>
      )}

      {prim.length > 0 && (
        <section>
          <h3 className="section-label">From the entrance ({prim.filter((p) => p.keep).length}/{prim.length} kept)</h3>
          <ul className="path-list">{prim.map(row)}</ul>
        </section>
      )}
      {st.showSecondary && sec.length > 0 && (
        <section>
          <div className="space-head">
            <h3 className="section-label">Office to office ({sec.filter((p) => p.keep).length}/{sec.length} kept)</h3>
            <span className="row">
              <button className="ghost small" onClick={() => setKeep(sec.map((p) => p.id), true)}>All</button>
              <button className="ghost small" onClick={() => setKeep(sec.map((p) => p.id), false)}>None</button>
            </span>
          </div>
          <p className="hint small">Secondary routes. Check the ones the optimizer should keep clear.</p>
          <ul className="path-list">{sec.map(row)}</ul>
        </section>
      )}
      {st.showSecondary && !sec.length && paths.length > 0 && <p className="hint">There are fewer than two office doors: no office-to-office paths.</p>}

      <div className="next">
        <button className="primary" onClick={() => st.set({ step: STEP.desks, tool: 'select', pathSel: null })}>Continue: desks</button>
      </div>
    </div>
  );
}
