import { useState } from 'react';
import { loadDefaults } from '../actions';
import { Num, Switch } from '../components/ui';
import { uid, useApp } from '../store';
import type { Config, CustomRule, Predicate, Rule, SelectorKind, TargetKind } from '../types';

const PRED: Record<Predicate, string> = {
  distMax: 'a no más de', distMin: 'a no menos de', facingToward: 'mirando hacia', notBackTo: 'sin dar la espalda a',
  keepTogether: 'juntos entre sí, a no más de', countMax: 'como máximo (cantidad)',
};
const TARGET: Record<TargetKind, string> = {
  mainDoor: 'la puerta principal', exits: 'las salidas', officeDoors: 'las puertas de oficinas', windows: 'las ventanas',
  columns: 'las columnas', obstacleLabel: 'la oficina / zona llamada…', mainCorridor: 'el pasillo principal', seats: 'otros puestos de…',
};
const SEL: Record<SelectorKind, string> = { any: 'Todos los puestos', deskType: 'Puestos del tipo', role: 'Personas con rol', client: 'Personas del cliente' };

function selectorOptions(c: Config, k: SelectorKind) {
  if (k === 'deskType') return c.catalog.map((t) => ({ id: t.id, name: t.name }));
  if (k === 'role') return c.org.roles.map((t) => ({ id: t.id, name: t.name }));
  if (k === 'client') return c.org.clients.map((t) => ({ id: t.id, name: t.name }));
  return [];
}

// reglas donde cambiar dura/blanda tiene efecto; las demás son geométricas y siempre se cumplen
const SOFTABLE = new Set(['E7', 'C8', 'A5', 'R1']);
const WEIGHTED = (r: Rule) => !r.hard && (SOFTABLE.has(r.id) || r.id.startsWith('A') || r.id.startsWith('R'));

function RuleRow({ r }: { r: Rule }) {
  const st = useApp();
  const [open, setOpen] = useState(false);
  const upd = (f: (r: Rule) => void) => st.updateConfig((c) => { const x = c.rules.find((x) => x.id === r.id); if (x) f(x); });
  return (
    <div className={'rule' + (r.enabled ? '' : ' off')}>
      <div className="rule-head">
        <Switch on={r.enabled} label={`Activar ${r.name}`} onChange={(v) => upd((x) => { x.enabled = v; })} />
        <button className="rule-name" aria-expanded={open} onClick={() => setOpen(!open)}>
          <span className="rid">{r.id}</span>{r.name}
        </button>
        <button className={'lock' + (r.locked ? ' on' : '')} title={r.locked ? 'Bloqueada: el sugeridor nunca propone relajarla' : 'Relajable: el sugeridor puede proponer cambiarla'}
          aria-pressed={r.locked} onClick={() => upd((x) => { x.locked = !x.locked; })}>{r.locked ? '🔒' : '🔓'}</button>
      </div>
      {open && (
        <div className="rule-body">
          <p>{r.description}</p>
          <div className="field-row">
            {SOFTABLE.has(r.id) ? (
              <label>Tipo
                <select value={r.hard ? 'hard' : 'soft'} onChange={(e) => upd((x) => { x.hard = e.target.value === 'hard'; })}>
                  <option value="hard">Dura (se cumple siempre)</option>
                  <option value="soft">Blanda (con peso)</option>
                </select>
              </label>
            ) : (
              <span className="hint">{r.hard ? 'Se cumple siempre (geométrica).' : 'Se cumple tanto como se pueda.'}</span>
            )}
            {WEIGHTED(r) && <label>Peso<Num value={r.weight} unit="" step={0.5} min={0} width={48} onChange={(v) => upd((x) => { x.weight = v; })} /></label>}
          </div>
          {r.items.map((it) => (
            <div key={it.id} className={'item' + (it.enabled ? '' : ' off')} title={it.help}>
              <Switch small on={it.enabled} label={it.label} onChange={(v) => upd((x) => { x.items.find((i) => i.id === it.id)!.enabled = v; })} />
              <span className="item-label">{it.label}</span>
              {it.value !== null && (
                <Num value={it.value} unit={it.unit} step={it.step || 1} min={it.min} max={it.max} disabled={!it.enabled || !r.enabled}
                  label={it.label} onChange={(v) => upd((x) => { x.items.find((i) => i.id === it.id)!.value = v; })} />
              )}
              {it.help && <small>{it.help}</small>}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function CustomRuleEditor({ initial, onDone }: { initial?: CustomRule; onDone: () => void }) {
  const st = useApp();
  const c = st.config!;
  const [r, setR] = useState<CustomRule>(initial ?? {
    id: uid('U'), name: '', enabled: true, hard: false, weight: 2,
    subject: { kind: 'deskType', value: c.catalog[0]?.id ?? '' }, predicate: 'distMax',
    target: { kind: 'windows', value: '' }, value: 6000,
  });
  const set = (f: (x: CustomRule) => void) => setR((p) => { const n = structuredClone(p); f(n); return n; });
  const needsValue = r.predicate !== 'facingToward';
  const isCount = r.predicate === 'countMax';
  const noTarget = r.predicate === 'keepTogether' || isCount;
  const sentence = () => {
    const subj = r.subject.kind === 'any' ? 'Todos los puestos' : `${SEL[r.subject.kind]} «${selectorOptions(c, r.subject.kind).find((o) => o.id === r.subject.value)?.name ?? '?'}»`;
    const tgt = noTarget ? '' : r.target.kind === 'obstacleLabel' ? `«${r.target.value}»` : r.target.kind === 'seats' ? `puestos de «${selectorOptions(c, r.target.seats?.kind ?? 'any').find((o) => o.id === r.target.seats?.value)?.name ?? 'todos'}»` : TARGET[r.target.kind];
    const val = isCount ? ` ${r.value}` : needsValue ? ` ${(r.value / 1000).toFixed(1)} m` : '';
    return `${subj} ${PRED[r.predicate]}${r.predicate === 'keepTogether' ? val : isCount ? val : ` ${tgt}${r.predicate === 'notBackTo' ? ' dentro de' : ''}${r.predicate === 'facingToward' ? '' : val}`}`;
  };
  return (
    <div className="custom-editor">
      <label className="stack">Nombre
        <input value={r.name} placeholder="p. ej. Drafters cerca de ventanas" onChange={(e) => set((x) => { x.name = e.target.value; })} />
      </label>
      <div className="field-row wrap">
        <select value={r.subject.kind} aria-label="Quién" onChange={(e) => set((x) => { x.subject = { kind: e.target.value as SelectorKind, value: selectorOptions(c, e.target.value as SelectorKind)[0]?.id ?? '' }; })}>
          {Object.entries(SEL).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
        </select>
        {r.subject.kind !== 'any' && (
          <select value={r.subject.value} aria-label="Cuál" onChange={(e) => set((x) => { x.subject.value = e.target.value; })}>
            {selectorOptions(c, r.subject.kind).map((o) => <option key={o.id} value={o.id}>{o.name}</option>)}
          </select>
        )}
        <select value={r.predicate} aria-label="Condición" onChange={(e) => set((x) => { x.predicate = e.target.value as Predicate; })}>
          {Object.entries(PRED).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
        </select>
        {!noTarget && (
          <select value={r.target.kind} aria-label="Referencia" onChange={(e) => set((x) => { x.target = { kind: e.target.value as TargetKind, value: '', seats: e.target.value === 'seats' ? { kind: 'role', value: c.org.roles[0]?.id ?? '' } : null }; })}>
            {Object.entries(TARGET).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
          </select>
        )}
        {!noTarget && r.target.kind === 'obstacleLabel' && (
          <input value={r.target.value} placeholder="Sala de juntas" aria-label="Rótulo" onChange={(e) => set((x) => { x.target.value = e.target.value; })} />
        )}
        {!noTarget && r.target.kind === 'seats' && (
          <>
            <select value={r.target.seats?.kind} onChange={(e) => set((x) => { x.target.seats = { kind: e.target.value as SelectorKind, value: selectorOptions(c, e.target.value as SelectorKind)[0]?.id ?? '' }; })}>
              {Object.entries(SEL).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
            </select>
            {r.target.seats?.kind !== 'any' && (
              <select value={r.target.seats?.value} onChange={(e) => set((x) => { x.target.seats!.value = e.target.value; })}>
                {selectorOptions(c, r.target.seats?.kind ?? 'any').map((o) => <option key={o.id} value={o.id}>{o.name}</option>)}
              </select>
            )}
          </>
        )}
        {needsValue && <Num value={r.value} unit={isCount ? 'puestos' : 'mm'} onChange={(v) => set((x) => { x.value = v; })} />}
      </div>
      <p className="sentence">{sentence()}</p>
      <div className="field-row">
        <label>Tipo
          <select value={r.hard ? 'hard' : 'soft'} onChange={(e) => set((x) => { x.hard = e.target.value === 'hard'; })}>
            <option value="soft">Blanda</option><option value="hard">Dura</option>
          </select>
        </label>
        {!r.hard && <label>Peso<Num value={r.weight} unit="" width={48} onChange={(v) => set((x) => { x.weight = v; })} /></label>}
      </div>
      <div className="row">
        <button className="primary" disabled={!r.name.trim()} onClick={() => {
          st.updateConfig((cc) => {
            const i = cc.customRules.findIndex((x) => x.id === r.id);
            if (i >= 0) cc.customRules[i] = r; else cc.customRules.push(r);
          });
          onDone();
        }}>Guardar regla</button>
        <button className="ghost" onClick={onDone}>Cancelar</button>
      </div>
    </div>
  );
}

export default function RulesStep() {
  const st = useApp();
  const c = st.config;
  const [editing, setEditing] = useState<CustomRule | 'new' | null>(null);
  if (!c) return null;
  const groups = Array.from(new Set(c.rules.map((r) => r.group)));
  return (
    <div className="step-body">
      <h2>Reglas</h2>
      <p className="hint">Las duras nunca se incumplen. Las blandas se cumplen tanto como se pueda, según su peso. El candado impide que el sugeridor proponga relajarlas.</p>
      {groups.map((g) => (
        <section key={g} className="rule-group">
          <h3>{g}</h3>
          {c.rules.filter((r) => r.group === g).map((r) => <RuleRow key={r.id} r={r} />)}
        </section>
      ))}

      <section className="rule-group">
        <h3>Tus reglas</h3>
        <p className="hint">Combina quién (tipo de escritorio, rol o cliente), una condición y una referencia del plano. Las que usan roles o clientes se aplican al asignar personas.</p>
        {c.customRules.map((r) => (
          <div key={r.id} className={'rule' + (r.enabled ? '' : ' off')}>
            <div className="rule-head">
              <Switch on={r.enabled} label={`Activar ${r.name}`} onChange={(v) => st.updateConfig((cc) => { cc.customRules.find((x) => x.id === r.id)!.enabled = v; })} />
              <button className="rule-name" onClick={() => setEditing(r)}><span className="rid">{r.hard ? 'dura' : `peso ${r.weight}`}</span>{r.name}</button>
              <button className="ghost small danger" onClick={() => st.updateConfig((cc) => { cc.customRules = cc.customRules.filter((x) => x.id !== r.id); })}>Quitar</button>
            </div>
          </div>
        ))}
        {editing ? (
          <CustomRuleEditor initial={editing === 'new' ? undefined : editing} onDone={() => setEditing(null)} />
        ) : (
          <button onClick={() => setEditing('new')}>Crear regla</button>
        )}
      </section>

      <div className="next">
        <button className="ghost" onClick={async () => { const d = await loadDefaults(); st.updateConfig((cc) => { cc.rules = d.rules; }); }}>Restaurar valores estándar</button>
        <button className="primary" onClick={() => st.set({ step: 3 })}>Continuar: organización</button>
      </div>
    </div>
  );
}
