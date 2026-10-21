import { useEffect, useState, type CSSProperties, type ReactNode } from 'react';

export function Switch({ on, onChange, label, small, disabled }: { on: boolean; onChange: (v: boolean) => void; label: string; small?: boolean; disabled?: boolean }) {
  return (
    <button type="button" role="switch" aria-checked={on} aria-label={label} title={label} disabled={disabled}
      className={'switch' + (on ? ' on' : '') + (small ? ' small' : '')} onClick={() => onChange(!on)}>
      <span />
    </button>
  );
}

/** Separador decimal que usa la persona (el último que escribió): se muestra igual. */
let decSep: '.' | ',' = '.';
try { if (localStorage.getItem('seatopt-dec') === ',') decSep = ','; } catch { /* sin almacenamiento */ }

/** Campo numérico. Las medidas en mm se editan en metros. Acepta punto o coma y guarda exactamente
 *  lo escrito; fuera del rango recomendado solo avisa (no cambia el valor). */
export function Num({ value, unit, onChange, min, max, step, width = 72, disabled, label }: {
  value: number; unit: string; onChange: (v: number) => void; min?: number; max?: number; step?: number; width?: number; disabled?: boolean; label?: string;
}) {
  const isMm = unit === 'mm';
  const shown = isMm ? value / 1000 : value;
  const [txt, setTxt] = useState(fmt(shown));
  useEffect(() => setTxt(fmt(shown)), [shown]);
  const parse = (t: string) => parseFloat(t.trim().replace(/\s/g, '').replace(',', '.'));
  // para medidas, el rango de la regla es una recomendación; para conteos sí es un límite
  const strict = !isMm;
  const out = isMm && ((min !== undefined && value < min - 0.5) || (max !== undefined && max > 0 && value > max + 0.5));
  const commit = () => {
    if (txt.includes(',')) decSep = ','; else if (txt.includes('.')) decSep = '.';
    try { localStorage.setItem('seatopt-dec', decSep); } catch { /* sin almacenamiento */ }
    let v = parse(txt);
    if (!isFinite(v)) { setTxt(fmt(shown)); return; }
    if (isMm) v = Math.round(v * 10000) / 10; // mm con un decimal, sin errores de coma flotante
    if (strict) {
      if (min !== undefined) v = Math.max(min, v);
      if (max !== undefined && max > 0) v = Math.min(max, v);
    } else if (v < 0) v = 0;
    onChange(v);
    setTxt(fmt(isMm ? v / 1000 : v));
  };
  const range = isMm && min !== undefined ? `Recommended: ${fmt(min / 1000)}${max ? `–${fmt(max / 1000)}` : ' or more'} m` : undefined;
  return (
    <span className={'num' + (out ? ' warn' : '')} title={out ? `Outside the recommended range. ${range}` : range}>
      <input aria-label={label} disabled={disabled} style={{ width }} value={txt} inputMode="decimal"
        onFocus={(e) => e.target.select()}
        onChange={(e) => setTxt(e.target.value.replace(/^(-?)0+(?=\d)/, '$1'))} onBlur={commit} onKeyDown={(e) => {
          if (e.key === 'Enter') (e.target as HTMLInputElement).blur();
          if (e.key === 'Escape') { setTxt(fmt(shown)); (e.target as HTMLInputElement).blur(); }
          if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
            e.preventDefault();
            const st = (isMm ? (step ?? 50) / 1000 : step ?? 1) * (e.key === 'ArrowUp' ? 1 : -1);
            const v = Math.round(((parse(txt) || 0) + st) * 10000) / 10000;
            setTxt(fmt(v));
            onChange(isMm ? Math.round(v * 10000) / 10 : v);
          }
        }} />
      <em>{isMm ? 'm' : unit}</em>
    </span>
  );
}

function fmt(v: number) {
  if (!isFinite(v)) return '';
  const s = Number.isInteger(v) ? String(v) : String(Math.round(v * 10000) / 10000);
  return decSep === ',' ? s.replace('.', ',') : s;
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
  const pct = Math.max(0, Math.min(100, (value / max) * 100));
  return (
    <label className="slider" title={hint}>
      <span className="slider-label">{label}</span>
      <span className="slider-track" style={{ '--p': `${pct}%` } as CSSProperties}>
        <span className="slider-fill" aria-hidden />
        <input type="range" min={0} max={max} step={0.5} value={value} onChange={(e) => onChange(parseFloat(e.target.value))} />
      </span>
      <span className="slider-val" key={value}>{value.toFixed(1)}</span>
    </label>
  );
}

export function Bar({ value }: { value: number }) {
  return <div className="bar"><span style={{ width: `${Math.round(Math.max(0, Math.min(1, value)) * 100)}%` }} /></div>;
}
