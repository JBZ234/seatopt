// Evita una consola extra en Windows en compilaciones de producción.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    seatopt_lib::run()
}
