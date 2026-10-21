import { defineConfig, type Plugin } from 'vite';
import react from '@vitejs/plugin-react';

// Tres formas de correr la misma interfaz:
//  - App de escritorio (Tauri): `npm run tauri dev` / `npm run tauri build`.
//  - Navegador con el servidor de desarrollo (`npm run dev`): la API la sirve `seatopt-devserver` en :8787.
//  - Versión web (GitHub Pages): `npm run build:web` (modo «web», ver .env.web). El motor va compilado a WebAssembly dentro de la
//    página (ver `crates/web` y `scripts/build-wasm.sh`). BASE es la ruta del sitio, p. ej. /seatopt/.

/** GitHub Pages no deja poner cabeceras: este service worker las agrega para poder usar hilos. */
const coi = (web: boolean): Plugin => ({
  name: 'coi-serviceworker',
  transformIndexHtml: (html) => (web ? html.replace('<head>', '<head>\n    <script src="./coi-serviceworker.min.js"></script>') : html),
});

/** En escritorio el motor WebAssembly no existe: se reemplaza por un módulo vacío al empaquetar. */
const wasmStub = (web: boolean): Plugin => ({
  name: 'wasm-stub',
  resolveId: (src) => (!web && /\/wasm\/(st|mt)\/seat\.js$/.test(src) ? '\0wasm-stub' : null),
  load: (id) => (id === '\0wasm-stub' ? 'export default async () => { throw new Error("motor web no incluido"); }; export const threads = () => 1;' : null),
});

export default defineConfig(({ mode }) => {
  const web = mode === 'web';
  return {
  base: web ? process.env.BASE || './' : '/',
  plugins: [react(), coi(web), wasmStub(web)],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    proxy: { '/api': 'http://127.0.0.1:8787' },
    // para probar la versión web con hilos en local (`npm run dev:web`)
    headers: web ? { 'Cross-Origin-Opener-Policy': 'same-origin', 'Cross-Origin-Embedder-Policy': 'require-corp' } : undefined,
  },
  worker: { format: 'es', plugins: () => [wasmStub(web)] },
  optimizeDeps: { exclude: ['./src/wasm'] },
  build: { target: 'es2021', chunkSizeWarningLimit: 2000, outDir: web ? 'dist-web' : 'dist' },
};
});
