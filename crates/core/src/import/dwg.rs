//! DWG es un formato cerrado de Autodesk. Se convierte a DXF con un convertidor externo:
//!  - ODA File Converter (gratuito, Windows/macOS/Linux): https://www.opendesign.com/guestfiles/oda_file_converter
//!  - LibreDWG (`dwg2dxf`, código abierto)
//! La app busca el convertidor en rutas típicas o usa la ruta configurada por el usuario.

use std::path::{Path, PathBuf};
use std::process::Command;

pub fn candidate_converters() -> Vec<PathBuf> {
    let mut v = Vec::new();
    #[cfg(target_os = "windows")]
    {
        for base in ["C:\\Program Files\\ODA", "C:\\Program Files (x86)\\ODA"] {
            if let Ok(rd) = std::fs::read_dir(base) {
                for e in rd.flatten() {
                    let p = e.path().join("ODAFileConverter.exe");
                    if p.exists() {
                        v.push(p);
                    }
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        v.push(PathBuf::from("/Applications/ODAFileConverter.app/Contents/MacOS/ODAFileConverter"));
    }
    #[cfg(target_os = "linux")]
    {
        v.push(PathBuf::from("/usr/bin/ODAFileConverter"));
        v.push(PathBuf::from("/usr/local/bin/ODAFileConverter"));
    }
    v.push(PathBuf::from("dwg2dxf"));
    v
}

fn is_oda(p: &Path) -> bool {
    p.file_name().map(|n| n.to_string_lossy().to_lowercase().contains("odafileconverter")).unwrap_or(false)
}

/// Convierte un DWG a DXF y devuelve los bytes del DXF.
pub fn convert_dwg(dwg_path: &Path, converter: Option<&Path>) -> Result<Vec<u8>, String> {
    let mut tried = Vec::new();
    let list: Vec<PathBuf> = match converter {
        Some(c) => vec![c.to_path_buf()],
        None => candidate_converters(),
    };
    let tmp = std::env::temp_dir().join(format!("seatopt_dwg_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    for conv in list {
        let out_dxf = tmp.join(dwg_path.file_stem().unwrap_or_default()).with_extension("dxf");
        let _ = std::fs::remove_file(&out_dxf);
        let status = if is_oda(&conv) {
            let in_dir = dwg_path.parent().unwrap_or(Path::new("."));
            let filter = dwg_path.file_name().unwrap().to_string_lossy().to_string();
            Command::new(&conv)
                .arg(in_dir)
                .arg(&tmp)
                .arg("ACAD2018")
                .arg("DXF")
                .arg("0")
                .arg("1")
                .arg(filter)
                .status()
        } else {
            Command::new(&conv).arg("-y").arg("-o").arg(&out_dxf).arg(dwg_path).status()
        };
        match status {
            Ok(_) if out_dxf.exists() => {
                return std::fs::read(&out_dxf).map_err(|e| e.to_string());
            }
            Ok(s) => tried.push(format!("{} (code {:?})", conv.display(), s.code())),
            Err(e) => tried.push(format!("{} ({e})", conv.display())),
        }
    }
    Err(format!(
        "The DWG could not be converted. Install ODA File Converter (free) or LibreDWG and, if needed, enter its path in Settings. Attempts: {}",
        tried.join("; ")
    ))
}
