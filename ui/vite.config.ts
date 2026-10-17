import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// En modo navegador (`npm run dev`) la API la sirve `seatopt-devserver` en :8787.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, proxy: { '/api': 'http://127.0.0.1:8787' } },
  build: { target: 'es2021', chunkSizeWarningLimit: 2000 },
});
