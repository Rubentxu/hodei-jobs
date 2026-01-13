#!/bin/bash
# Build script for Hodei Operator Web Dashboard

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "Building Hodei Operator Web Dashboard..."

# Build WASM
echo "Building WASM..."
cargo build -p hodei-operator-web --release --target wasm32-unknown-unknown

# Generate JS bindings
echo "Generating JS bindings..."
mkdir -p assets/pkg
wasm-bindgen --out-dir assets/pkg/ --target web target/wasm32-unknown-unknown/release/hodei_operator_web.wasm

echo "Build complete!"
echo ""
echo "Files generated in assets/pkg/:"
ls -la assets/pkg/

echo ""
echo "To serve locally:"
echo "  python3 -m http.server 8080 --directory assets"
