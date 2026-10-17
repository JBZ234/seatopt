//! Envoltura Tauri del núcleo `seat_core`.

use seat_core::api;
use serde_json::Value;
use std::path::PathBuf;

/// Despacho genérico: mismos comandos que el servidor de desarrollo.
#[tauri::command]
async fn call(cmd: String, args: Value) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || api::dispatch(&cmd, args)).await.map_err(|e| e.to_string())?
}

/// Importa un plano desde disco. Los DWG se convierten con ODA File Converter / LibreDWG.
#[tauri::command]
async fn import_path(path: String, converter: Option<String>) -> Result<api::ImportOut, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let lower = path.to_lowercase();
        if lower.ends_with(".dwg") {
            let conv = converter.filter(|c| !c.trim().is_empty()).map(PathBuf::from);
            api::import_dwg(&p, conv.as_deref())
        } else {
            let bytes = std::fs::read(&p).map_err(|e| format!("No se pudo leer {path}: {e}"))?;
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(path.clone());
            api::import_bytes(&name, &bytes)
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn save_text(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(|e| format!("No se pudo guardar {path}: {e}"))
}

#[tauri::command]
fn read_text(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("No se pudo leer {path}: {e}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![call, import_path, save_text, read_text])
        .run(tauri::generate_context!())
        .expect("error al iniciar la aplicación");
}
