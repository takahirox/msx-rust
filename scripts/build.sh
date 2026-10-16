#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build --locked --release --target wasm32-unknown-unknown -p msx-wasm
mkdir -p dist
cp web/index.html web/style.css web/app.js LICENSE firmware/ROM-LICENSE.txt dist/
cp target/wasm32-unknown-unknown/release/msx_wasm.wasm dist/
cargo run --locked --quiet -p msx-core --example assets -- dist
