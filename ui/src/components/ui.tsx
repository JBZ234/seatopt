import { useEffect, useState, type ReactNode } from 'react';

export function Switch({ on, onChange, label, small, disabled }: { on: boolean; onChange: (v: boolean) => void; label: string; small?: boolean; disabled?: boolean }) {
  return (
    <button type="button" role="switch" aria-checked={on} aria-label={label} title={label} disabled={disabled}
      className={'switch' + (on ? ' on' : '') + (small ? ' small' : '')} onClick={() => onChange(!on)}>
      <span />
    </button>
  );
}

/** Campo numérico. Las medidas en mm se editan en metros. */
export function Num({ value, unit, onChange, min, max, step, width = 72, disabled, label }: {
  value: number; unit: string; onChange: (v: number) => void; min?: number; max?: number; step?: number; width?: number; disabled?: boolean; label?: string;
}) {
  const isMm = unit === 'mm';
  const shown = isMm ? value / 1000 : value;
  const [txt, setTxt] = useState(fmt(shown));
  useEffect(() => setTxt(fmt(shown)), [shown]);
  const commit = () => {
    let v = parseFloat(txt.replace(',', '.'));
    if (!isFinite(v)) { setTxt(fmt(shown)); return; }
    if (isMm) v *= 1000;
    if (min !== undefined) v = Math.max(min, v);
    if (max !== undefined && max > 0) v = Math.min(max, v);
    onChange(v);
  };
  return (
    <span className="num">
      <input aria-label={label} disabled={disabled} style={{ width }} value={txt} inputMode="decimal"
        onFocus={(e) => e.target.select()}
        onChange={(e) => setTxt(e.target.value.replace(/^(-?)0+(?=\d)/, '$1'))} onBlur={commit} onKeyDown={(e) => {
          if (e.key === 'Enter') (e.target as HTMLInputElement).blur();
          if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
            e.preventDefault();
            const st = (isMm ? (step ?? 50) / 1000 : step ?? 1) * (e.key === 'ArrowUp' ? 1 : -1);
            const v = (parseFloat(txt.replace(',', '.')) || 0) + st;
            setTxt(fmt(v));
            onChange(isMm ? Math.round(v * 1000) : v);
          }
        }} />
      <em>{isMm ? 'm' : unit}</em>
    </span>
  );
}

function fmt(v: number) {
  if (!isFinite(v)) return '';
  return Number.isInteger(v) ? String(v) : String(Math.round(v * 1000) / 1000);
}

export function Fold({ title, children, open: o = false, right }: { title: ReactNode; children: ReactNode; open?: boolean; right?: ReactNode }) {
  const [open, setOpen] = useState(o);
  return (
    <section className={'fold' + (open ? ' open' : '')}>
      <header>
        <button type="button" className="fold-btn" aria-expanded={open} onClick={() => setOpen(!open)}>
          <span className="chev" aria-hidden>▸</span>{title}
        </button>
        {right}
      </header>
      {open && <div className="fold-body">{children}</div>}
    </section>
  );
}

export function Slider({ label, value, onChange, hint, max = 10 }: { label: string; value: number; onChange: (v: number) => void; hint?: string; max?: number }) {
  return (
    <label className="slider" title={hint}>
      <span className="slider-label">{label}</span>
      <input type="range" min={0} max={max} step={0.5} value={value} onChange={(e) => onChange(parseFloat(e.target.value))} />
      <span className="slider-val">{value.toFixed(1)}</span>
    </label>
  );
}

export function Bar({ value }: { value: number }) {
  return <div className="bar"><span style={{ width: `${Math.round(Math.max(0, Math.min(1, value)) * 100)}%` }} /></div>;
}
