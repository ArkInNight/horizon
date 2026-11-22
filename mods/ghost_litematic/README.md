# ghost_litematic — litematica-lite PoC

A native Horizon mod that reads a `.litematic` file and draws its blocks as a translucent
ghost overlay. This is a **proof of concept**: no engine changes, no mixins, only the public
mod API (`render_level_stage`, `input_key`, `register_gui_layers`, `gui_layer_draw`, `data_files`).

## What works

- Loads a real gzipped, named-root NBT `.litematic` from the mod's data folder.
- Reads `Size`, `Position`, `BlockStatePalette` and packed `BlockStates` for every region.
- Draws one translucent top-face quad per non-air block, plus a wireframe box per region.
- Culls by camera distance and caps batches at the API's 4,096-quad limit.
- `G` toggles the overlay, `L` reloads the file, HUD shows status and errors.

## Not in this PoC

- Exact vanilla ghost block models (drawn as flat unlit quads).
- Block entities, entities, layer/slice controls.
- Easy-place mode, schematic verifier, bulk paste into the world (client mods cannot write
  blocks — that needs a companion server mod).

## Install / test

1. Build the mod against this checkout (it depends on the API crate by path):
   ```sh
   cargo test --manifest-path mods/ghost_litematic/Cargo.toml
   ```
   A real installable `.hmod` needs a signed SDK for the pinned build; `cargo horizon-mod check`
   from this folder will report a 404 until a developer SDK is published for this commit.
2. Copy the built native mod into the instance and place the schematic in the mod's data folder:
   ```
   <game data>/horizon-mod-data/ghost_litematic/build.litematic
   ```
3. Start the game, press `G`.

A tiny valid fixture is included: `fixtures/sample.litematic` (3×2×3, 9 blocks). Regenerate it
with `python3 make_sample.py`. To use it as the live file:
```sh
cp fixtures/sample.litematic <game data>/horizon-mod-data/ghost_litematic/build.litematic
```
