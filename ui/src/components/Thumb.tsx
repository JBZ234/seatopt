import { useMemo } from 'react';
import type { Config, Layout, Plan, Pt } from '../types';

const ring = (r: Pt[]) => (r.length ? `M${r.map((p) => `${p.x.toFixed(0)} ${(-p.y).toFixed(0)}`).join('L')}Z` : '');

/** Vista previa pequeña de una distribución (plano + escritorios). */
export default function Thumb({ plan, layout, config, width = 148, height = 96 }: { plan: Plan; layout: Layout; config: Config; width?: number; height?: number }) {
  const vb = useMemo(() => {
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    for (const r of plan.region) for (const p of r) { x0 = Math.min(x0, p.x); x1 = Math.max(x1, p.x); y0 = Math.min(y0, -p.y); y1 = Math.max(y1, -p.y); }
    const m = Math.max(x1 - x0, y1 - y0) * 0.03;
    return `${x0 - m} ${y0 - m} ${x1 - x0 + 2 * m} ${y1 - y0 + 2 * m}`;
  }, [plan.region]);
  const color = useMemo(() => Object.fromEntries(config.catalog.map((t) => [t.id, t.color])), [config.catalog]);
  return (
    <svg className="thumb" viewBox={vb} width={width} height={height} preserveAspectRatio="xMidYMid meet" aria-hidden>
      <path className="t-region" d={plan.region.map(ring).join('')} fillRule="evenodd" />
      {plan.obstacles.map((o) => <path key={o.id} className={'t-ob ' + o.kind} d={ring(o.poly)} />)}
      {layout.modules.map((m) => <path key={m.id} d={ring(m.body)} fill={color[m.typeId] ?? '#888'} />)}
    </svg>
  );
}
