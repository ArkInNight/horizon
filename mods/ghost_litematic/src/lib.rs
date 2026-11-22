//! Litematica-lite proof of concept for Horizon.
//!
//! Put a `build.litematic` or `build.schem` in this mod's data folder
//! (`horizon-mod-data/ghost_litematic/`), press `G` to toggle the ghost and `L` to reload.
//! The overlay is drawn with the
//! public client render API only; no mixins and no engine changes are used.
use mod_api::{
    ClientHost, ConfigPath, GuiLayer, GuiLayerDrawEvent, GuiLayerPlacement, Hooks, InputKeyEvent,
    KeyAction, RegisterGuiLayersEvent, RenderLevelStage, RenderLevelStageEvent, Tag, WorldDepth,
    WorldLine, WorldQuad,
};
use std::io::Read;
use std::sync::Mutex;

/// The public `WorldDrawContext` batch cap. Keep a wide margin: a large build needs culling.
const MAX_QUADS: usize = 4096;
/// Cull quads this far from the camera (blocks).
const DRAW_DISTANCE: f64 = 96.0;

#[derive(Default)]
struct Ghost {
    state: Mutex<State>,
}

#[derive(Clone, Copy, Default, PartialEq)]
enum Layers {
    #[default]
    All,
    Single,
    Range,
    Follow,
}

#[derive(Default)]
struct State {
    visible: bool,
    attempted: bool,
    schematic: Option<Schematic>,
    error: Option<String>,
    layers: Layers,
    /// Layer offset from the region's bottom, used by Single and Range.
    center: i32,
    /// Half-height of a Range view.
    half: i32,
}

struct Schematic {
    regions: Vec<Region>,
    blocks: usize,
}

struct PlacedQuad {
    y: i32,
    quad: WorldQuad,
}

struct Region {
    #[allow(dead_code)]
    name: String,
    origin: [i32; 3],
    size: [i32; 3],
    quads: Vec<PlacedQuad>,
    outline: Vec<WorldLine>,
}

#[mod_api::horizon_mod]
impl Hooks for Ghost {
    fn input_key(&self, _host: &mut dyn ClientHost, event: &mut InputKeyEvent<'_>) {
        if event.action != KeyAction::Press {
            return;
        }
        let mut state = self.state.lock().unwrap();
        match event.key {
            "key.keyboard.g" => state.visible ^= true,
            "key.keyboard.l" => {
                state.attempted = false;
                state.schematic = None;
                state.error = None;
            }
            "key.keyboard.o" => state.layers = Layers::All,
            "key.keyboard.p" => state.layers = Layers::Single,
            "key.keyboard.h" => state.layers = Layers::Follow,
            "key.keyboard.r" => state.layers = Layers::Range,
            "key.keyboard.left.bracket" => state.center -= 1,
            "key.keyboard.right.bracket" => state.center += 1,
            "key.keyboard.minus" if state.layers == Layers::Range => {
                state.half = (state.half - 1).max(0);
            }
            "key.keyboard.equal" if state.layers == Layers::Range => state.half += 1,
            _ => {}
        }
    }

    fn register_gui_layers(
        &self,
        _context: &mod_api::RegistrationContext,
        event: &mut RegisterGuiLayersEvent<'_>,
    ) {
        event.layers.push(GuiLayer {
            id: "ghost_litematic:status".into(),
            placement: GuiLayerPlacement::AboveAll,
        });
    }

    fn gui_layer_draw(&self, _host: &dyn ClientHost, event: &mut GuiLayerDrawEvent<'_>) {
        if event.layer != "ghost_litematic:status" {
            return;
        }
        let state = self.state.lock().unwrap();
        let (title, detail) = if let Some(error) = &state.error {
            (
                "Litematic: could not load build.litematic".to_string(),
                error.clone(),
            )
        } else if !state.visible {
            (
                "Litematic ghost: OFF".to_string(),
                "Press G to show, L to reload build.litematic".to_string(),
            )
        } else if let Some(schematic) = &state.schematic {
            (
                format!(
                    "Litematic ghost: {} blocks / {} regions",
                    schematic.blocks,
                    schematic.regions.len()
                ),
                "G: hide   L: reload".to_string(),
            )
        } else {
            (
                "Litematic ghost: loading…".to_string(),
                "Put build.litematic in horizon-mod-data/ghost_litematic/".to_string(),
            )
        };
        let layers = match state.layers {
            Layers::All => "all layers".to_string(),
            Layers::Single => format!("layer {}", state.center),
            Layers::Range => format!(
                "layers {}..={}",
                state.center - state.half,
                state.center + state.half
            ),
            Layers::Follow => "slice at player".to_string(),
        };
        event.draw.text(&title, [8.0, 8.0], 0xFFFFFFFF, true);
        event.draw.text(
            &format!("{layers}  [O all  P single  H follow  R range  [ ] layer  -/= thickness]"),
            [8.0, 22.0],
            0xFFFFC840,
            true,
        );
        event.draw.text(&detail, [8.0, 36.0], 0xFFB0B0B0, true);
    }

    fn render_level_stage(&self, host: &dyn ClientHost, event: &mut RenderLevelStageEvent<'_>) {
        if event.stage != RenderLevelStage::AfterTranslucentBlocks {
            return;
        }
        if !self.state.lock().unwrap().visible {
            return;
        }

        // Load once, lazily: data_files() needs a live world.
        {
            let mut state = self.state.lock().unwrap();
            if !state.attempted {
                state.attempted = true;
                match load(host) {
                    Ok(schematic) => state.schematic = Some(schematic),
                    Err(error) => state.error = Some(error),
                }
            }
        }

        let state = self.state.lock().unwrap();
        let Some(schematic) = state.schematic.as_ref() else {
            return;
        };

        let camera = event.view.camera;
        // The camera is the player's eye; feet are about 1.6 blocks lower.
        let player_y = (camera[1] - 1.6).floor() as i32;
        let mut quads = Vec::with_capacity(MAX_QUADS.min(1024));
        let mut outline = Vec::new();
        for region in &schematic.regions {
            let center = [
                region.origin[0] as f64 + region.size[0] as f64 / 2.0,
                region.origin[1] as f64 + region.size[1] as f64 / 2.0,
                region.origin[2] as f64 + region.size[2] as f64 / 2.0,
            ];
            if distance_squared(center, camera) > (DRAW_DISTANCE * 2.0).powi(2) {
                continue;
            }
            for placed in &region.quads {
                let shown = match state.layers {
                    Layers::All => true,
                    Layers::Single => placed.y == region.origin[1] + state.center,
                    Layers::Range => (region.origin[1] + state.center - state.half
                        ..=region.origin[1] + state.center + state.half)
                        .contains(&placed.y),
                    Layers::Follow => placed.y == player_y,
                };
                if !shown
                    || distance_squared(placed.quad.corners[0], camera) > DRAW_DISTANCE.powi(2)
                {
                    continue;
                }
                if quads.len() == MAX_QUADS {
                    break;
                }
                quads.push(WorldQuad::new(
                    placed.quad.corners,
                    placed.quad.color,
                    placed.quad.depth,
                ));
            }
            for line in &region.outline {
                if outline.len() < 512 {
                    outline.push(WorldLine::new(
                        line.from, line.to, line.color, line.width, line.depth,
                    ));
                }
            }
        }
        // Batches are atomic: either all of it draws or none does.
        let _ = event.draw.quads(&quads);
        let _ = event.draw.lines(&outline);
    }
}

fn distance_squared(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Read `build.litematic` (or `build.schem`) from this mod's data files and decode it.
fn load(host: &dyn ClientHost) -> Result<Schematic, String> {
    let (name, compressed) = ["build.litematic", "build.schem"]
        .into_iter()
        .find_map(|file| read_file(host, file).transpose().map(|bytes| (file, bytes)))
        .ok_or("put build.litematic or build.schem in horizon-mod-data/ghost_litematic/")?;
    let compressed = compressed?;
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(&compressed[..])
        .read_to_end(&mut raw)
        .map_err(|e| format!("gunzip: {e}"))?;
    let root = read_named_root(&raw)?;

    let mut result = Schematic {
        regions: Vec::new(),
        blocks: 0,
    };
    if let Some(Tag::Compound(regions)) = root.get("Regions") {
        for (region_name, region) in regions {
            match parse_region(region_name.to_string_lossy(), region) {
                Ok(region) => {
                    result.blocks += region.quads.len();
                    result.regions.push(region);
                }
                Err(error) => {
                    return Err(format!("region {}: {error}", region_name.to_string_lossy()));
                }
            }
        }
    } else {
        // Sponge schematics sometimes wrap the real tag in a `Schematic` compound.
        let schem = match root.get("Schematic") {
            Some(tag @ Tag::Compound(_)) => tag,
            _ => &root,
        };
        let region = parse_schem(schem)?;
        result.blocks += region.quads.len();
        result.regions.push(region);
    }
    if result.regions.is_empty() {
        return Err(format!("{name}: no regions or blocks"));
    }
    Ok(result)
}

fn read_file(host: &dyn ClientHost, name: &str) -> Result<Option<Vec<u8>>, String> {
    let path = ConfigPath::new(name).map_err(|e| e.to_string())?;
    if host
        .data_files()
        .stat(&path)
        .map_err(|e| e.to_string())?
        .is_none()
    {
        return Ok(None);
    }
    host.data_files()
        .read(&path)
        .map(Some)
        .map_err(|e| format!("{name}: {e}"))
}

/// A `.litematic` is a gzipped, *named-root* NBT file. The mod API exposes the network
/// (nameless-root) codec, so strip the root type byte and name length and decode the payload.
fn read_named_root(bytes: &[u8]) -> Result<Tag, String> {
    let (&kind, rest) = bytes.split_first().ok_or("empty NBT file")?;
    if kind != 10 {
        return Err(format!("root tag is {kind}, expected a compound (10)"));
    }
    if rest.len() < 2 {
        return Err("truncated root name".into());
    }
    let name_len = u16::from_be_bytes([rest[0], rest[1]]) as usize;
    let body = rest.get(2 + name_len..).ok_or("truncated root name")?;
    // The network codec still wants the root's type byte, just not its name.
    let mut payload = Vec::with_capacity(body.len() + 1);
    payload.push(kind);
    payload.extend_from_slice(body);
    // Litematics are far bigger than the network codec's 2 MiB default quota.
    let (tag, _) =
        Tag::from_network_bytes(&payload, Some(256 << 20)).map_err(|e| format!("NBT: {e}"))?;
    Ok(tag)
}

fn parse_region(name: String, tag: &Tag) -> Result<Region, String> {
    if !matches!(tag, Tag::Compound(_)) {
        return Err("region is not a compound".into());
    }
    let size = int_array(tag.get("Size").ok_or("no Size")?)?;
    let position = int_array(tag.get("Position").ok_or("no Position")?)?;
    let palette = match tag.get("BlockStatePalette") {
        Some(Tag::List(values)) => values,
        _ => return Err("no BlockStatePalette list".into()),
    };
    let states = match tag.get("BlockStates") {
        Some(Tag::LongArray(values)) => values,
        _ => return Err("no BlockStates long array".into()),
    };
    let [sx, sy, sz] = size;
    if sx <= 0 || sy <= 0 || sz <= 0 {
        return Err(format!("bad region size {sx}x{sy}x{sz}"));
    }
    let (xs, ys, zs) = (sx as usize, sy as usize, sz as usize);
    let count = xs
        .checked_mul(ys)
        .and_then(|n| n.checked_mul(zs))
        .ok_or("region too large")?;
    let bits = bits_for(palette.len());

    let mut quads = Vec::new();
    for index in 0..count {
        let entry = match palette.get(unpack(states, bits, index)) {
            Some(entry @ Tag::Compound(_)) => entry,
            _ => continue,
        };
        let Tag::String(block) = entry.get("Name").ok_or("palette entry has no Name")? else {
            continue;
        };
        let block = block.to_string_lossy();
        if block == "minecraft:air" || block == "air" {
            continue;
        }
        // Litematica packs cells x-fastest: i = x + z*sx + y*sx*sz.
        let (x, z, y) = (index % xs, (index / xs) % zs, index / (xs * zs));
        let wx = position[0] as f64 + x as f64;
        let wy = position[1] as f64 + y as f64;
        let wz = position[2] as f64 + z as f64;
        let inset = 0.49;
        let top = wy + 1.01;
        let color = color_for(&block);
        quads.push(PlacedQuad {
            y: wy as i32,
            quad: WorldQuad::new(
                [
                    [wx - inset, top, wz - inset],
                    [wx - inset, top, wz + inset],
                    [wx + inset, top, wz + inset],
                    [wx + inset, top, wz - inset],
                ],
                color,
                WorldDepth::Tested,
            ),
        });
    }

    let (x0, y0, z0) = (position[0] as f64, position[1] as f64, position[2] as f64);
    let (x1, y1, z1) = (x0 + sx as f64, y0 + sy as f64, z0 + sz as f64);
    let mut outline = Vec::with_capacity(12);
    for (from, to) in [
        ([x0, y0, z0], [x1, y0, z0]),
        ([x1, y0, z0], [x1, y0, z1]),
        ([x1, y0, z1], [x0, y0, z1]),
        ([x0, y0, z1], [x0, y0, z0]),
        ([x0, y1, z0], [x1, y1, z0]),
        ([x1, y1, z0], [x1, y1, z1]),
        ([x1, y1, z1], [x0, y1, z1]),
        ([x0, y1, z1], [x0, y1, z0]),
        ([x0, y0, z0], [x0, y1, z0]),
        ([x1, y0, z0], [x1, y1, z0]),
        ([x1, y0, z1], [x1, y1, z1]),
        ([x0, y0, z1], [x0, y1, z1]),
    ] {
        outline.push(WorldLine::new(
            from,
            to,
            0x80_FF_FF_40,
            2.0,
            WorldDepth::ThroughWalls,
        ));
    }
    Ok(Region {
        name,
        origin: position,
        size,
        quads,
        outline,
    })
}

/// Read a Sponge `.schem` (versions 1–3): a palette of state strings and x-fastest-packed
/// varint indices. The layout follows the public Schematic specification, as does the repo's
/// own reader in `tools/lra_schem.py`.
fn parse_schem(tag: &Tag) -> Result<Region, String> {
    let version = match tag.get("Version") {
        Some(Tag::Int(value)) => *value,
        _ => 1,
    };
    if !(1..=3).contains(&version) {
        return Err(format!("unsupported .schem version {version}"));
    }
    let width = short(tag.get("Width").ok_or("no Width")?)? as usize;
    let height = short(tag.get("Height").ok_or("no Height")?)? as usize;
    let length = short(tag.get("Length").ok_or("no Length")?)? as usize;
    if width == 0 || height == 0 || length == 0 {
        return Err("empty schematic".into());
    }
    let origin = match tag.get("Origin") {
        Some(origin) => int_array(origin)?,
        None => [0, 0, 0],
    };
    let (blocks, data) = if version >= 3 {
        let blocks = tag.get("Blocks").ok_or("no Blocks compound")?;
        (
            blocks,
            match blocks.get("Data") {
                Some(Tag::ByteArray(bytes)) => bytes.as_slice(),
                _ => return Err("no Blocks.Data byte array".into()),
            },
        )
    } else {
        (
            tag,
            match tag.get("BlockData") {
                Some(Tag::ByteArray(bytes)) => bytes.as_slice(),
                _ => return Err("no BlockData byte array".into()),
            },
        )
    };
    let Tag::Compound(entries) = blocks.get("Palette").ok_or("no Palette")? else {
        return Err("Palette is not a compound".into());
    };
    let mut palette: Vec<Option<String>> = Vec::with_capacity(entries.len());
    for (state, index) in entries {
        let Tag::Int(index) = index else {
            return Err("a palette index is not an int".into());
        };
        if *index < 0 {
            return Err("negative palette index".into());
        }
        let slot = *index as usize;
        if palette.len() <= slot {
            palette.resize_with(slot + 1, || None);
        }
        palette[slot] = Some(state.to_string_lossy());
    }
    let count = width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(length))
        .ok_or("schematic too large")?;
    let indices = varints(data, count)?;

    let mut quads = Vec::new();
    for (cell, index) in indices.into_iter().enumerate() {
        let Some(state) = palette.get(index).and_then(|slot| slot.as_deref()) else {
            continue;
        };
        if matches!(
            state,
            "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air" | "air"
        ) {
            continue;
        }
        let (x, z, y) = (
            cell % width,
            (cell / width) % length,
            cell / (width * length),
        );
        let wx = origin[0] as f64 + x as f64;
        let wy = origin[1] as f64 + y as f64;
        let wz = origin[2] as f64 + z as f64;
        let inset = 0.49;
        let top = wy + 1.01;
        quads.push(PlacedQuad {
            y: wy as i32,
            quad: WorldQuad::new(
                [
                    [wx - inset, top, wz - inset],
                    [wx - inset, top, wz + inset],
                    [wx + inset, top, wz + inset],
                    [wx + inset, top, wz - inset],
                ],
                color_for(state),
                WorldDepth::Tested,
            ),
        });
    }
    let (x0, y0, z0) = (origin[0] as f64, origin[1] as f64, origin[2] as f64);
    let (x1, y1, z1) = (x0 + width as f64, y0 + height as f64, z0 + length as f64);
    let mut outline = Vec::with_capacity(12);
    for (from, to) in [
        ([x0, y0, z0], [x1, y0, z0]),
        ([x1, y0, z0], [x1, y0, z1]),
        ([x1, y0, z1], [x0, y0, z1]),
        ([x0, y0, z1], [x0, y0, z0]),
        ([x0, y1, z0], [x1, y1, z0]),
        ([x1, y1, z0], [x1, y1, z1]),
        ([x1, y1, z1], [x0, y1, z1]),
        ([x0, y1, z1], [x0, y1, z0]),
        ([x0, y0, z0], [x0, y1, z0]),
        ([x1, y0, z0], [x1, y1, z0]),
        ([x1, y0, z1], [x1, y1, z1]),
        ([x0, y0, z1], [x0, y1, z1]),
    ] {
        outline.push(WorldLine::new(
            from,
            to,
            0x80_FF_FF_40,
            2.0,
            WorldDepth::ThroughWalls,
        ));
    }
    Ok(Region {
        name: "schematic".into(),
        origin,
        size: [width as i32, height as i32, length as i32],
        quads,
        outline,
    })
}

fn short(tag: &Tag) -> Result<i16, String> {
    match tag {
        Tag::Short(value) => Ok(*value),
        Tag::Int(value) => Ok(*value as i16),
        _ => Err("expected a short".into()),
    }
}

/// Unsigned LEB128 indices, at most five bytes each.
fn varints(bytes: &[u8], count: usize) -> Result<Vec<usize>, String> {
    let mut out = Vec::with_capacity(count);
    let (mut value, mut shift, mut used) = (0usize, 0u32, 0u8);
    for &byte in bytes {
        used += 1;
        if used > 5 || (used == 5 && byte & 0x80 != 0) {
            return Err("overlong varint".into());
        }
        value |= ((byte & 0x7F) as usize) << shift;
        if byte & 0x80 != 0 {
            shift += 7;
            continue;
        }
        out.push(value);
        if out.len() == count {
            return Ok(out);
        }
        if out.len() > count {
            return Err("too many block indices".into());
        }
        (value, shift, used) = (0, 0, 0);
    }
    Err(format!("{} block indices for {count} cells", out.len()))
}

fn int_array(tag: &Tag) -> Result<[i32; 3], String> {
    let Tag::IntArray(values) = tag else {
        return Err("expected an int array".into());
    };
    if values.len() != 3 {
        return Err(format!("expected 3 ints, got {}", values.len()));
    }
    Ok([values[0], values[1], values[2]])
}

/// Litematica uses at least 2 bits per palette entry.
fn bits_for(entries: usize) -> u32 {
    let mut bits = 0;
    let mut value = entries.saturating_sub(1);
    while value > 0 {
        bits += 1;
        value >>= 1;
    }
    bits.max(2)
}

/// Palette indices are packed little-endian bit order into a long array.
fn unpack(states: &[i64], bits: u32, index: usize) -> usize {
    let per_word = 64 / bits as usize;
    let word = index / per_word;
    let shift = (index % per_word) * bits as usize;
    let Some(value) = states.get(word) else {
        return 0;
    };
    (((*value as u64) >> shift) as usize) & ((1usize << bits) - 1)
}

/// A stable translucent colour per block so different materials are distinguishable.
fn color_for(block: &str) -> u32 {
    let mut hash = 0u32;
    for byte in block.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(byte as u32);
    }
    0x50_00_00_00 | (hash & 0x00FF_FFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_sample_litematic() {
        let mut raw = Vec::new();
        flate2::read::GzDecoder::new(&include_bytes!("../fixtures/sample.litematic")[..])
            .read_to_end(&mut raw)
            .unwrap();
        let root = read_named_root(&raw).unwrap();
        let Tag::Compound(regions) = root.get("Regions").unwrap() else {
            panic!("Regions is not a compound");
        };
        let (name, region) = regions.iter().next().unwrap();
        let region = parse_region(name.to_string_lossy(), region).unwrap();
        assert_eq!(region.name, "sample");
        assert_eq!(region.origin, [0, 64, 0]);
        assert_eq!(region.size, [3, 2, 3]);
        assert_eq!(region.quads.len(), 9, "nine non-air cells");
        assert_eq!(region.outline.len(), 12, "a box has twelve edges");
    }

    #[test]
    fn reads_the_sample_schem() {
        let mut raw = Vec::new();
        flate2::read::GzDecoder::new(&include_bytes!("../fixtures/sample.schem")[..])
            .read_to_end(&mut raw)
            .unwrap();
        let root = read_named_root(&raw).unwrap();
        let schem = root.get("Schematic").unwrap_or(&root);
        let region = parse_schem(schem).unwrap();
        assert_eq!(region.origin, [0, 64, 0]);
        assert_eq!(region.size, [3, 2, 3]);
        assert_eq!(region.quads.len(), 9, "nine non-air cells");
    }
}
