# ghost_litematic — litematica-lite PoC

See [COMPARISON.md](COMPARISON.md) for a feature-by-feature comparison with the Java
Litematica/MaLiLib jars in the project root, including what is rewritten, partial, or not possible
client-side.

A native Horizon mod that reads a `.litematic` **or** a Sponge `.schem` and draws its blocks as a
translucent ghost overlay. This is a **proof of concept**: no engine changes, no mixins, only the
public mod API (`render_level_stage`, `input_key`, `register_gui_layers`, `gui_layer_draw`, `data_files`).

## What works

- Loads `build.litematic` (gzipped, named-root NBT, `BlockStatePalette` + packed `BlockStates`)
  or `build.schem` (Sponge v1–3, palette + LEB128 varint indices) from the mod's data folder.
- One translucent quad per non-air block, plus a wireframe box per region/schematic.
- Camera-distance culling and the API's 4,096-quad batch cap.
- Controls are **registered key mappings**: open Options > Controls, find the `ghost_litematic`
  section, and rebind or reset any of them. Defaults are listed below.
- Layer / slice controls:
  - `O` — all layers
  - `P` — single layer
  - `H` — slice that follows the player's feet
  - `R` — a range of layers around the current one
  - `[` / `]` — move the current layer down / up
  - `-` / `=` — shrink / grow the range thickness
- Defaults: `G` toggle, `L` reload, `C` config screen, `V` verifier, `E` easy-place, `B` place next,
  `T` auto-place, `F` pick block, `O/P/H/R` layer modes, `[`/`]` layer, `-`/`=` thickness, `Z/X`, `K/I`, `,`/`.`
  move the schematic on X/Y/Z. The config screen has `X-/X+/Y-/Y+/Z-/Z+` buttons as well.
- The first load is anchored near the player, so test schematics no longer appear high in the sky.

## Not in this PoC

- Exact vanilla ghost block models (drawn as flat unlit quads).
- Block entities, entities, per-region visibility lists.
- Easy-place mode, schematic verifier, bulk paste into the world (client mods cannot write
  blocks — that needs a companion server mod).

## Install / test

1. Build and test the crate:
   ```sh
   cargo test --manifest-path mods/ghost_litematic/Cargo.toml
   ```
   A real installable `.hmod` needs a signed SDK for the pinned build; `cargo horizon-mod check`
   from this folder will report a 404 until a developer SDK is published for this commit.
2. Put the schematic in the mod's data folder:
   ```
   <game data>/horizon-mod-data/ghost_litematic/build.litematic
   ```
   or rename a `.schem` to `build.schem` in the same folder.
3. Start the game, press `G` (or whatever you rebound it to), then adjust Controls if desired.

Two small valid fixtures are included. Regenerate them with `python3 make_sample.py`:
```sh
cp mods/ghost_litematic/fixtures/sample.litematic <data>/horizon-mod-data/ghost_litematic/build.litematic
# or
cp mods/ghost_litematic/fixtures/sample.schem <data>/horizon-mod-data/ghost_litematic/build.schem
```

## Testing the baked-model renderer (developer build)

`draw_ghost_blocks` is a fork-only engine API. A published Nightly SDK does not have it, so the
mod must be built as a **native mod** against a prebuilt SDK of this fork:

```sh
# from the checkout, after committing the engine change
cargo build --locked -p horizon-game --bin horizon-client
cargo build --locked -p horizon-client-lib --bin horizon-ctl
cargo build --locked -p horizon-prepare
cargo build --locked -p horizon-server
cargo run --locked -p cargo-horizon-mod -- wasm-std target/debug/horizon-wasm-std.wasm
./target/debug/horizon-prepare --prepare --root . --server-jar <your 26.3 server.jar>
./target/debug/horizon-prepare compile-exporters --root .
# copy .local/exporters/<hash>/{manifest.json,client,server,client.log,server.log} into exporters/
horizon-launcher developer-install . --program target/debug/horizon-client --trust-unsigned
RUSTUP_TOOLCHAIN=$(cat tools/ci/rust-version) python3 tools/prebuilt_game.py build /tmp/dev-sdk
horizon-launcher prepare --version <developer-tag> --minecraft <your .minecraft>
HORIZON_LAUNCHER_DEVELOPER_SDK=/tmp/dev-sdk \
  horizon-launcher native-build <mod-folder> --instance <instance> --version <developer-tag> --trust-native-code
```
