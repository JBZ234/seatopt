import { reassign } from '../actions';
import { Fold, Num } from '../components/ui';
import { STEP, uid, useApp } from '../store';
import type { Noise, Organization, TargetKind } from '../types';

const PALETTE = ['#2f6f9f', '#c05a3a', '#4f8a3c', '#8a5bb0', '#c79a1e', '#2a8f8a', '#b04b78', '#6b6f2a', '#4a5d8f', '#9a6a4a'];
// paleta de clientes distinta de la de roles para que las dos vistas no se confundan
const CLIENT_PALETTE = ['#3b8f6e', '#d06d2c', '#7a4fa8', '#2d7fb8', '#c23b5a', '#8e8a2b', '#4f5fb0', '#b0743a', '#2f9aa0', '#9c4f8a'];
const NEAR: Partial<Record<TargetKind, string>> = { mainDoor: 'Main entrance', windows: 'Windows', exits: 'Exits', obstacleLabel: 'Office / area named…', mainCorridor: 'Main corridor' };

function example(): Organization {
  const roles = [
    { id: 'pm', name: 'Project manager', color: '#c05a3a', deskTypes: ['l_150'], noise: 'loud' as Noise, near: [] },
    { id: 'adm', name: 'Administrative', color: '#c79a1e', deskTypes: [], noise: 'loud' as Noise, near: [{ target: { kind: 'mainDoor' as TargetKind, value: '' }, maxDist: 10000 }] },
    { id: 'dr', name: 'Drafter', color: '#2f6f9f', deskTypes: [], noise: 'quiet' as Noise, near: [] },
  ];
  const clients = ['Client A', 'Client B', 'Client C', 'Client D', 'Client E'].map((n, i) => ({ id: 'c' + (i + 1), name: n, color: CLIENT_PALETTE[i % CLIENT_PALETTE.length] }));
  const teams = clients.flatMap((c) => [
    { clientId: c.id, roleId: 'pm', count: 1 },
    { clientId: c.id, roleId: 'adm', count: 1 },
    { clientId: c.id, roleId: 'dr', count: 20 },
  ]);
  return { roles, clients, teams, affinities: [{ a: 'pm', b: 'dr', value: 1 }] };
}

export default function OrgStep() {
  const st = useApp();
  const c = st.config;
  if (!c) return null;
  const org = c.org;
  const upd = (f: (o: Organization) => void) => st.updateConfig((cc) => f(cc.org));
  const count = (cid: string, rid: string) => org.teams.find((t) => t.clientId === cid && t.roleId === rid)?.count ?? 0;
  const setCount = (cid: string, rid: string, n: number) => upd((o) => {
    const t = o.teams.find((t) => t.clientId === cid && t.roleId === rid);
    if (t) t.count = Math.max(0, Math.round(n)); else o.teams.push({ clientId: cid, roleId: rid, count: Math.max(0, Math.round(n)) });
    o.teams = o.teams.filter((t) => t.count > 0);
  });
  const total = org.teams.reduce((a, t) => a + t.count, 0);
  const placed = st.layout?.kpis.seatsPlaced ?? 0;
  const rowsClients = [...org.clients, { id: '', name: 'No client', color: '#888' }];
  const aff = (a: string, b: string) => org.affinities.find((x) => (x.a === a && x.b === b) || (x.a === b && x.b === a))?.value ?? 0;

  return (
    <div className="step-body">
      <h2>Roles and clients</h2>
      <p className="hint">Optional. First all the desks are placed; then people are seated by role and client. The “Group by role and client” weight in Optimize decides how much it matters. Someone may end up outside their group if that improves the whole.</p>
      {org.roles.length === 0 && (
        <div className="row"><button onClick={() => upd((o) => Object.assign(o, example()))}>Load example: 5 clients with PM, administrative and drafters</button></div>
      )}

      <h3>Roles</h3>
      {org.roles.map((r) => (
        <div key={r.id} className="role-row">
          <input type="color" value={r.color} aria-label="Role color" onChange={(e) => upd((o) => { o.roles.find((x) => x.id === r.id)!.color = e.target.value; })} />
          <input className="grow" value={r.name} aria-label="Role name" onChange={(e) => upd((o) => { o.roles.find((x) => x.id === r.id)!.name = e.target.value; })} />
          <select value={r.noise} title="Noise level (rule R5)" onChange={(e) => upd((o) => { o.roles.find((x) => x.id === r.id)!.noise = e.target.value as Noise; })}>
            <option value="normal">Normal noise</option><option value="loud">Loud</option><option value="quiet">Needs quiet</option>
          </select>
          <button className="ghost small danger" aria-label="Remove role" onClick={() => upd((o) => { o.roles = o.roles.filter((x) => x.id !== r.id); o.teams = o.teams.filter((t) => t.roleId !== r.id); })}>Remove</button>
          <div className="chips" title="Desk types this role uses (rule R1). None = any.">
            {c.catalog.filter((t) => c.demand.some((d) => d.typeId === t.id)).map((t) => {
              const on = r.deskTypes.includes(t.id);
              return (
                <button key={t.id} className={'chip' + (on ? ' active' : '')} onClick={() => upd((o) => { const x = o.roles.find((x) => x.id === r.id)!; x.deskTypes = on ? x.deskTypes.filter((d) => d !== t.id) : [...x.deskTypes, t.id]; })}>{t.name}</button>
              );
            })}
          </div>
          <div className="near">
            {r.near.map((n, i) => (
              <span key={i} className="near-item">
                near {NEAR[n.target.kind] ?? n.target.kind}{n.target.value ? ` “${n.target.value}”` : ''} ≤
                <Num value={n.maxDist} unit="mm" width={48} onChange={(v) => upd((o) => { o.roles.find((x) => x.id === r.id)!.near[i].maxDist = v; })} />
                <button className="ghost small" aria-label="Remove" onClick={() => upd((o) => { o.roles.find((x) => x.id === r.id)!.near.splice(i, 1); })}>×</button>
              </span>
            ))}
            <select value="" aria-label="Add a nearness" onChange={(e) => {
              const k = e.target.value as TargetKind;
              if (!k) return;
              const value = k === 'obstacleLabel' ? prompt('Label of the office or area (as it appears on the plan):') ?? '' : '';
              upd((o) => { o.roles.find((x) => x.id === r.id)!.near.push({ target: { kind: k, value }, maxDist: 8000 }); });
            }}>
              <option value="">+ near…</option>
              {Object.entries(NEAR).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
            </select>
          </div>
        </div>
      ))}
      <button onClick={() => upd((o) => { o.roles.push({ id: uid('r'), name: 'New role', color: PALETTE[o.roles.length % PALETTE.length], deskTypes: [], noise: 'normal', near: [] }); })}>Add role</button>

      <h3>Clients</h3>
      <div className="clients">
        {org.clients.map((cl) => (
          <span key={cl.id} className="client">
            <input type="color" value={cl.color} aria-label="Client color" onChange={(e) => upd((o) => { o.clients.find((x) => x.id === cl.id)!.color = e.target.value; })} />
            <input value={cl.name} aria-label="Client name" onChange={(e) => upd((o) => { o.clients.find((x) => x.id === cl.id)!.name = e.target.value; })} />
            <button className="ghost small" aria-label="Remove client" onClick={() => upd((o) => { o.clients = o.clients.filter((x) => x.id !== cl.id); o.teams = o.teams.filter((t) => t.clientId !== cl.id); })}>×</button>
          </span>
        ))}
        <button onClick={() => upd((o) => { o.clients.push({ id: uid('c'), name: `Client ${o.clients.length + 1}`, color: CLIENT_PALETTE[o.clients.length % CLIENT_PALETTE.length] }); })}>Add client</button>
      </div>

      {org.roles.length > 0 && (
        <>
          <h3>People per client and role</h3>
          <div className="matrix-wrap">
            <table className="matrix">
              <thead><tr><th />{org.roles.map((r) => <th key={r.id}><i className="dot" style={{ background: r.color }} />{r.name}</th>)}</tr></thead>
              <tbody>
                {rowsClients.map((cl) => (
                  <tr key={cl.id || 'none'}>
                    <th><i className="dot" style={{ background: cl.color }} />{cl.name}</th>
                    {org.roles.map((r) => (
                      <td key={r.id}><Num value={count(cl.id, r.id)} unit="" width={44} min={0} label={`${cl.name} ${r.name}`} onChange={(v) => setCount(cl.id, r.id, v)} /></td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <p className={'stat-line' + (placed && total > placed ? ' warn' : '')}>
            <span className="big">{total}</span> people{placed ? ` · ${placed} seats placed` : ''}{placed && total > placed ? ` · ${total - placed} seats short` : ''}
          </p>

          <Fold title="Affinity between roles (rule R3)">
            <p className="hint">+2 = must be together, −2 = better apart. Applies within the same client.</p>
            <table className="matrix">
              <thead><tr><th />{org.roles.map((r) => <th key={r.id}>{r.name}</th>)}</tr></thead>
              <tbody>
                {org.roles.map((a, i) => (
                  <tr key={a.id}>
                    <th>{a.name}</th>
                    {org.roles.map((b, j) => (
                      <td key={b.id}>
                        {j < i ? null : (
                          <select value={aff(a.id, b.id)} onChange={(e) => upd((o) => {
                            o.affinities = o.affinities.filter((x) => !((x.a === a.id && x.b === b.id) || (x.a === b.id && x.b === a.id)));
                            const v = parseFloat(e.target.value);
                            if (v !== 0) o.affinities.push({ a: a.id, b: b.id, value: v });
                          })}>
                            {[-2, -1, 0, 1, 2].map((v) => <option key={v} value={v}>{v > 0 ? '+' + v : v}</option>)}
                          </select>
                        )}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </Fold>
        </>
      )}

      <div className="next">
        {st.layout && total > 0 && <button onClick={reassign}>Seat the people again</button>}
        <button className="primary" onClick={() => st.set({ step: STEP.optimize })}>Continue: optimize</button>
      </div>
    </div>
  );
}
