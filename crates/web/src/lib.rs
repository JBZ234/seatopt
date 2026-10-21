//! Puente WebAssembly: expone exactamente la misma API que los comandos de Tauri y el servidor
//! de desarrollo (`seat_core::api::dispatch`). La interfaz la usa desde un Web Worker.

use seat_core::api;
use wasm_bindgen::prelude::*;

#[cfg(all(target_arch = "wasm32", feature = "threads"))]
pub use wasm_bindgen_rayon::init_thread_pool;

fn wrap(r: Result<serde_json::Value, String>) -> String {
    match r {
        Ok(v) => serde_json::json!({ "ok": v }).to_string(),
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

/// Ejecuta un comando (`defaults`, `analyze`, `start_job`, `job_status`, …) con argumentos JSON.
/// Devuelve `{"ok": …}` o `{"error": "…"}`.
#[wasm_bindgen]
pub fn call(cmd: &str, args: &str) -> String {
    let args: serde_json::Value = match serde_json::from_str(args) {
        Ok(v) => v,
        Err(e) => return wrap(Err(format!("Invalid JSON: {e}"))),
    };
    let r = std::panic::catch_unwind(|| api::dispatch(cmd, args)).unwrap_or_else(|_| Err("Internal engine error".into()));
    wrap(r)
}

/// Importa un plano a partir de sus bytes (DXF o PDF vectorial).
#[wasm_bindgen]
pub fn import_bytes(name: &str, bytes: &[u8]) -> String {
    if name.to_lowercase().ends_with(".dwg") {
        return wrap(Err("The web version does not read DWG. Save it as DXF from AutoCAD (or use the desktop app).".into()));
    }
    wrap(api::import_bytes(name, bytes).map(|v| serde_json::to_value(v).unwrap()))
}

/// Cuántos hilos usa el motor (1 si el navegador no permite hilos).
#[wasm_bindgen]
pub fn threads() -> usize {
    seat_core::engine::search::thread_count()
}
