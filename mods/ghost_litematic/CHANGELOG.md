# Changelog

All notable changes to this mod are documented here. Dates use the commit date in UTC.

## [Unreleased]

## [0.11.0] — 2026-11-23

### Added (engine + mod)
- New mod API `WorldDrawContext::draw_ghost_blocks`: bakes `(BlockState, position)` lists into
  translucent atlas-textured world quads using the engine's existing block-model catalog, tint
  table and alpha-blended world-primitives pipeline. No WGSL changes needed.
- The mod now renders real baked block models instead of hand-built three-face cubes, including
  for verifier colours and extra blocks.
- Config screen has **Files / Materials** tabs; material list compares schematic requirements with
  the player's inventory and shows a HUD summary.
- Block-entity labels and wrong-state verifier from 0.10.0 remain.

> The engine part must be present in the running game build; a published Nightly SDK does not have
> this new API yet, so testing it requires a local developer build of this fork.

## [0.10.0] — 2026-11-23

### Added
- Block-entity ghost labels: `TileEntities` / `BlockEntities` positions and ids are parsed from the
  schematic and projected onto the HUD (pink labels above their blocks).
- Pick block: `F` (rebindable) selects the hotbar slot holding the item for the nearest ghost block.
- Verifier distinguishes wrong block from wrong *state* (same block, different properties): the
  latter is yellow and counted separately in the HUD.

## [0.9.1] — 2026-11-22

### Fixed
- E2E found that ghost quads and region boxes were still drawn at the file's original coordinates
  even after moving/anchoring: the offset is now applied to quad corners, culling, verifier lookups
  and box outlines. The ghost actually appears where the player placed it.

## [0.9.0] — 2026-11-22

### Added
- All 21 controls are now **registered key mappings**: they show under Options > Controls and
  can be rebound (and reset) by the player. Defaults are plain keys; the old Alt hard-coding is gone.
- Actions are polled from `ClientHost::consume_click` in `client_tick_pre`.

## [0.8.0] — 2026-11-22

### Changed
- **All mod bindings now require Alt**, so they no longer collide with vanilla controls.
  - `Alt+G/L/C/V/E/B/T` and `Alt+O/P/H/R`, `Alt+[`/`]`, `Alt+-`/`=`.
- The ghost is drawn as three shaded faces per block instead of a single flat top tile.
- The first schematic load is **anchored near the player** instead of staying at the file's saved
  Y coordinate.

### Added
- Schematic translation: `Alt+arrow keys` move X/Z, `Alt+PageUp/PageDown` move Y;
  hold Shift to move 16 blocks at a time. The config screen has `X-/X+/Y-/Y+/Z-/Z+` buttons
  and shows the current offset.
- Verifier's extra-block colour is magenta and the tally includes `extra`.

## [0.7.0] — 2026-11-22

### Added
- Extra-block detection in verifier mode: real world blocks inside the schematic volume that the
  schematic does not call for are drawn magenta and counted as `extra` in the HUD.
- Each region caches the set of absolute target positions, so the reverse diff is a set lookup.
- The extra scan follows the active layer (all / single / range / follow) to stay bounded.

## [0.6.0] — 2026-11-22

### Added
- Verifier overlay: `V` (or the `Verify` button in the config screen) tints ghost cells by how
  they compare with the live world — green = correct, red = wrong block, orange = missing.
  The HUD shows a correct / wrong / missing tally for the currently visible cells.

## [0.5.0] — 2026-11-22

### Added
- Config screen: press `C` in a world to open a malilib-style UI.
  - Lists `.litematic` / `.schem` files from the mod's data folder; click a row to load it.
  - Buttons for all layer modes, layer up/down, range thickness, easy-place and auto-place.
  - Scroll wheel pages the file list; `Close` (or Escape) exits.
- `load()` now honours the file selected in the screen instead of only the two default names.

## [0.4.0] — 2026-11-22

### Added
- Auto-place loop: `T` starts/stops placing one ghost block every 250 ms, bottom-up, and stops
  automatically when nothing reachable is left. Uses `client_tick_post` and the game clock
  (`ClientHost::millis`) for the cooldown.
- Robust hotbar matching: scan literal slots 0..=8 with `select_hotbar_slot`, verify with
  `PlayerRef::selected_slot` and `LivingRef::hand_item` instead of trusting the client's
  compacted inventory list.
- Cells already matching the target state are skipped, so the loop never repeats a block.

## [0.3.0] — 2026-11-22

### Added
- Easy-place MVP: `E` enables it, `B` places the nearest reachable ghost block.
  - Target cell now stores its full `[x, y, z]` position and block-state string.
  - Finds a hotbar slot whose item maps to the target block, selects it, aims at the top of the
    supporting block and fires `key.use`, so the game builds the placement packet with its own
    sequence number and vanilla reach/cooldown checks.
- HUD line showing easy-place state.

### Fixed
- `build-wasm.sh` now enters the scaffolded project directory before copying sources.

## [0.2.0] — 2026-11-22

### Added
- Layer / slice controls: `O` all layers, `P` single layer, `H` slice at player height, `R` range,
  `[` / `]` move the layer, `-` / `=` change range thickness.
- Sponge `.schem` import (versions 1–3), including the optional `Schematic` wrapper.
- `build-wasm.sh`: reproducible wasm build against an installed Nightly SDK, with automatic install.
- Two generated fixtures: `fixtures/sample.litematic` and `fixtures/sample.schem`.

## [0.1.0] — 2026-11-22

### Added
- First proof of concept.
- Read `build.litematic` from `horizon-mod-data/ghost_litematic/` (gzip, named-root NBT,
  `BlockStatePalette` + bit-packed `BlockStates`).
- Draw a translucent top-face quad for each non-air block and a wireframe box per region,
  with camera-distance culling and the API's 4,096-quad batch cap.
- `G` toggles the overlay, `L` reloads the file, HUD shows status and parse errors.
- `cargo test` fixture verifying the generated `.litematic` parses to 9 blocks and 12 box edges.
