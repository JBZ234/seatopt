// Paletas de color. «Verde» es la original (los valores exactos están en styles.css); las demás
// tiñen los fondos con su propio matiz, como el resaltado de los videos de YouTube.
export interface Palette { id: string; name: string; hue: number; sat: number; dark: string; light: string }

export const PALETTES: Palette[] = [
  { id: 'verde', name: 'Verde', hue: 152, sat: 1, dark: '#45b08f', light: '#1f6f5c' },
  { id: 'oceano', name: 'Océano', hue: 212, sat: 1.5, dark: '#5fa4e6', light: '#2363a6' },
  { id: 'ciruela', name: 'Ciruela', hue: 266, sat: 1.5, dark: '#a98cf5', light: '#6a4cc0' },
  { id: 'sakura', name: 'Sakura', hue: 330, sat: 1.6, dark: '#f08cc0', light: '#b33c7b' },
  { id: 'ambar', name: 'Ámbar', hue: 32, sat: 1.7, dark: '#e3a75b', light: '#a3651c' },
  { id: 'granate', name: 'Granate', hue: 356, sat: 1.5, dark: '#ec7d74', light: '#ad3a34' },
  { id: 'grafito', name: 'Grafito', hue: 218, sat: 0.55, dark: '#a3b3c7', light: '#46576d' },
];

const VARS = ['--vellum', '--paper', '--panel', '--rule', '--region', '--graphite', '--graphite-2', '--muted', '--mat', '--mat-ink'];

export function applyPalette(id: string, dark: boolean) {
  const root = document.documentElement;
  const p = PALETTES.find((x) => x.id === id) ?? PALETTES[0];
  root.dataset.palette = p.id;
  if (p.id === 'verde') {
    VARS.forEach((v) => root.style.removeProperty(v));
    return;
  }
  const h = p.hue;
  const s = (x: number) => `${Math.min(100, x * p.sat).toFixed(1)}%`;
  const hsl = (sat: number, l: number) => `hsl(${h} ${s(sat)} ${l}%)`;
  const v: Record<string, string> = dark
    ? {
        '--vellum': hsl(11, 9), '--panel': hsl(11, 11), '--paper': hsl(11, 13.5), '--rule': hsl(10, 21), '--region': hsl(12, 15),
        '--graphite': hsl(14, 91), '--graphite-2': hsl(9, 69), '--muted': hsl(6, 48), '--mat': p.dark, '--mat-ink': hsl(40, 8),
      }
    : {
        '--vellum': hsl(15, 93), '--panel': hsl(20, 97), '--paper': hsl(25, 98.6), '--rule': hsl(10, 85), '--region': hsl(25, 98.6),
        '--graphite': hsl(13, 17), '--graphite-2': hsl(7, 36), '--muted': hsl(5, 57), '--mat': p.light, '--mat-ink': hsl(50, 96),
      };
  for (const [k, val] of Object.entries(v)) root.style.setProperty(k, val);
}
