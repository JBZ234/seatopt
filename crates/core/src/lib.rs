//! Núcleo del optimizador de puestos de trabajo.
/// Reloj: en escritorio es `std::time`; compilado a WebAssembly usa el reloj del navegador.
pub mod time {
    pub use web_time::{Duration, Instant, SystemTime, UNIX_EPOCH};
}
pub mod analyze;
pub mod defaults;
pub mod geom;
pub mod import;
pub mod model;
pub mod raster;
pub mod render;
pub mod assign;
pub mod engine;
pub mod api;
pub mod export;
pub mod pdf_out;
pub mod simple;
