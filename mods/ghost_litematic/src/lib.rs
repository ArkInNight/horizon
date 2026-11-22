//! Litematica-lite proof of concept for Horizon.
//!
//! Put a `build.litematic` or `build.schem` in this mod's data folder
//! (`horizon-mod-data/ghost_litematic/`), press `G` to toggle the ghost and `L` to reload.
//! The overlay is drawn with the
//! public client render API only; no mixins and no engine changes are used.
use mod_api::client::widgets::{Button, Outcome, WidgetId, Widgets};
use mod_api::{
    CONFIG_LIST_MAX, ClientHost, ClientTickPostEvent, ClientTickPreEvent, ConfigEntryKind,
    ConfigPath, Flow, GuiLayer, GuiLayerDrawEvent, GuiLayerPlacement, Hand, Hooks, InputKeyEvent,
    KeyAction, LivingRef, RegisterGuiLayersEvent, RenderLevelStage, RenderLevelStageEvent,
    ScreenCharTypedPreEvent, ScreenKeyPressedPreEvent, ScreenMouseButtonPressedPreEvent,
    ScreenMouseButtonReleasedPreEvent, ScreenMouseDraggedPreEvent, ScreenMouseScrolledPreEvent,
    ScreenRenderPostEvent, Tag, Text, WorldDepth, WorldLine, WorldQuad,
};
use std::io::Read;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// The public `WorldDrawContext` batch cap. Keep a wide margin: a large build needs culling.
const MAX_QUADS: usize = 4096;
/// Cull quads this far from the camera (blocks).
const DRAW_DISTANCE: f64 = 96.0;

#[derive(Default)]
struct Ghost {
    state: Mutex<State>,
    config: Mutex<Option<ConfigUi>>,
    close_config: AtomicBool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
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
    /// Whether B tries to place the next ghost block through normal gameplay.
    easy_place: bool,
    /// Whether T keeps placing bottom-up, one block every 250 ms.
    auto: bool,
    /// Game-clock millis of the last placement attempt.
    last_place: Option<u64>,
    /// A short status line about the last auto-place attempt.
    auto_note: Option<String>,
    /// The schematic chosen in the config screen; None uses the default file names.
    file: Option<String>,
    /// Whether to tint the ghost by match against the live world.
    verify: bool,
    /// Last rendered verification tally: correct / wrong block / missing.
    verify_tally: [usize; 3],
}

const SCREEN_ID: &str = "ghost_litematic";
const ROW_H: f32 = 14.0;
const VISIBLE_ROWS: usize = 10;
const LIST_X: f32 = 20.0;
const LIST_Y: f32 = 92.0;
const LIST_W: f32 = 300.0;

struct ConfigUi {
    widgets: Widgets,
    close: WidgetId,
    all: WidgetId,
    single: WidgetId,
    follow: WidgetId,
    range: WidgetId,
    layer_down: WidgetId,
    layer_up: WidgetId,
    thin: WidgetId,
    thick: WidgetId,
    easy: WidgetId,
    auto: WidgetId,
    verify: WidgetId,
    rows: Vec<String>,
    offset: usize,
}

impl ConfigUi {
    fn new(host: &dyn ClientHost) -> Self {
        let mut widgets = Widgets::new();
        let button = |widgets: &mut Widgets, x: f32, y: f32, label: &'static str| {
            widgets.add_button(Button::new([x, y, 68.0, 20.0], Text::literal(label)))
        };
        let all = button(&mut widgets, 20.0, 20.0, "All");
        let single = button(&mut widgets, 92.0, 20.0, "Single");
        let follow = button(&mut widgets, 164.0, 20.0, "Follow");
        let range = button(&mut widgets, 236.0, 20.0, "Range");
        let layer_down = button(&mut widgets, 20.0, 46.0, "Layer -");
        let layer_up = button(&mut widgets, 92.0, 46.0, "Layer +");
        let thin = button(&mut widgets, 164.0, 46.0, "Thin -");
        let thick = button(&mut widgets, 236.0, 46.0, "Thin +");
        let easy = button(&mut widgets, 20.0, 68.0, "Easy");
        let auto = button(&mut widgets, 92.0, 68.0, "Auto");
        let verify = button(&mut widgets, 164.0, 68.0, "Verify");
        let close = button(&mut widgets, 236.0, 68.0, "Close");

        Self {
            widgets,
            close,
            all,
            single,
            follow,
            range,
            layer_down,
            layer_up,
            thin,
            thick,
            easy,
            auto,
            verify,
            rows: list_schematics(host),
            offset: 0,
        }
    }

    fn row_at(&self, [x, y]: [f32; 2]) -> Option<usize> {
        if x < LIST_X || x >= LIST_X + LIST_W {
            return None;
        }
        let index = ((y - LIST_Y) / ROW_H).floor();
        if index < 0.0 || index as usize >= VISIBLE_ROWS {
            return None;
        }
        let index = self.offset + index as usize;
        (index < self.rows.len()).then_some(index)
    }
}

fn list_schematics(host: &dyn ClientHost) -> Vec<String> {
    let files = host.data_files();
    let root = ConfigPath::root();
    let mut out = Vec::new();
    let mut after: Option<String> = None;
    loop {
        let page = match files.list(&root, after.as_deref(), CONFIG_LIST_MAX as u32) {
            Ok(page) => page,
            Err(_) => break,
        };
        if page.is_empty() {
            break;
        }
        for entry in page {
            if entry.stat.kind == ConfigEntryKind::File
                && (entry.name.ends_with(".litematic") || entry.name.ends_with(".schem"))
            {
                out.push(entry.name.clone());
            }
            after = Some(entry.name);
        }
    }
    out
}

#[derive(Clone)]
struct Schematic {
    regions: Vec<Region>,
    blocks: usize,
}

#[derive(Clone)]
struct PlacedQuad {
    /// The absolute target cell this ghost stands for.
    pos: [i32; 3],
    /// The block state a click should place here (NBT `Name` + `Properties`).
    state: String,
    quad: WorldQuad,
}

#[derive(Clone)]
struct Region {
    #[allow(dead_code)]
    name: String,
    origin: [i32; 3],
    size: [i32; 3],
    quads: Vec<PlacedQuad>,
    outline: Vec<WorldLine>,
}

impl Ghost {
    /// Route widget input and apply the button that was pressed.
    fn config_input(&self, host: &dyn ClientHost, f: impl FnOnce(&mut Widgets) -> Outcome) -> Flow {
        if host.mod_screen() != Some(SCREEN_ID) {
            return Flow::Continue;
        }
        let mut guard = self.config.lock().unwrap();
        let Some(ui) = guard.as_mut() else {
            return Flow::Continue;
        };
        let outcome = f(&mut ui.widgets);
        if let Outcome::Pressed(id) = outcome {
            if id == ui.close {
                self.close_config.store(true, Ordering::Relaxed);
            } else {
                let mut state = self.state.lock().unwrap();
                if id == ui.all {
                    state.layers = Layers::All;
                } else if id == ui.single {
                    state.layers = Layers::Single;
                } else if id == ui.follow {
                    state.layers = Layers::Follow;
                } else if id == ui.range {
                    state.layers = Layers::Range;
                } else if id == ui.layer_down {
                    state.center -= 1;
                } else if id == ui.layer_up {
                    state.center += 1;
                } else if id == ui.thin {
                    state.half = (state.half - 1).max(0);
                } else if id == ui.thick {
                    state.half += 1;
                } else if id == ui.easy {
                    state.easy_place ^= true;
                } else if id == ui.verify {
                    state.verify ^= true;
                } else if id == ui.auto {
                    state.auto ^= true;
                    if state.auto {
                        state.easy_place = true;
                        state.last_place = None;
                    }
                }
            }
        }
        if outcome.handled() {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    /// Click a file row: load that schematic on the next render.
    fn select_row(&self, host: &dyn ClientHost, at: [f32; 2]) -> Flow {
        if host.mod_screen() != Some(SCREEN_ID) {
            return Flow::Continue;
        }
        let guard = self.config.lock().unwrap();
        let Some(ui) = guard.as_ref() else {
            return Flow::Continue;
        };
        let Some(index) = ui.row_at(at) else {
            return Flow::Continue;
        };
        let file = ui.rows[index].clone();
        drop(guard);
        let mut state = self.state.lock().unwrap();
        state.file = Some(file);
        state.attempted = false;
        state.schematic = None;
        state.error = None;
        state.auto = false;
        Flow::Cancel
    }
}

#[mod_api::horizon_mod]
impl Hooks for Ghost {
    fn input_key(&self, host: &mut dyn ClientHost, event: &mut InputKeyEvent<'_>) {
        if event.action != KeyAction::Press {
            return;
        }
        let mut state = self.state.lock().unwrap();
        if event.key == "key.keyboard.b" && state.easy_place {
            let schematic = state.schematic.clone();
            if let Some(schematic) = schematic {
                drop(state);
                place_next(host, &schematic);
            }
            return;
        }
        match event.key {
            "key.keyboard.c" if host.playing() => {
                *self.config.lock().unwrap() = None;
                let _ = host.mod_screen_open(SCREEN_ID);
            }
            "key.keyboard.g" => state.visible ^= true,
            "key.keyboard.e" => state.easy_place ^= true,
            "key.keyboard.v" => state.verify ^= true,
            "key.keyboard.t" => {
                state.auto ^= true;
                if state.auto {
                    state.easy_place = true;
                    state.last_place = None;
                    state.auto_note = None;
                } else {
                    state.auto_note = Some("auto-place stopped".into());
                }
            }
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

    fn client_tick_post(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPostEvent) {
        let now = host.millis();
        let mut state = self.state.lock().unwrap();
        if !state.auto || !state.visible {
            return;
        }
        if state
            .last_place
            .is_some_and(|at| now.saturating_sub(at) < 250)
        {
            return;
        }
        let Some(schematic) = state.schematic.clone() else {
            return;
        };
        state.last_place = Some(now);
        drop(state);
        if place_next(host, &schematic) {
            return;
        }
        let mut state = self.state.lock().unwrap();
        state.auto = false;
        state.auto_note = Some("auto-place: nothing reachable left".into());
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
                format!(
                    "G: hide  L: reload  E: easy-place {}  B: next  T: auto {}",
                    if state.easy_place { "on" } else { "off" },
                    if state.auto { "on" } else { "off" }
                ),
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
        if let Some(note) = &state.auto_note {
            event.draw.text(note, [8.0, 50.0], 0xFF80FF80, true);
        }
        if state.verify {
            event.draw.text(
                &format!(
                    "Verify: {} ok  {} wrong  {} missing",
                    state.verify_tally[0], state.verify_tally[1], state.verify_tally[2]
                ),
                [8.0, 64.0],
                0xFF80FF80,
                true,
            );
        }
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
                let file = state.file.clone();
                match load(host, file.as_deref()) {
                    Ok(schematic) => state.schematic = Some(schematic),
                    Err(error) => state.error = Some(error),
                }
            }
        }

        let mut state = self.state.lock().unwrap();
        let Some(schematic) = state.schematic.as_ref() else {
            return;
        };
        let level = host.local_player().and_then(|player| player.level(host));

        let camera = event.view.camera;
        // The camera is the player's eye; feet are about 1.6 blocks lower.
        let player_y = (camera[1] - 1.6).floor() as i32;
        let mut quads = Vec::with_capacity(MAX_QUADS.min(1024));
        let mut outline = Vec::new();
        let mut tally = [0usize; 3];
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
                    Layers::Single => placed.pos[1] == region.origin[1] + state.center,
                    Layers::Range => (region.origin[1] + state.center - state.half
                        ..=region.origin[1] + state.center + state.half)
                        .contains(&placed.pos[1]),
                    Layers::Follow => placed.pos[1] == player_y,
                };
                if !shown
                    || distance_squared(placed.quad.corners[0], camera) > DRAW_DISTANCE.powi(2)
                {
                    continue;
                }
                if quads.len() == MAX_QUADS {
                    break;
                }
                let mut color = placed.quad.color;
                if state.verify {
                    let world = level
                        .and_then(|level| level.block_state(host, placed.pos))
                        .and_then(|block| host.block_state_string(block));
                    match world.as_deref() {
                        Some(name) if name == placed.state => {
                            tally[0] += 1;
                            color = 0x60_40_FF_40; // correct: green
                        }
                        Some(name) if name != "minecraft:air" => {
                            tally[1] += 1;
                            color = 0x60_40_40_FF; // wrong block: red
                        }
                        _ => {
                            tally[2] += 1;
                            color = 0x60_FF_C0_40; // missing: orange
                        }
                    }
                }
                quads.push(WorldQuad::new(
                    placed.quad.corners,
                    color,
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
        if state.verify {
            state.verify_tally = tally;
        }
    }

    fn client_tick_pre(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPreEvent) {
        if self.close_config.swap(false, Ordering::Relaxed) {
            host.mod_screen_close(SCREEN_ID);
        }
    }

    fn screen_render_post(&self, host: &dyn ClientHost, event: &mut ScreenRenderPostEvent<'_>) {
        if host.mod_screen() != Some(SCREEN_ID) {
            return;
        }
        let mut guard = self.config.lock().unwrap();
        let ui = guard.get_or_insert_with(|| ConfigUi::new(host));
        let draw = &mut *event.draw;
        draw.fill(
            [0.0, 0.0, 340.0, 80.0 + VISIBLE_ROWS as f32 * ROW_H],
            0xE0_10_10_14,
        );
        draw.text(
            "Schematic files (click to load)",
            [LIST_X, LIST_Y - 14.0],
            0xFFFFC840,
            true,
        );
        let _ = ui.widgets.draw(draw, host, event.screen.mouse);
        let layer = self.state.lock().unwrap();
        draw.text(
            &format!(
                "Mode: {:?}   layer {}   range +/-{}   easy {}   auto {}",
                layer.layers, layer.center, layer.half, layer.easy_place, layer.auto
            ),
            [20.0, 88.0 + VISIBLE_ROWS as f32 * ROW_H],
            0xFFB0B0B0,
            true,
        );
        drop(layer);
        for (slot, name) in ui
            .rows
            .iter()
            .skip(ui.offset)
            .take(VISIBLE_ROWS)
            .enumerate()
        {
            let y = LIST_Y + slot as f32 * ROW_H;
            if ui.row_at(event.screen.mouse) == Some(ui.offset + slot) {
                draw.fill([LIST_X, y, LIST_W, ROW_H], 0x40FFFFFF);
            }
            draw.text(name, [LIST_X + 2.0, y + 2.0], 0xFFFFFFFF, true);
        }
    }

    fn screen_mouse_button_pressed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseButtonPressedPreEvent<'_>,
    ) -> Flow {
        let (at, button, modifiers) = (event.screen.mouse, event.button, event.modifiers);
        let flow = self.config_input(host, |w| w.mouse_pressed(host, at, button, modifiers));
        if flow == Flow::Cancel {
            return flow;
        }
        self.select_row(host, at)
    }

    fn screen_mouse_button_released_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseButtonReleasedPreEvent<'_>,
    ) -> Flow {
        self.config_input(host, |w| w.mouse_released(event.button))
    }

    fn screen_mouse_dragged_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseDraggedPreEvent<'_>,
    ) -> Flow {
        let (at, button) = (event.screen.mouse, event.button);
        self.config_input(host, |w| w.mouse_dragged(host, at, button))
    }

    fn screen_mouse_scrolled_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseScrolledPreEvent<'_>,
    ) -> Flow {
        if host.mod_screen() != Some(SCREEN_ID) {
            return Flow::Continue;
        }
        let mut guard = self.config.lock().unwrap();
        let Some(ui) = guard.as_mut() else {
            return Flow::Continue;
        };
        let before = ui.offset;
        ui.offset = ui
            .offset
            .saturating_add_signed((event.scroll[1] as isize).saturating_neg())
            .min(ui.rows.len().saturating_sub(VISIBLE_ROWS));
        if ui.offset == before {
            Flow::Continue
        } else {
            Flow::Cancel
        }
    }

    fn screen_key_pressed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenKeyPressedPreEvent<'_>,
    ) -> Flow {
        self.config_input(host, |w| w.key_pressed(host, event.key, event.modifiers))
    }

    fn screen_char_typed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenCharTypedPreEvent<'_>,
    ) -> Flow {
        self.config_input(host, |w| w.char_typed(host, event.c))
    }
}

fn distance_squared(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Place one ghost block through ordinary gameplay: select its item, aim at the top of the
/// supporting block and click `key.use`. The game then builds its own packet, with the correct
/// sequence number and all vanilla reach/cooldown checks.
fn place_next(host: &mut dyn ClientHost, schematic: &Schematic) -> bool {
    let Some(player) = host.local_player() else {
        return false;
    };
    let Some(eye) = player.entity().eye_position(host) else {
        return false;
    };
    let Some(level) = player.level(host) else {
        return false;
    };

    let mut best: Option<(&PlacedQuad, (i32, f64))> = None;
    for region in &schematic.regions {
        for cell in &region.quads {
            // Already correct: skip it, so an auto-place loop never repeats a cell.
            if level
                .block_state(host, cell.pos)
                .and_then(|state| host.block_state_string(state))
                .as_deref()
                == Some(cell.state.as_str())
            {
                continue;
            }
            let distance = distance_squared(
                [
                    cell.pos[0] as f64 + 0.5,
                    cell.pos[1] as f64 + 0.5,
                    cell.pos[2] as f64 + 0.5,
                ],
                eye,
            );
            if distance > 5.0f64.powi(2) {
                continue;
            }
            // Only a cell whose supporting block is real can be placed right now.
            let support = [cell.pos[0], cell.pos[1] - 1, cell.pos[2]];
            let supported = level
                .block_state(host, support)
                .and_then(|state| host.block_state_string(state))
                .is_some_and(|name| name != "minecraft:air");
            if !supported {
                continue;
            }
            // Bottom-up first, nearest first within a layer.
            let rank = (cell.pos[1], distance);
            if best.as_ref().is_none_or(|(_, seen)| &rank < seen) {
                best = Some((cell, rank));
            }
        }
    }
    let Some((cell, _)) = best else {
        return false;
    };

    // Find the real hotbar slot, not a compacted inventory index. `select_hotbar_slot`
    // updates the selected slot immediately, so the held item can be checked right away.
    let wanted = host.block_state_id(&cell.state).map(|state| state.block());
    let Some(slot) = find_hotbar_slot(host, player, wanted) else {
        return false;
    };
    let _ = host.select_hotbar_slot(slot);

    // Aim at the top face of the supporting block (one below the ghost cell).
    let aim = [
        cell.pos[0] as f64 + 0.5,
        cell.pos[1] as f64 - 0.05,
        cell.pos[2] as f64 + 0.5,
    ];
    let (yaw, pitch) = look_at(eye, aim);
    let _ = host.set_local_rotation(yaw, pitch);
    host.click_mapping("key.use").is_ok()
}

/// Scan literal hotbar slots 0..=8 rather than trusting the client's compacted inventory list.
fn find_hotbar_slot(
    host: &mut dyn ClientHost,
    player: mod_api::PlayerRef,
    wanted: Option<mod_api::BlockKey>,
) -> Option<u8> {
    let living = LivingRef::new(player.entity());
    for slot in 0..=8u8 {
        if host.select_hotbar_slot(slot).is_err() {
            continue;
        }
        if player.selected_slot(host) != Some(slot as usize) {
            continue;
        }
        let held = living.hand_item(host, Hand::Main);
        if let Some(item) = held
            && host
                .item_block(item.item())
                .is_some_and(|block| Some(block) == wanted)
        {
            return Some(slot);
        }
    }
    None
}

/// Vanilla yaw/pitch for a view vector pointing from `eye` to `at`.
fn look_at(eye: [f64; 3], at: [f64; 3]) -> (f32, f32) {
    let (dx, dy, dz) = (at[0] - eye[0], at[1] - eye[1], at[2] - eye[2]);
    let yaw = mod_api::strict_math::atan2(-dx, dz).to_degrees() as f32;
    let pitch = mod_api::strict_math::atan2(-dy, dx.hypot(dz)).to_degrees() as f32;
    (yaw, pitch)
}

/// Read `build.litematic` (or `build.schem`) from this mod's data files and decode it.
fn load(host: &dyn ClientHost, chosen: Option<&str>) -> Result<Schematic, String> {
    let (name, compressed) = if let Some(file) = chosen {
        let bytes = read_file(host, file)?
            .ok_or_else(|| format!("{file} is gone from the mod's data folder"))?;
        (file, Ok(bytes))
    } else {
        ["build.litematic", "build.schem"]
            .into_iter()
            .find_map(|file| read_file(host, file).transpose().map(|bytes| (file, bytes)))
            .ok_or("put build.litematic or build.schem in horizon-mod-data/ghost_litematic/")?
    };
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
            pos: [wx as i32, wy as i32, wz as i32],
            state: state_string(entry),
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
            pos: [wx as i32, wy as i32, wz as i32],
            state: state.to_string(),
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

/// `minecraft:oak_stairs[facing=east,waterlogged=false]` — the form `block_state_id` parses.
fn state_string(entry: &Tag) -> String {
    let Some(Tag::String(name)) = entry.get("Name") else {
        return "minecraft:air".into();
    };
    let name = name.to_string_lossy();
    let name = if name.contains(':') {
        name
    } else {
        format!("minecraft:{name}")
    };
    let mut properties = Vec::new();
    if let Some(Tag::Compound(props)) = entry.get("Properties") {
        for (key, value) in props {
            if let Tag::String(value) = value {
                properties.push(format!(
                    "{}={}",
                    key.to_string_lossy(),
                    value.to_string_lossy()
                ));
            }
        }
    }
    if properties.is_empty() {
        name
    } else {
        format!("{}[{}]", name, properties.join(","))
    }
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
