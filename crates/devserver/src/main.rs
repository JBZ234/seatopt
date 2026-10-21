//! Servidor de desarrollo: expone la misma API que los comandos de Tauri sobre HTTP
//! y sirve la interfaz compilada (ui/dist). Uso: `cargo run -p seatopt-devserver -- [puerto] [carpeta_ui]`

use seat_core::api;
use tiny_http::{Header, Method, Response, Server};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let port = args.get(1).cloned().unwrap_or_else(|| "8787".into());
    let ui = args.get(2).cloned().unwrap_or_else(|| "ui/dist".into());
    let server = Server::http(format!("127.0.0.1:{port}")).expect("could not open the port");
    eprintln!("Seats optimizer: http://127.0.0.1:{port}  (UI from {ui})");
    for mut req in server.incoming_requests() {
        let url = req.url().to_string();
        let json = Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap();
        if req.method() == &Method::Post && url.starts_with("/api/") {
            let mut body = Vec::new();
            let _ = req.as_reader().read_to_end(&mut body);
            let path = url.trim_start_matches("/api/");
            let (cmd, query) = path.split_once('?').unwrap_or((path, ""));
            let result: Result<serde_json::Value, String> = if cmd == "import" {
                let name = query.split('&').find_map(|kv| kv.strip_prefix("name=")).map(decode).unwrap_or_else(|| "plano.dxf".into());
                api::import_bytes(&name, &body).map(|v| serde_json::to_value(v).unwrap())
            } else {
                match serde_json::from_slice::<serde_json::Value>(&body) {
                    Ok(args) => api::dispatch(cmd, args),
                    Err(e) => Err(format!("Invalid JSON: {e}")),
                }
            };
            let (code, payload) = match result {
                Ok(v) => (200, serde_json::json!({ "ok": v })),
                Err(e) => (200, serde_json::json!({ "error": e })),
            };
            let _ = req.respond(Response::from_string(payload.to_string()).with_status_code(code).with_header(json));
            continue;
        }
        // estáticos
        let rel = url.split('?').next().unwrap_or("/").trim_start_matches('/');
        let rel = if rel.is_empty() { "index.html" } else { rel };
        if rel.contains("..") {
            let _ = req.respond(Response::from_string("no").with_status_code(400));
            continue;
        }
        let path = std::path::Path::new(&ui).join(rel);
        let path = if path.exists() { path } else { std::path::Path::new(&ui).join("index.html") };
        match std::fs::read(&path) {
            Ok(bytes) => {
                let ct = match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
                    "html" => "text/html; charset=utf-8",
                    "js" => "text/javascript",
                    "css" => "text/css",
                    "svg" => "image/svg+xml",
                    "png" => "image/png",
                    "json" => "application/json",
                    _ => "application/octet-stream",
                };
                let _ = req.respond(Response::from_data(bytes).with_header(Header::from_bytes("Content-Type", ct).unwrap()));
            }
            Err(_) => {
                let _ = req.respond(Response::from_string("Build the interface first: cd ui && npm run build").with_status_code(404));
            }
        }
    }
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}
