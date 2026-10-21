#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build --locked --release --target wasm32-unknown-unknown -p msx-wasm
rm -rf dist
mkdir -p dist
cp web/index.html web/style.css web/app.js web/saves.js LICENSE firmware/ROM-LICENSE.txt dist/
cp target/wasm32-unknown-unknown/release/msx_wasm.wasm dist/
cp firmware/cbios/*.rom firmware/cbios/CBIOS-LICENSE.txt firmware/cbios/CBIOS-PROVENANCE.txt dist/
cp -R demos dist/demos
cargo run --locked --quiet -p msx-core --example assets -- dist
