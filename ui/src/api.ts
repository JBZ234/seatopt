// Puente con el núcleo en Rust: comandos de Tauri en la app de escritorio,
// o HTTP contra `seatopt-devserver` cuando se usa en el navegador.
import type { ImportOut, JobKind, JobState, Problem } from './types';

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export const backendName = isTauri ? 'escritorio' : 'navegador';

async function tauriInvoke<T>(cmd: string, args: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(cmd, args);
}

export async function call<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  if (isTauri) return tauriInvoke<T>('call', { cmd, args });
  const r = await fetch(`/api/${cmd}`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(args) });
  const j = await r.json();
  if (j.error) throw new Error(j.error);
  return j.ok as T;
}

/** Abre un plano (DXF, DWG o PDF). Devuelve null si el usuario cancela. */
export async function openPlan(converter?: string): Promise<ImportOut | null> {
  if (isTauri) {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const path = await open({ multiple: false, filters: [{ name: 'Planos', extensions: ['dxf', 'dwg', 'pdf'] }] });
    if (!path || Array.isArray(path)) return null;
    return tauriInvoke<ImportOut>('import_path', { path, converter: converter || null });
  }
  const file = await pickFile('.dxf,.dwg,.pdf');
  if (!file) return null;
  return importBytes(file.name, await file.arrayBuffer());
}

export async function importBytes(name: string, bytes: ArrayBuffer): Promise<ImportOut> {
  const r = await fetch(`/api/import?name=${encodeURIComponent(name)}`, { method: 'POST', body: bytes });
  const j = await r.json();
  if (j.error) throw new Error(j.error);
  return j.ok as ImportOut;
}

function pickFile(accept: string): Promise<File | null> {
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = accept;
    input.onchange = () => resolve(input.files?.[0] ?? null);
    input.oncancel = () => resolve(null);
    input.click();
  });
}

/** Guarda texto: diálogo nativo en escritorio, descarga en navegador. */
export async function saveText(defaultName: string, content: string, ext: string): Promise<string | null> {
  if (isTauri) {
    const { save } = await import('@tauri-apps/plugin-dialog');
    const path = await save({ defaultPath: defaultName, filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
    if (!path) return null;
    await tauriInvoke('save_text', { path, content });
    return path;
  }
  const blob = new Blob([content], { type: 'text/plain;charset=utf-8' });
  const a = document.createElement('a');
  a.href = URL.createObjectURL(blob);
  a.download = defaultName;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 2000);
  return defaultName;
}

/** Guarda bytes en base64 (PDF). */
export async function saveBase64(defaultName: string, b64: string, ext: string): Promise<string | null> {
  if (isTauri) {
    const { save } = await import('@tauri-apps/plugin-dialog');
    const path = await save({ defaultPath: defaultName, filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
    if (!path) return null;
    await tauriInvoke('save_base64', { path, data: b64 });
    return path;
  }
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  const a = document.createElement('a');
  a.href = URL.createObjectURL(new Blob([bytes], { type: ext === 'pdf' ? 'application/pdf' : 'application/octet-stream' }));
  a.download = defaultName;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 4000);
  return defaultName;
}

export async function openText(ext: string): Promise<string | null> {
  if (isTauri) {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const path = await open({ multiple: false, filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
    if (!path || Array.isArray(path)) return null;
    return tauriInvoke<string>('read_text', { path });
  }
  const f = await pickFile('.' + ext);
  return f ? f.text() : null;
}

export async function startJob(kind: JobKind, problem: Problem, n?: number): Promise<string> {
  return call<string>('start_job', { kind, problem, n });
}

/** Sondea un trabajo hasta que termine, reportando progreso. */
export async function runJob<T>(kind: JobKind, problem: Problem, onProgress: (s: JobState<T>) => void, opts: { n?: number; signal?: { cancel: boolean } } = {}): Promise<T> {
  const id = await startJob(kind, problem, opts.n);
  for (;;) {
    await new Promise((r) => setTimeout(r, 250));
    if (opts.signal?.cancel) {
      await call('cancel_job', { jobId: id });
      opts.signal.cancel = false;
    }
    const st = await call<JobState<T>>('job_status', { jobId: id });
    onProgress(st);
    if (st.done) {
      if (st.error) throw new Error(st.error);
      return st.result as T;
    }
  }
}
