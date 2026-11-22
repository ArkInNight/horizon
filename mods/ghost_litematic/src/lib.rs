//! Litematica-lite proof of concept for Horizon.
//!
//! Put a `build.litematic` in this mod's data folder (`horizon-mod-data/ghost_litematic/`),
//! press `G` to toggle the ghost and `L` to reload the file. The overlay is drawn with the
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
const FILE: &str = "build.litematic";

#[derive(Default)]
struct Ghost {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    visible: bool,
    attempted: bool,
    schematic: Option<Schematic>,
    error: Option<String>,
}

struct Schematic {
    regions: Vec<Region>,
    blocks: usize,
}

struct Region {
    #[allow(dead_code)]
    name: String,
    origin: [i32; 3],
    size: [i32; 3],
    quads: Vec<WorldQuad>,
    outline: Vec<WorldLine>,
}

#[mod_api::horizon_mod]
impl Hooks for Ghost {
    fn input_key(&self, _host: &mut dyn ClientHost, event: &mut InputKeyEvent<'_>) {
        if event.action != KeyAction::Press {
            return;
        }
        match event.key {
            "key.keyboard.g" => self.state.lock().unwrap().visible ^= true,
            "key.keyboard.l" => {
                let mut state = self.state.lock().unwrap();
                state.attempted = false;
                state.schematic = None;
                state.error = None;
            }
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
                format!("Litematic ghost: {} blocks / {} regions", schematic.blocks, schematic.regions.len()),
                "G: hide   L: reload".to_string(),
            )
        } else {
            (
                "Litematic ghost: loading…".to_string(),
                "Put build.litematic in horizon-mod-data/ghost_litematic/".to_string(),
            )
        };
        event.draw.text(&title, [8.0, 8.0], 0xFFFFFFFF, true);
        event.draw.text(&detail, [8.0, 22.0], 0xFFFFC840, true);
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
            for quad in &region.quads {
                if distance_squared(quad.corners[0], camera) > DRAW_DISTANCE.powi(2) {
                    continue;
                }
                if quads.len() == MAX_QUADS {
                    break;
                }
                quads.push(WorldQuad::new(
                    quad.corners,
                    quad.color,
                    quad.depth,
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

/// Read `build.litematic` from this mod's bulk data files and decode a region list.
fn load(host: &dyn ClientHost) -> Result<Schematic, String> {
    let path = ConfigPath::new(FILE).map_err(|e| e.to_string())?;
    let compressed = host
        .data_files()
        .read(&path)
        .map_err(|e| format!("{FILE}: {e}"))?;
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(&compressed[..])
        .read_to_end(&mut raw)
        .map_err(|e| format!("gunzip: {e}"))?;
    let root = read_named_root(&raw)?;

    let Tag::Compound(regions) = root
        .get("Regions")
        .ok_or("the file has no Regions compound")?
    else {
        return Err("Regions is not a compound".into());
    };
    let mut result = Schematic {
        regions: Vec::new(),
        blocks: 0,
    };
    for (name, region) in regions {
        match parse_region(name.to_string_lossy(), region) {
            Ok(region) => {
                result.blocks += region.quads.len();
                result.regions.push(region);
            }
            Err(error) => return Err(format!("region {}: {error}", name.to_string_lossy())),
        }
    }
    if result.regions.is_empty() {
        return Err("the file has no regions".into());
    }
    Ok(result)
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
    let (tag, _) = Tag::from_network_bytes(&payload, Some(256 << 20))
        .map_err(|e| format!("NBT: {e}"))?;
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
        quads.push(WorldQuad::new(
            [
                [wx - inset, top, wz - inset],
                [wx - inset, top, wz + inset],
                [wx + inset, top, wz + inset],
                [wx + inset, top, wz - inset],
            ],
            color,
            WorldDepth::Tested,
        ));
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
}
