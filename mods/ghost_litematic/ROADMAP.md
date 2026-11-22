# Roadmap — ghost_litematic

Status rows: **Done**, **Next**, **Later**, **Blocked**. Effort is a rough one-person estimate.

## Done

| Version | Feature | Notes |
|---|---|---|
| 0.1.0 | `.litematic` import | Gzipped named-root NBT, `BlockStatePalette` + packed `BlockStates`, one region or many. |
| 0.1.0 | Ghost overlay | Translucent quad per non-air block + wireframe region box, camera culling, 4,096-quad cap. |
| 0.1.0 | Toggle / reload / HUD | `G` toggle, `L` reload, status + error readout. |
| 0.2.0 | Layer / slice control | `O` all, `P` single, `H` follow player, `R` range, `[`/`]` layer, `-`/`=` thickness. |
| 0.2.0 | `.schem` import | Sponge v1–3: palette + LEB128 varint indices, optional `Schematic` wrapper. |
| 0.2.0 | Fixtures + build script | Valid `.litematic` and `.schem` samples; `build-wasm.sh` builds and installs the `.hmod`. |
| 0.3.0 | Easy-place MVP | `E` enable, `B` place next reachable cell: hotbar match, aim at support, `key.use`. |
| 0.4.0 | Auto-place loop | `T` toggles a 250 ms bottom-up loop; stops when nothing is reachable. |
| 0.4.0 | Robust hotbar matching | Scan literal slots 0..=8 and verify the held item; no dependence on compacted inventory indices. |
| 0.5.0 | Config screen | `C` opens a UI: pick a schematic file, layer modes, easy/auto toggles. |
| 0.6.0 | Verifier overlay | `V` tints correct / wrong / missing cells and shows a tally. |
| 0.7.0 | Extra-block detection | Reverse diff marks world blocks not in the schematic in magenta. |
| 0.8.0 | Alt-only bindings | No more collisions with vanilla keys. |
| 0.8.0 | Ghost cube rendering | Three shaded faces per block instead of one flat tile. |
| 0.8.0 | Move / anchor schematic | `Alt+arrows` and screen buttons; first load anchored near the player. |

## Next

| Priority | Feature | Effort | Why |
|---|---|---|---|
| P1 | Block-entity ghost labels | M | Show chest/sign positions and IDs from `TileEntities` / `BlockEntities`. |
| P1 | Verifier overlay | M | Compare `level.block_state` with the schematic and tint matching / mismatched cells. |
| P1 | Block-entity ghosts | M | Show chest/sign/beacon positions and labels from `TileEntities` / `BlockEntities`. |

## Later

| Feature | Effort | Notes |
|---|---|---|
| Accurate ghost models | M–L | `register_render_pipelines` + WGSL + atlas sprite instead of flat quads. |
| Presets on disk | S | Save layer mode / center / half to `data_files()` and reload. |
| Multi-region explorer | M | Region list, per-region enable, jump-to-region. |
| Placement of unsupported cells | M | Raw `use_item_on` packet fallback when no real support block exists. |

## Blocked (needs engine or a companion server mod)

| Feature | Blocker |
|---|---|
| Bulk paste / world edits | Client mods cannot write blocks; `edit_blocks` is `WorldOnly` (server). Need a paired server mod + custom payload. |
| Block entity NBT placement | `edit_blocks` explicitly does not support NBT; needs `upgrade_block_entity_nbt` on the server side. |
| Exact vanilla translucent block models | No mod API to draw a `BlockState`'s baked model translucently; model instances refuse translucent faces. |
| Safe raw placement packets | The game's prediction sequence counter is private; the server enforces reach and cursor bounds. |

## Non-goals

- Loading original Fabric jars / Mixins — Horizon has no JVM bridge; everything here is a rewrite against the public mod API.
- Replacing structure blocks / vanilla data packs.
