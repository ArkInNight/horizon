# Changelog

All notable changes to this mod are documented here. Dates use the commit date in UTC.

## [Unreleased]

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
