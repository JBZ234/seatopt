// Motor en el navegador (versión web): el mismo núcleo en Rust compilado a WebAssembly.
// Si la página tiene aislamiento de origen (SharedArrayBuffer), usa la compilación con hilos:
// rayon reparte la búsqueda en un Web Worker por núcleo, igual que la app de escritorio.
// Si no, usa la compilación de un solo hilo (mismo resultado, más lento).

type Engine = { call(cmd: string, args: string): string; import_bytes(name: string, bytes: Uint8Array): string; threads(): number };

let ready: Promise<{ eng: Engine; threads: number }> | null = null;

async function load() {
  const canThreads = (self as unknown as { crossOriginIsolated?: boolean }).crossOriginIsolated === true && typeof SharedArrayBuffer !== 'undefined';
  if (canThreads) {
    try {
      // @ts-ignore: se genera al compilar el motor (scripts/build-wasm.sh)
      const m = await import('./wasm/mt/seat.js');
      await m.default();
      const n = Math.max(1, Math.min(navigator.hardwareConcurrency || 4, 32));
      await (m as unknown as { initThreadPool(n: number): Promise<void> }).initThreadPool(n);
      return { eng: m as unknown as Engine, threads: m.threads() };
    } catch (e) {
      console.warn('[engine] no threads:', e);
    }
  }
  // @ts-ignore: se genera al compilar el motor (scripts/build-wasm.sh)
  const m = await import('./wasm/st/seat.js');
  await m.default();
  return { eng: m as unknown as Engine, threads: 1 };
}

type Msg = { id: number; cmd: string; args?: unknown; name?: string; bytes?: ArrayBuffer };

self.onmessage = async (ev: MessageEvent<Msg>) => {
  // Este mismo archivo también arranca los hilos de rayon (wasm-bindgen-rayon crea los workers
  // con la URL del worker actual): se reconoce por el mensaje de inicio.
  const d = ev.data as unknown as { type?: string; init?: unknown; receiver?: number };
  if (d && d.type === 'wasm_bindgen_worker_init') {
    // @ts-ignore: se genera al compilar el motor (scripts/build-wasm.sh)
    const m = (await import('./wasm/mt/seat.js')) as unknown as { default(i: unknown): Promise<unknown>; wbg_rayon_start_worker(r: number): void };
    await m.default(d.init);
    (self as unknown as Worker).postMessage({ type: 'wasm_bindgen_worker_ready' });
    m.wbg_rayon_start_worker(d.receiver as number);
    return;
  }
  const { id, cmd } = ev.data;
  try {
    ready ??= load();
    const { eng, threads } = await ready;
    let out: string;
    if (cmd === '__info') out = JSON.stringify({ ok: { threads } });
    else if (cmd === '__import') out = eng.import_bytes(ev.data.name ?? 'plano.dxf', new Uint8Array(ev.data.bytes!));
    else out = eng.call(cmd, JSON.stringify(ev.data.args ?? {}));
    (self as unknown as Worker).postMessage({ id, out });
  } catch (e) {
    (self as unknown as Worker).postMessage({ id, out: JSON.stringify({ error: String((e as Error)?.message ?? e) }) });
  }
};
