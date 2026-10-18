import type { ReactNode } from 'react';
import { applySuggestion, loadDefaults, suggest } from '../actions';
import { Num, Switch } from '../components/ui';
import { useApp } from '../store';
import type { Rule, Suggestion } from '../types';

/** Dibujos simples de cada medida (la cota en color de acento). */
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

interface Space {
  key: string;
  title: string;
  desc: string;
  toggle?: string;
  items: [string, string, string][];
}

const SPACES: Space[] = [
  { key: 'C1', title: 'Pasillo principal', toggle: 'C1', desc: 'Une la entrada con las salidas. Se traza antes de poner escritorios.', items: [['C1', 'ancho', 'Ancho']] },
  { key: 'C2', title: 'Pasillos hacia los puestos', desc: 'Es la regla de acceso: cada puesto debe alcanzarse caminando desde la entrada por pasillos de este ancho; el último tramo (detrás de las sillas o por el costado, en los extremos de fila) puede ser más angosto. También vigila que ninguna puerta quede bloqueada.', items: [['C2', 'ancho', 'Ancho'], ['C2', 'paso_sillas', 'Último tramo, detrás de las sillas']] },
  { key: 'E1', title: 'Espacio para la silla', desc: 'Del borde del escritorio al muro, del lado donde se sienta la persona.', items: [['E1', 'muro', 'Distancia']] },
  { key: 'E2', title: 'Separación entre filas', desc: 'Entre los bordes de escritorios de filas vecinas.', items: [['E2', 'silla_silla', 'Sillas espalda con espalda'], ['E2', 'silla_espalda', 'Silla con espalda de escritorio']] },
  { key: 'E3', title: 'Corte de filas largas', toggle: 'E3', desc: 'Cada cuántos puestos seguidos se corta una fila para dejar un paso.', items: [['E3', 'max', 'Puestos seguidos por lado'], ['E3', 'corte', 'Ancho del paso']] },
  { key: 'C3', title: 'Entrada libre', toggle: 'C3', desc: 'Zona sin escritorios frente a la puerta principal.', items: [['C3', 'profundidad', 'Fondo'], ['C3', 'ancho', 'Ancho']] },
  { key: 'C4', title: 'Frente de las puertas', toggle: 'C4', desc: 'Además del barrido de la hoja, espacio libre frente a cada puerta.', items: [['C4', 'extra', 'Espacio adicional']] },
  { key: 'O13', title: 'Separación a columnas y oficinas', desc: 'Distancia mínima de escritorios y sillas a columnas y a muros de oficinas.', items: [['O1', 'dist', 'A columnas'], ['O3', 'dist', 'A oficinas fijas']] },
];

const fmtVal = (v: number | null, unit: string) => (v === null ? '' : unit === 'mm' ? `${(v / 1000).toFixed(2)} m` : `${v} ${unit}`);

export default function RulesStep() {
  const st = useApp();
  const c = st.config;
  if (!c) return null;
  const rule = (id: string) => c.rules.find((r) => r.id === id);
  const upd = (id: string, f: (r: Rule) => void) => st.updateConfig((cc) => { const r = cc.rules.find((x) => x.id === id); if (r) f(r); });
  const sugFor = (sp: Space): Suggestion[] => {
    const ids = new Set(sp.items.map((i) => i[0]).concat(sp.toggle ? [sp.toggle] : []));
    return (st.suggestions ?? []).filter((s) => ids.has(s.ruleId));
  };
  const others = (st.suggestions ?? []).filter((s) => !SPACES.some((sp) => sp.items.some((i) => i[0] === s.ruleId) || sp.toggle === s.ruleId));

  return (
    <div className="step-body">
      <h2>Espacios</h2>
      <p className="hint">Las medidas que necesita la gente para caminar y sentarse. Siempre se cumple que todos los puestos se alcancen desde la entrada y que ninguna puerta quede bloqueada.</p>

      <div className="row wrap">
        <button className="primary" disabled={!!st.job || !st.plan} onClick={suggest}>Buscar mejores valores</button>
      </div>
      {st.suggestions && st.suggestions.length === 0 && <p className="hint">Con estas medidas ningún ajuste individual gana puestos.</p>}

      <div className="spaces">
        {SPACES.map((sp) => {
          const on = sp.toggle ? rule(sp.toggle)?.enabled ?? false : true;
          const sg = sugFor(sp);
          return (
            <article key={sp.key} className={'space' + (on ? '' : ' off')}>
              <svg viewBox="0 0 120 70" className="space-dg" aria-hidden>{DIAGRAMS[sp.key]}</svg>
              <div className="space-body">
                <div className="space-head">
                  <h3>{sp.title}</h3>
                  {sp.toggle && <Switch on={on} label={`Usar: ${sp.title}`} onChange={(v) => upd(sp.toggle!, (r) => { r.enabled = v; })} />}
                </div>
                <p className="hint">{sp.desc}</p>
                {on && sp.items.map(([rid, iid, label]) => {
                  const it = rule(rid)?.items.find((i) => i.id === iid);
                  if (!it || it.value === null) return null;
                  return (
                    <label key={rid + iid} className="space-field">
                      <span>{label}</span>
                      <Num value={it.value} unit={it.unit} step={it.step || 50} min={it.min} max={it.max} label={label}
                        onChange={(v) => upd(rid, (r) => { const x = r.items.find((i) => i.id === iid)!; x.value = v; x.enabled = true; r.enabled = true; })} />
                    </label>
                  );
                })}
                {sg.map((s, i) => (
                  <div key={i} className="gain-row">
                    <span className="gain">+{s.gainSeats}</span>
                    <span>{s.newValue !== null ? `con ${fmtVal(s.newValue, rule(s.ruleId)?.items.find((x) => x.id === s.itemId)?.unit ?? 'mm')}` : 'si se desactiva'}</span>
                    <button className="small" onClick={() => applySuggestion(s)}>Aplicar</button>
                  </div>
                ))}
              </div>
            </article>
          );
        })}
      </div>

      {others.length > 0 && (
        <section className="suggestions">
          <h3>Otros cambios que ganan puestos</h3>
          <ul>
            {others.map((s, i) => (
              <li key={i}><span className="gain">+{s.gainSeats}</span><span className="desc">{s.description}</span><button className="small" onClick={() => applySuggestion(s)}>Aplicar</button></li>
            ))}
          </ul>
        </section>
      )}

      <div className="next">
        <button className="ghost" onClick={async () => { const d = await loadDefaults(); st.updateConfig((cc) => { cc.rules = d.rules; }); st.set({ suggestions: null }); }}>Restaurar medidas</button>
        <button className="primary" onClick={() => st.set({ step: 3, tool: 'select' })}>Continuar: organización</button>
      </div>
    </div>
  );
}
