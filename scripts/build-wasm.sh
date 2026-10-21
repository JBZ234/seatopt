#!/usr/bin/env bash
# Compila el motor (seat_core) a WebAssembly para la versión web, en dos variantes:
#   mt: con hilos (rayon sobre Web Workers; necesita aislamiento de origen, lo pone coi-serviceworker)
#   st: un solo hilo (respaldo para navegadores sin SharedArrayBuffer)
# Requisitos: Rust estable con el componente rust-src y el target wasm32-unknown-unknown, y
# wasm-bindgen-cli 0.2.129 (cargo install wasm-bindgen-cli --version 0.2.129 --locked).
# La app de escritorio no usa nada de esto: sigue compilando el motor nativo.
set -euo pipefail
cd "$(dirname "$0")/.."

export RUSTC_BOOTSTRAP=1   # -Z build-std (std con atomics) sin cambiar de toolchain
BASE_FLAGS='--cfg getrandom_backend="wasm_js"'
STD='-Z build-std=std,panic_abort'

echo "== motor web: un hilo"
RUSTFLAGS="$BASE_FLAGS" CARGO_TARGET_DIR=target/wasm-st \
  cargo build --release -p seat_web --target wasm32-unknown-unknown $STD

echo "== motor web: con hilos"
RUSTFLAGS="$BASE_FLAGS -C target-feature=+atomics,+bulk-memory,+mutable-globals -C link-arg=--max-memory=4294967296 -C link-arg=--shared-memory -C link-arg=--import-memory -C link-arg=--export=__wasm_init_tls -C link-arg=--export=__tls_size -C link-arg=--export=__tls_align -C link-arg=--export=__tls_base" \
  CARGO_TARGET_DIR=target/wasm-mt \
  cargo build --release -p seat_web --features threads --target wasm32-unknown-unknown $STD

rm -rf ui/src/wasm
for v in st mt; do
  wasm-bindgen --target web --out-dir ui/src/wasm/$v --out-name seat \
    target/wasm-$v/wasm32-unknown-unknown/release/seat_web.wasm
done
# wasm-bindgen-rayon importa el módulo principal como "../../.." desde sus snippets
printf "export * from './seat.js';\nexport { default } from './seat.js';\n" > ui/src/wasm/mt/index.js
echo "== listo: ui/src/wasm"
