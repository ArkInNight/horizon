#!/usr/bin/env bash
# Build ghost_litematic as a wasm .hmod against an installed Nightly's signed SDK and install it.
# Usage: ./build-wasm.sh [full-horizon-sha]
set -euo pipefail

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
SDK=${1:-2b77287de099c9316278ba7ac4d83590fdb7ef30}
DATA=${HORIZON_DATA:-/home/Endministrator/.local/share/horizon}
WORK=${TMPDIR:-/tmp}/ghost-litematic-wasm-build

rm -rf "$WORK"
mkdir -p "$WORK"
cd "$WORK"

# Scaffold a wasm mod: this fetches the SDK for $SDK into .horizon/sdk/.
cargo run --locked --manifest-path "$ROOT/Cargo.toml" -p cargo-horizon-mod -- \
  new ghost-wasm-build --template wasm --horizon "$SDK"
cd "$WORK/ghost-wasm-build"

cp "$HERE/src/lib.rs" src/lib.rs
cat > horizon-mod.toml <<'TOML'
id = "ghost_litematic"
name = "Litematic Ghost (PoC)"
version = "0.1.0"
description = "Load .litematic/.schem and draw a translucent ghost overlay."
authors = ["ArkInNight"]
api = "0.2"
side = "client"
capabilities = ["registries"]
TOML
python3 - <<'PY'
from pathlib import Path
p = Path('Cargo.toml')
s = p.read_text().replace(
    'horizon-mod-api = { path = ".horizon/sdk/api/crates/horizon-mod-api" }',
    'horizon-mod-api = { path = ".horizon/sdk/api/crates/horizon-mod-api" }\n'
    'flate2 = { version = "=1.1.10", default-features = false, features = ["rust_backend"] }',
)
p.write_text(s)
PY

mkdir -p data/ghost_litematic
cp "$HERE/fixtures/sample.litematic" data/ghost_litematic/build.litematic
cargo run --locked --manifest-path "$ROOT/Cargo.toml" -p cargo-horizon-mod -- build --wasm

mkdir -p "$DATA/horizon-mods" "$DATA/horizon-mod-data/ghost_litematic"
cp target/horizon-mod/ghost_litematic.hmod "$DATA/horizon-mods/"
cp "$HERE/fixtures/sample.litematic" "$DATA/horizon-mod-data/ghost_litematic/build.litematic"
grep -q '^mod\.ghost_litematic:' "$DATA/horizon-mods.txt" || \
  echo 'mod.ghost_litematic:true' >> "$DATA/horizon-mods.txt"
echo "Installed $DATA/horizon-mods/ghost_litematic.hmod"
