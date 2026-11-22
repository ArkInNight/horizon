# ghost_litematic — litematica-lite PoC

A native Horizon mod that reads a `.litematic` **or** a Sponge `.schem` and draws its blocks as a
translucent ghost overlay. This is a **proof of concept**: no engine changes, no mixins, only the
public mod API (`render_level_stage`, `input_key`, `register_gui_layers`, `gui_layer_draw`, `data_files`).

## What works

- Loads `build.litematic` (gzipped, named-root NBT, `BlockStatePalette` + packed `BlockStates`)
  or `build.schem` (Sponge v1–3, palette + LEB128 varint indices) from the mod's data folder.
- One translucent quad per non-air block, plus a wireframe box per region/schematic.
- Camera-distance culling and the API's 4,096-quad batch cap.
- Layer / slice controls:
  - `O` — all layers
  - `P` — single layer
  - `H` — slice that follows the player's feet
  - `R` — a range of layers around the current one
  - `[` / `]` — move the current layer down / up
  - `-` / `=` — shrink / grow the range thickness
- `G` toggles the overlay, `L` reloads the file, `C` opens the config screen. The HUD shows the active mode and layer.

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
3. Start the game, press `G`, then use the layer keys above.

Two small valid fixtures are included. Regenerate them with `python3 make_sample.py`:
```sh
cp mods/ghost_litematic/fixtures/sample.litematic <data>/horizon-mod-data/ghost_litematic/build.litematic
# or
cp mods/ghost_litematic/fixtures/sample.schem <data>/horizon-mod-data/ghost_litematic/build.schem
```
