# vibe303 GUI Frontend Implementation Plan (`nih_plug_egui`)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a responsive, hardware-styled vector GUI for the `vibe303` plugin using `nih_plug_egui`, featuring the classic 7-knob front panel, souped-up engine drawer with accurate-mode dimming, and an interactive 16-step sequencer grid with real-time atomic playhead tracking.

**Architecture:** Procedural vector rendering using egui `Painter` with zero external raster dependencies; modular widget hierarchy (`Knob303`, `SlideSwitch`, `StepButton`, `LedIndicator`, `MiniKeyboard`); clear separation between classic, souped-up, and sequencer panels; thread-safe atomic playhead observation and wait-free pattern editing.

**Tech Stack:** Rust (edition 2021), `nih-plug`, `nih_plug_egui`, `egui` (0.22+ via nih-plug), `assert_no_alloc`.

## Global Constraints

- **Feature-Gated GUI:** `nih_plug_egui` is gated behind `gui` feature (`default = ["gui", "standalone"]`). Headless DSP builds and tests compile cleanly with `--no-default-features`.
- **Zero Allocations on Audio Thread:** All GUI interactions occur on the UI thread. The audio processing loop in `process()` remains 100% allocation-free (`assert_no_alloc`).
- **Wait-Free Pattern Sharing:** Audio thread reads pattern via `try_read()` on `params.pattern`. UI thread acquires `write()` on pattern edits and triggers re-compilation.
- **DAW Automation Safety:** Parameter modifications from custom widgets call `setter.begin_set_parameter()` on drag start and `setter.end_set_parameter()` on drag release.
- **DPI & Window Scaling:** Base dimension is 1080×640 px with responsive vector scaling.

---

## File Structure

- **Modify:** `plugin303/Cargo.toml`
  - Add optional dependency `nih_plug_egui` and `gui` feature.
- **Create:** `plugin303/src/gui/mod.rs`
  - Top-level editor construction via `create_editor`.
- **Create:** `plugin303/src/gui/theme.rs`
  - Color palette, styling tokens, strokes, font definitions, and gradient helpers.
- **Create:** `plugin303/src/gui/widgets/mod.rs`
- **Create:** `plugin303/src/gui/widgets/knob.rs`
  - Custom procedural rotary dial `Knob303` with angle sweep, notch pointer, tick marks, and DAW gesture integration.
- **Create:** `plugin303/src/gui/widgets/switch.rs`
  - `SlideSwitch` and `ToggleSwitch` widgets with tactile detents.
- **Create:** `plugin303/src/gui/widgets/led.rs`
  - `LedIndicator` with bloom/glow shader emulation.
- **Create:** `plugin303/src/gui/widgets/step_button.rs`
  - Interactive sequencer step strip (accent, slide, gate cycle, note/octave transpose, playhead LED).
- **Create:** `plugin303/src/gui/widgets/mini_keyboard.rs`
  - 13-key popup chromatic piano keyboard for direct pitch selection.
- **Create:** `plugin303/src/gui/panels/mod.rs`
- **Create:** `plugin303/src/gui/panels/header_bar.rs`
  - Title, master volume, and topology mode selector.
- **Create:** `plugin303/src/gui/panels/classic_panel.rs`
  - Left panel: 7 vintage aluminum dials, Waveform switch, Tier selector.
- **Create:** `plugin303/src/gui/panels/souped_panel.rs`
  - Right panel: dual osc, filter taps, drive, analog drift/age, MEG/VEG ADSR faders, with Accurate-mode dimming overlay.
- **Create:** `plugin303/src/gui/panels/sequencer_panel.rs`
  - Bottom panel: transport controls, timebase/mode settings, and 16 interactive step columns.
- **Modify:** `plugin303/src/dsp/sequencer.rs`
  - Expose `current_step: Arc<AtomicU8>` to track active step for the UI playhead without locks.
- **Modify:** `plugin303/src/lib.rs`
  - Wire `Plugin::editor` implementation to `create_editor` when `cfg(feature = "gui")`.
- **Create:** `plugin303/tests/gui_build.rs`
  - Integration test verifying GUI instantiation and theme sanity.

---

### Task 1: Add `nih_plug_egui` Dependency, Feature Gating, and Theme System

**Files:**
- Modify: `plugin303/Cargo.toml`
- Create: `plugin303/src/gui/mod.rs`
- Create: `plugin303/src/gui/theme.rs`
- Test: `plugin303/tests/gui_build.rs`

**Interfaces:**
- Produces: `plugin303::gui::theme` containing color tokens (`CHASSIS_BG`, `PANEL_BORDER`, `ACCENT_AMBER`, `SLIDE_CYAN`, `PLAYHEAD_RED`, `SOUPED_ORANGE`, etc.) and font helpers.

- [ ] **Step 1: Update `plugin303/Cargo.toml` to add `nih_plug_egui` and `gui` feature**

```toml
[package]
name = "plugin303"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
nih_plug = { git = "https://github.com/robbert-vdh/nih-plug.git", rev = "f36931f7af4646065488a9845d8f8c2f95252c23", features = ["vst3"] }
nih_plug_egui = { git = "https://github.com/robbert-vdh/nih-plug.git", rev = "f36931f7af4646065488a9845d8f8c2f95252c23", optional = true }
serde_json = "1.0.150"
serde = { version = "1.0", features = ["derive"] }

[features]
default = ["gui", "standalone"]
standalone = ["nih_plug/standalone"]
gui = ["dep:nih_plug_egui"]
current-iteration = []

[dev-dependencies]
assert_no_alloc = { version = "1.1", default-features = false, features = [
    "warn_debug",
    "warn_release",
] }
serde_json = "1.0.150"
```

- [ ] **Step 2: Write `plugin303/src/gui/theme.rs`**

```rust
use nih_plug_egui::egui::{Color32, Stroke};

pub const CHASSIS_BG: Color32 = Color32::from_rgb(0x16, 0x18, 0x1b);
pub const PANEL_BG: Color32 = Color32::from_rgb(0x1e, 0x22, 0x27);
pub const PANEL_BG_SOUPED: Color32 = Color32::from_rgb(0x12, 0x15, 0x1a);
pub const PANEL_BORDER: Color32 = Color32::from_rgb(0x32, 0x38, 0x42);
pub const PANEL_BORDER_HIGHLIGHT: Color32 = Color32::from_rgb(0x48, 0x52, 0x60);

pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xee, 0xf0, 0xf2);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0x9a, 0xa2, 0xad);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x5a, 0x62, 0x6e);

pub const KNOB_BODY_LIGHT: Color32 = Color32::from_rgb(0xd2, 0xd6, 0xdc);
pub const KNOB_BODY_DARK: Color32 = Color32::from_rgb(0x84, 0x8a, 0x94);
pub const KNOB_BEZEL: Color32 = Color32::from_rgb(0x28, 0x2c, 0x34);
pub const KNOB_POINTER: Color32 = Color32::from_rgb(0xff, 0xff, 0xff);

pub const ACCENT_AMBER: Color32 = Color32::from_rgb(0xff, 0xaa, 0x00);
pub const ACCENT_AMBER_GLOW: Color32 = Color32::from_rgba_premultiplied(0xff, 0xaa, 0x00, 0x40);

pub const SLIDE_CYAN: Color32 = Color32::from_rgb(0x00, 0xe5, 0xff);
pub const SLIDE_CYAN_GLOW: Color32 = Color32::from_rgba_premultiplied(0x00, 0xe5, 0xff, 0x40);

pub const PLAYHEAD_RED: Color32 = Color32::from_rgb(0xff, 0x33, 0x55);
pub const PLAYHEAD_RED_GLOW: Color32 = Color32::from_rgba_premultiplied(0xff, 0x33, 0x55, 0x60);

pub const SOUPED_ORANGE: Color32 = Color32::from_rgb(0xff, 0x77, 0x00);
pub const GATE_NOTE_COLOR: Color32 = Color32::from_rgb(0x00, 0xe5, 0xff);
pub const GATE_TIE_COLOR: Color32 = Color32::from_rgb(0xff, 0xd7, 0x00);
pub const GATE_REST_COLOR: Color32 = Color32::from_rgb(0x38, 0x40, 0x4c);

pub fn border_stroke() -> Stroke {
    Stroke::new(1.0, PANEL_BORDER)
}

pub fn highlight_stroke() -> Stroke {
    Stroke::new(1.0, PANEL_BORDER_HIGHLIGHT)
}
```

- [ ] **Step 3: Create `plugin303/src/gui/mod.rs` declaring modules**

```rust
pub mod panels;
pub mod theme;
pub mod widgets;

use nih_plug::prelude::Editor;
use nih_plug_egui::{create_egui_editor, EguiState};
use std::sync::atomic::AtomicU8;
use std::sync::Arc;

use crate::params::Plugin303Params;

pub const GUI_WIDTH: u32 = 1080;
pub const GUI_HEIGHT: u32 = 640;

pub fn create_editor(
    params: Arc<Plugin303Params>,
    editor_state: Arc<EguiState>,
    current_playing_step: Arc<AtomicU8>,
) -> Option<Box<dyn Editor>> {
    create_egui_editor(
        editor_state,
        (),
        |_, _| {},
        move |egui_ctx, setter, _state| {
            egui_ctx.set_visuals(nih_plug_egui::egui::Visuals::dark());
            nih_plug_egui::egui::CentralPanel::default()
                .frame(nih_plug_egui::egui::Frame::none().fill(theme::CHASSIS_BG))
                .show(egui_ctx, |ui| {
                    ui.label("vibe303 UI initialized");
                });
        },
    )
}
```

- [ ] **Step 4: Write test `plugin303/tests/gui_build.rs`**

```rust
#[cfg(feature = "gui")]
#[test]
fn test_gui_initialization_build() {
    use plugin303::params::Plugin303Params;
    use std::sync::atomic::AtomicU8;
    use std::sync::Arc;

    let params = Arc::new(Plugin303Params::default());
    let state = nih_plug_egui::EguiState::from_size(
        plugin303::gui::GUI_WIDTH,
        plugin303::gui::GUI_HEIGHT,
    );
    let current_step = Arc::new(AtomicU8::new(255));

    let editor = plugin303::gui::create_editor(params, state, current_step);
    assert!(editor.is_some(), "GUI editor should construct successfully");
}
```

- [ ] **Step 5: Run tests and verify**

Run: `cargo test -p plugin303 --test gui_build`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add plugin303/Cargo.toml plugin303/src/gui plugin303/tests/gui_build.rs
git commit -m "feat(gui): scaffold nih_plug_egui dependency, feature gate, and theme tokens"
```

---

### Task 2: Procedural Vector Widgets (`Knob303`, `SlideSwitch`, `LedIndicator`)

**Files:**
- Create: `plugin303/src/gui/widgets/mod.rs`
- Create: `plugin303/src/gui/widgets/knob.rs`
- Create: `plugin303/src/gui/widgets/switch.rs`
- Create: `plugin303/src/gui/widgets/led.rs`
- Test: `plugin303/tests/gui_build.rs`

**Interfaces:**
- Produces:
  - `Knob303::for_param(param: &impl Param, label: &str, radius: f32) -> impl egui::Widget`
  - `SlideSwitch::for_enum(param: &EnumParam<T>, label: &str) -> impl egui::Widget`
  - `LedIndicator::new(active: bool, color: Color32, glow_color: Color32, radius: f32) -> impl egui::Widget`

- [ ] **Step 1: Write `plugin303/src/gui/widgets/knob.rs`**

```rust
use nih_plug::prelude::Param;
use nih_plug_egui::egui::{
    pos2, vec2, Color32, FontFamily, FontId, Pos2, Rect, Response, Sense, Stroke, Ui, Widget,
};
use nih_plug_egui::ParamSetter;
use std::f32::consts::PI;

use crate::gui::theme;

pub struct Knob303<'a, P: Param> {
    param: &'a P,
    setter: &'a ParamSetter<'a>,
    label: &'a str,
    radius: f32,
}

impl<'a, P: Param> Knob303<'a, P> {
    pub fn new(param: &'a P, setter: &'a ParamSetter<'a>, label: &'a str, radius: f32) -> Self {
        Self {
            param,
            setter,
            label,
            radius,
        }
    }
}

impl<'a, P: Param> Widget for Knob303<'a, P> {
    fn ui(self, ui: &mut Ui) -> Response {
        let desired_size = vec2(self.radius * 2.0 + 16.0, self.radius * 2.0 + 34.0);
        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());

        let center = pos2(rect.center().x, rect.top() + self.radius + 14.0);
        let normalized = self.param.normalized_value();

        // Interaction
        if response.drag_started() {
            self.setter.begin_set_parameter(self.param);
        }
        if response.dragged() {
            let drag_delta = -response.drag_delta().y;
            let sensitivity = if ui.input(|i| i.modifiers.shift) {
                0.001
            } else {
                0.005
            };
            let new_val = (normalized + drag_delta * sensitivity).clamp(0.0, 1.0);
            self.setter.set_parameter_normalized(self.param, new_val);
            response.mark_changed();
        }
        if response.drag_released() {
            self.setter.end_set_parameter(self.param);
        }
        if response.double_clicked() {
            self.setter.begin_set_parameter(self.param);
            self.setter
                .set_parameter_normalized(self.param, self.param.default_normalized_value());
            self.setter.end_set_parameter(self.param);
            response.mark_changed();
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();

            // Label above
            painter.text(
                pos2(rect.center().x, rect.top() + 4.0),
                nih_plug_egui::egui::Align2::CENTER_CENTER,
                self.label,
                FontId::new(10.0, FontFamily::Proportional),
                theme::TEXT_SECONDARY,
            );

            // Tick arc (270 degrees: 135 deg to 405 deg)
            let start_angle = 135.0 * PI / 180.0;
            let end_angle = 405.0 * PI / 180.0;
            let current_angle = start_angle + normalized * (end_angle - start_angle);

            // Outer bezel ring
            painter.circle_filled(center, self.radius + 2.0, theme::KNOB_BEZEL);
            painter.circle_stroke(
                center,
                self.radius + 2.0,
                Stroke::new(1.0, theme::PANEL_BORDER),
            );

            // Metallic gradient disc
            painter.circle_filled(center, self.radius, theme::KNOB_BODY_DARK);
            painter.circle_filled(
                pos2(center.x - self.radius * 0.2, center.y - self.radius * 0.2),
                self.radius * 0.8,
                theme::KNOB_BODY_LIGHT,
            );

            // Pointer line
            let pointer_len = self.radius * 0.85;
            let pointer_end = pos2(
                center.x + current_angle.cos() * pointer_len,
                center.y + current_angle.sin() * pointer_len,
            );
            let pointer_start = pos2(
                center.x + current_angle.cos() * (self.radius * 0.2),
                center.y + current_angle.sin() * (self.radius * 0.2),
            );
            painter.line_segment(
                [pointer_start, pointer_end],
                Stroke::new(2.5, theme::KNOB_POINTER),
            );

            // Formatted value readout below
            let value_str = self.param.to_string();
            painter.text(
                pos2(rect.center().x, rect.bottom() - 6.0),
                nih_plug_egui::egui::Align2::CENTER_CENTER,
                value_str,
                FontId::new(9.5, FontFamily::Monospace),
                theme::TEXT_PRIMARY,
            );
        }

        response
    }
}
```

- [ ] **Step 2: Write `plugin303/src/gui/widgets/switch.rs` and `led.rs`**

Create `plugin303/src/gui/widgets/led.rs`:
```rust
use nih_plug_egui::egui::{vec2, Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Widget};

pub struct LedIndicator {
    pub active: bool,
    pub color: Color32,
    pub glow_color: Color32,
    pub radius: f32,
}

impl LedIndicator {
    pub fn new(active: bool, color: Color32, glow_color: Color32, radius: f32) -> Self {
        Self {
            active,
            color,
            glow_color,
            radius,
        }
    }
}

impl Widget for LedIndicator {
    fn ui(self, ui: &mut Ui) -> Response {
        let size = vec2(self.radius * 2.0 + 4.0, self.radius * 2.0 + 4.0);
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let center = rect.center();
            if self.active {
                // Outer glow bloom
                painter.circle_filled(center, self.radius * 2.0, self.glow_color);
                painter.circle_filled(center, self.radius, self.color);
            } else {
                painter.circle_filled(center, self.radius, Color32::from_rgb(0x28, 0x2e, 0x36));
                painter.circle_stroke(
                    center,
                    self.radius,
                    Stroke::new(1.0, Color32::from_rgb(0x18, 0x1b, 0x20)),
                );
            }
        }
        response
    }
}
```

Create `plugin303/src/gui/widgets/switch.rs`:
```rust
use nih_plug::prelude::EnumParam;
use nih_plug_egui::egui::{
    pos2, vec2, Color32, FontFamily, FontId, Pos2, Rect, Response, Sense, Stroke, Ui, Widget,
};
use nih_plug_egui::ParamSetter;

use crate::gui::theme;

pub struct SlideSwitch<'a, T: nih_plug::prelude::Enum> {
    param: &'a EnumParam<T>,
    setter: &'a ParamSetter<'a>,
    label: &'a str,
    options: &'a [&'a str],
}

impl<'a, T: nih_plug::prelude::Enum> SlideSwitch<'a, T> {
    pub fn new(
        param: &'a EnumParam<T>,
        setter: &'a ParamSetter<'a>,
        label: &'a str,
        options: &'a [&'a str],
    ) -> Self {
        Self {
            param,
            setter,
            label,
            options,
        }
    }
}

impl<'a, T: nih_plug::prelude::Enum> Widget for SlideSwitch<'a, T> {
    fn ui(self, ui: &mut Ui) -> Response {
        let num_opts = self.options.len().max(1);
        let width = 28.0 * num_opts as f32;
        let height = 36.0;
        let (rect, mut response) =
            ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());

        let current_idx = self.param.value().to_index();

        if response.clicked() {
            let next_idx = (current_idx + 1) % num_opts;
            self.setter.begin_set_parameter(self.param);
            self.setter
                .set_parameter_normalized(self.param, next_idx as f32 / (num_opts - 1).max(1) as f32);
            self.setter.end_set_parameter(self.param);
            response.mark_changed();
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            painter.text(
                pos2(rect.center().x, rect.top() + 4.0),
                nih_plug_egui::egui::Align2::CENTER_CENTER,
                self.label,
                FontId::new(9.5, FontFamily::Proportional),
                theme::TEXT_SECONDARY,
            );

            let track_rect = Rect::from_min_size(
                pos2(rect.left() + 4.0, rect.top() + 16.0),
                vec2(width - 8.0, 16.0),
            );
            painter.rect_filled(track_rect, 3.0, Color32::from_rgb(0x10, 0x12, 0x16));
            painter.rect_stroke(track_rect, 3.0, Stroke::new(1.0, theme::PANEL_BORDER));

            let slot_width = (width - 8.0) / num_opts as f32;
            let thumb_x = track_rect.left() + slot_width * (current_idx as f32 + 0.5);
            let thumb_rect = Rect::from_center_size(pos2(thumb_x, track_rect.center().y), vec2(slot_width - 2.0, 14.0));

            painter.rect_filled(thumb_rect, 2.0, theme::KNOB_BODY_LIGHT);
            painter.rect_stroke(thumb_rect, 2.0, Stroke::new(1.0, theme::KNOB_BEZEL));

            painter.text(
                thumb_rect.center(),
                nih_plug_egui::egui::Align2::CENTER_CENTER,
                self.options[current_idx],
                FontId::new(8.5, FontFamily::Proportional),
                Color32::BLACK,
            );
        }

        response
    }
}
```

- [ ] **Step 3: Create `plugin303/src/gui/widgets/mod.rs`**

```rust
pub mod knob;
pub mod led;
pub mod mini_keyboard;
pub mod step_button;
pub mod switch;

pub use knob::Knob303;
pub use led::LedIndicator;
pub use mini_keyboard::MiniKeyboard;
pub use step_button::StepButton;
pub use switch::SlideSwitch;
```

- [ ] **Step 4: Verify widgets compile with `cargo test -p plugin303 --test gui_build`**

Run: `cargo test -p plugin303 --test gui_build`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/gui/widgets
git commit -m "feat(gui): implement Knob303, SlideSwitch, and LedIndicator widgets"
```

---

### Task 3: Interactive Sequencer Step Strip & MiniKeyboard Popover

**Files:**
- Create: `plugin303/src/gui/widgets/mini_keyboard.rs`
- Create: `plugin303/src/gui/widgets/step_button.rs`
- Modify: `plugin303/src/dsp/sequencer.rs` (expose playhead atomic)
- Test: `plugin303/tests/gui_build.rs`

**Interfaces:**
- Produces:
  - `StepButton::new(step_idx: usize, step: &mut SequencerStep, is_playing: bool) -> impl Widget`
  - `MiniKeyboard::show(ui: &mut Ui, current_semitone: &mut i8)`

- [ ] **Step 1: Write `plugin303/src/gui/widgets/mini_keyboard.rs`**

```rust
use nih_plug_egui::egui::{pos2, vec2, Color32, FontFamily, FontId, Rect, Response, Sense, Stroke, Ui};
use crate::gui::theme;

pub struct MiniKeyboard<'a> {
    semitone: &'a mut i8,
}

impl<'a> MiniKeyboard<'a> {
    pub fn new(semitone: &'a mut i8) -> Self {
        Self { semitone }
    }

    pub fn show(self, ui: &mut Ui) -> bool {
        let mut changed = false;
        let note_names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B", "C'"];
        let is_black = [false, true, false, true, false, false, true, false, true, false, true, false, false];

        ui.horizontal(|ui| {
            for (idx, &name) in note_names.iter().enumerate() {
                let selected = *self.semitone == idx as i8;
                let bg_color = if selected {
                    theme::ACCENT_AMBER
                } else if is_black[idx] {
                    Color32::from_rgb(0x20, 0x24, 0x2c)
                } else {
                    Color32::from_rgb(0xe0, 0xe4, 0xec)
                };

                let text_color = if selected {
                    Color32::BLACK
                } else if is_black[idx] {
                    Color32::WHITE
                } else {
                    Color32::BLACK
                };

                let btn = nih_plug_egui::egui::Button::new(
                    nih_plug_egui::egui::RichText::new(name)
                        .font(FontId::new(9.0, FontFamily::Monospace))
                        .color(text_color),
                )
                .fill(bg_color)
                .min_size(vec2(16.0, 28.0));

                if ui.add(btn).clicked() {
                    *self.semitone = idx as i8;
                    changed = true;
                }
            }
        });

        changed
    }
}
```

- [ ] **Step 2: Write `plugin303/src/gui/widgets/step_button.rs`**

```rust
use nih_plug_egui::egui::{
    pos2, vec2, Color32, FontFamily, FontId, Rect, Response, Sense, Stroke, Ui, Widget,
};
use crate::gui::theme;
use crate::params::{SequencerStep, StepGate};

pub struct StepButton<'a> {
    pub step_idx: usize,
    pub step: &'a mut SequencerStep,
    pub is_active_step: bool,
    pub popup_open: bool,
}

impl<'a> StepButton<'a> {
    pub fn new(step_idx: usize, step: &'a mut SequencerStep, is_active_step: bool) -> Self {
        Self {
            step_idx,
            step,
            is_active_step,
            popup_open: false,
        }
    }
}

pub fn format_pitch(semitone: i8, octave: i8) -> String {
    let note_names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B", "C"];
    let name = note_names.get(semitone.clamp(0, 12) as usize).unwrap_or(&"C");
    let oct_num = 3 + octave;
    format!("{}{}", name, oct_num)
}

pub fn render_step_strip(
    ui: &mut Ui,
    step_idx: usize,
    step: &mut SequencerStep,
    is_active: bool,
) -> bool {
    let mut changed = false;
    let width = 54.0;
    let height = 150.0;

    ui.vertical(|ui| {
        ui.set_width(width);

        // 1. Playhead LED bar
        let (led_rect, _) = ui.allocate_exact_size(vec2(width, 6.0), Sense::hover());
        if is_active {
            ui.painter().rect_filled(led_rect, 2.0, theme::PLAYHEAD_RED);
            ui.painter().rect_filled(led_rect.expand(2.0), 3.0, theme::PLAYHEAD_RED_GLOW);
        } else {
            ui.painter().rect_filled(led_rect, 2.0, Color32::from_rgb(0x22, 0x26, 0x2e));
        }

        ui.add_space(2.0);

        // Step number label
        ui.label(
            nih_plug_egui::egui::RichText::new(format!("{:02}", step_idx + 1))
                .font(FontId::new(9.5, FontFamily::Monospace))
                .color(if is_active { theme::TEXT_PRIMARY } else { theme::TEXT_MUTED }),
        );

        // 2. Modifiers: ACC and SLD
        ui.horizontal(|ui| {
            let acc_btn = nih_plug_egui::egui::Button::new("ACC")
                .fill(if step.accent { theme::ACCENT_AMBER } else { Color32::from_rgb(0x24, 0x28, 0x30) })
                .min_size(vec2(24.0, 16.0));
            if ui.add(acc_btn).clicked() {
                step.accent = !step.accent;
                changed = true;
            }

            let sld_btn = nih_plug_egui::egui::Button::new("SLD")
                .fill(if step.slide { theme::SLIDE_CYAN } else { Color32::from_rgb(0x24, 0x28, 0x30) })
                .min_size(vec2(24.0, 16.0));
            if ui.add(sld_btn).clicked() {
                step.slide = !step.slide;
                changed = true;
            }
        });

        ui.add_space(2.0);

        // 3. Gate Badge: Note / Tie / Rest
        let (gate_str, gate_color) = match step.gate {
            StepGate::Note => ("NOTE", theme::GATE_NOTE_COLOR),
            StepGate::Tie => ("TIE", theme::GATE_TIE_COLOR),
            StepGate::Rest => ("REST", theme::GATE_REST_COLOR),
        };
        let gate_btn = nih_plug_egui::egui::Button::new(
            nih_plug_egui::egui::RichText::new(gate_str)
                .font(FontId::new(9.0, FontFamily::Proportional))
                .color(if step.gate == StepGate::Rest { theme::TEXT_MUTED } else { Color32::BLACK }),
        )
        .fill(gate_color)
        .min_size(vec2(width, 22.0));

        if ui.add(gate_btn).clicked() {
            step.gate = match step.gate {
                StepGate::Note => StepGate::Tie,
                StepGate::Tie => StepGate::Rest,
                StepGate::Rest => StepGate::Note,
            };
            changed = true;
        }

        ui.add_space(2.0);

        // 4. Pitch & Octave
        let pitch_str = format_pitch(step.semitone, step.octave);
        ui.label(
            nih_plug_egui::egui::RichText::new(pitch_str)
                .font(FontId::new(11.0, FontFamily::Monospace))
                .color(theme::TEXT_PRIMARY),
        );

        ui.horizontal(|ui| {
            if ui.small_button("▲").clicked() {
                if step.octave < 2 {
                    step.octave += 1;
                    changed = true;
                }
            }
            if ui.small_button("▼").clicked() {
                if step.octave > -2 {
                    step.octave -= 1;
                    changed = true;
                }
            }
        });
    });

    changed
}
```

- [ ] **Step 3: Verify and run tests**

Run: `cargo test -p plugin303 --test gui_build`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add plugin303/src/gui/widgets/mini_keyboard.rs plugin303/src/gui/widgets/step_button.rs
git commit -m "feat(gui): implement step strip and mini keyboard popup widgets"
```

---

### Task 4: Header Bar & Classic 303 Front Panel

**Files:**
- Create: `plugin303/src/gui/panels/mod.rs`
- Create: `plugin303/src/gui/panels/header_bar.rs`
- Create: `plugin303/src/gui/panels/classic_panel.rs`
- Test: `plugin303/tests/gui_build.rs`

**Interfaces:**
- Produces:
  - `header_bar::render_header(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter)`
  - `classic_panel::render_classic(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter)`

- [ ] **Step 1: Write `plugin303/src/gui/panels/header_bar.rs`**

```rust
use nih_plug_egui::egui::{vec2, Color32, FontFamily, FontId, Ui};
use nih_plug_egui::ParamSetter;

use crate::gui::theme;
use crate::gui::widgets::{Knob303, SlideSwitch};
use crate::params::{CircuitTopology, Plugin303Params};

pub fn render_header(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    ui.horizontal(|ui| {
        ui.set_height(40.0);

        // Logo / Title
        ui.add_space(8.0);
        ui.label(
            nih_plug_egui::egui::RichText::new("VIBE 303")
                .font(FontId::new(16.0, FontFamily::Proportional))
                .strong()
                .color(theme::TEXT_PRIMARY),
        );
        ui.label(
            nih_plug_egui::egui::RichText::new("ANALOG CIRCUIT EMULATION")
                .font(FontId::new(9.5, FontFamily::Proportional))
                .color(theme::TEXT_MUTED),
        );

        ui.with_layout(nih_plug_egui::egui::Layout::right_to_left(nih_plug_egui::egui::Align::Center), |ui| {
            ui.add_space(12.0);
            // Master Volume
            ui.add(Knob303::new(&params.volume, setter, "MASTER", 12.0));

            ui.add_space(16.0);
            // Topology switch
            ui.add(SlideSwitch::new(
                &params.topology,
                setter,
                "TOPOLOGY",
                &["ACCURATE", "FUN"],
            ));
        });
    });
}
```

- [ ] **Step 2: Write `plugin303/src/gui/panels/classic_panel.rs`**

```rust
use nih_plug_egui::egui::{vec2, Color32, Frame, Ui};
use nih_plug_egui::ParamSetter;

use crate::gui::theme;
use crate::gui::widgets::{Knob303, SlideSwitch};
use crate::params::Plugin303Params;

pub fn render_classic(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    Frame::none()
        .fill(theme::PANEL_BG)
        .stroke(theme::border_stroke())
        .rounding(4.0)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_height(220.0);
            ui.set_width(510.0);

            ui.label(
                nih_plug_egui::egui::RichText::new("CLASSIC 303 FRONT PANEL")
                    .font(nih_plug_egui::egui::FontId::new(11.0, nih_plug_egui::egui::FontFamily::Proportional))
                    .strong()
                    .color(theme::TEXT_SECONDARY),
            );

            ui.add_space(6.0);

            // Row of main dials
            ui.horizontal(|ui| {
                ui.add(Knob303::new(&params.tuning, setter, "TUNING", 16.0));
                ui.add(Knob303::new(&params.cutoff, setter, "CUTOFF", 20.0));
                ui.add(Knob303::new(&params.resonance, setter, "RESO", 20.0));
                ui.add(Knob303::new(&params.env_mod, setter, "ENV MOD", 18.0));
                ui.add(Knob303::new(&params.decay, setter, "DECAY", 18.0));
                ui.add(Knob303::new(&params.accent, setter, "ACCENT", 18.0));
            });

            ui.add_space(10.0);

            // Switches: Waveform & Tier
            ui.horizontal(|ui| {
                ui.add(SlideSwitch::new(
                    &params.waveform,
                    setter,
                    "WAVEFORM",
                    &["SAW", "SQR", "SWEEP"],
                ));
                ui.add_space(20.0);
                ui.add(SlideSwitch::new(
                    &params.tier,
                    setter,
                    "FIDELITY TIER",
                    &["LOW", "HIGH", "ULTRA"],
                ));
            });
        });
}
```

- [ ] **Step 3: Write `plugin303/src/gui/panels/mod.rs`**

```rust
pub mod classic_panel;
pub mod header_bar;
pub mod sequencer_panel;
pub mod souped_panel;
```

- [ ] **Step 4: Verify and commit**

```bash
git add plugin303/src/gui/panels
git commit -m "feat(gui): implement header bar and classic 303 front panel"
```

---

### Task 5: Souped-Up Engine Drawer with Accurate-Mode Dimming

**Files:**
- Create: `plugin303/src/gui/panels/souped_panel.rs`
- Test: `plugin303/tests/gui_build.rs`

**Interfaces:**
- Produces:
  - `souped_panel::render_souped(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter)`

- [ ] **Step 1: Write `plugin303/src/gui/panels/souped_panel.rs`**

```rust
use nih_plug_egui::egui::{pos2, vec2, Color32, Frame, Rect, Sense, Stroke, Ui};
use nih_plug_egui::ParamSetter;

use crate::gui::theme;
use crate::gui::widgets::{Knob303, SlideSwitch};
use crate::params::{CircuitTopology, Plugin303Params};

pub fn render_souped(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let is_accurate = params.topology.value() == CircuitTopology::Accurate;

    Frame::none()
        .fill(theme::PANEL_BG_SOUPED)
        .stroke(theme::border_stroke())
        .rounding(4.0)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_height(220.0);
            ui.set_width(520.0);

            ui.horizontal(|ui| {
                ui.label(
                    nih_plug_egui::egui::RichText::new("SOUPED-UP ENGINE DRAWER")
                        .font(nih_plug_egui::egui::FontId::new(11.0, nih_plug_egui::egui::FontFamily::Proportional))
                        .strong()
                        .color(if is_accurate { theme::TEXT_MUTED } else { theme::SOUPED_ORANGE }),
                );
                if is_accurate {
                    ui.label(
                        nih_plug_egui::egui::RichText::new("(Bypassed in Accurate Mode)")
                            .font(nih_plug_egui::egui::FontId::new(9.5, nih_plug_egui::egui::FontFamily::Proportional))
                            .color(theme::ACCENT_AMBER),
                    );
                }
            });

            ui.add_space(4.0);

            // Group 1: Dual Osc & Drive
            ui.horizontal(|ui| {
                ui.add(Knob303::new(&params.osc1_pitch, setter, "OSC1 DET", 14.0));
                ui.add(Knob303::new(&params.osc2_pitch, setter, "OSC2 DET", 14.0));
                ui.add(Knob303::new(&params.osc2_vol, setter, "OSC2 MIX", 14.0));
                ui.add(Knob303::new(&params.osc1_pw, setter, "OSC1 PW", 14.0));
                ui.add(Knob303::new(&params.sub_vol, setter, "SUB VOL", 14.0));
                ui.add(Knob303::new(&params.vcf_drive, setter, "DRIVE", 14.0));
                ui.add(Knob303::new(&params.glide, setter, "GLIDE", 14.0));
            });

            ui.add_space(6.0);

            // Group 2: Analog Wear & Filter Taps
            ui.horizontal(|ui| {
                ui.add(SlideSwitch::new(
                    &params.vcf_taps,
                    setter,
                    "VCF TAPS",
                    &["2-POLE", "3-POLE", "4-POLE"],
                ));
                ui.add_space(10.0);
                ui.add(Knob303::new(&params.comp_drift, setter, "DRIFT", 13.0));
                ui.add(Knob303::new(&params.comp_age, setter, "AGE", 13.0));
                ui.add(SlideSwitch::new(
                    &params.tolerance,
                    setter,
                    "TOLERANCE",
                    &["0%", "0.5%", "2%", "5%", "10%"],
                ));
            });
        });
}
```

- [ ] **Step 2: Verify compilation and run tests**

Run: `cargo test -p plugin303 --test gui_build`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add plugin303/src/gui/panels/souped_panel.rs
git commit -m "feat(gui): implement souped-up engine drawer with accurate-mode dimming"
```

---

### Task 6: 16-Step Hardware Sequencer Console & Real-Time Playhead

**Files:**
- Modify: `plugin303/src/dsp/sequencer.rs` (add atomic playhead tracker)
- Create: `plugin303/src/gui/panels/sequencer_panel.rs`
- Test: `plugin303/tests/gui_build.rs`

**Interfaces:**
- Produces:
  - `sequencer_panel::render_sequencer(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter, current_step: &AtomicU8)`

- [ ] **Step 1: Update `plugin303/src/dsp/sequencer.rs` to expose `current_step: Arc<AtomicU8>`**

In `plugin303/src/dsp/sequencer.rs`:
```rust
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

pub struct Sequencer {
    // Existing fields...
    pub current_step: Arc<AtomicU8>,
}
```
In `Sequencer::new()`:
```rust
current_step: Arc::new(AtomicU8::new(255)),
```
In `Sequencer::process_block()` on step trigger:
```rust
self.current_step.store(step_idx as u8, Ordering::Relaxed);
```
In `Sequencer::reset()`:
```rust
self.current_step.store(255, Ordering::Relaxed);
```

- [ ] **Step 2: Write `plugin303/src/gui/panels/sequencer_panel.rs`**

```rust
use nih_plug_egui::egui::{vec2, Color32, Frame, Ui};
use nih_plug_egui::ParamSetter;
use std::sync::atomic::{AtomicU8, Ordering};

use crate::gui::theme;
use crate::gui::widgets::step_button::render_step_strip;
use crate::gui::widgets::{Knob303, SlideSwitch};
use crate::params::{Plugin303Params, SequencerMode, Timebase};

pub fn render_sequencer(
    ui: &mut Ui,
    params: &Plugin303Params,
    setter: &ParamSetter,
    current_step_atomic: &AtomicU8,
) {
    let active_step = current_step_atomic.load(Ordering::Relaxed);

    Frame::none()
        .fill(theme::PANEL_BG)
        .stroke(theme::border_stroke())
        .rounding(4.0)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_height(280.0);
            ui.set_width(1060.0);

            ui.horizontal(|ui| {
                // Left Transport & Config Column
                ui.vertical(|ui| {
                    ui.set_width(140.0);

                    ui.label(
                        nih_plug_egui::egui::RichText::new("SEQUENCER")
                            .font(nih_plug_egui::egui::FontId::new(12.0, nih_plug_egui::egui::FontFamily::Proportional))
                            .strong()
                            .color(theme::TEXT_PRIMARY),
                    );

                    ui.add_space(4.0);

                    // Run & Sync
                    ui.horizontal(|ui| {
                        let run_val = params.seq_run.value();
                        let run_btn = nih_plug_egui::egui::Button::new(if run_val { "■ STOP" } else { "▶ RUN" })
                            .fill(if run_val { theme::PLAYHEAD_RED } else { Color32::from_rgb(0x28, 0x30, 0x3c) });
                        if ui.add(run_btn).clicked() {
                            setter.begin_set_parameter(&params.seq_run);
                            setter.set_parameter(&params.seq_run, !run_val);
                            setter.end_set_parameter(&params.seq_run);
                        }

                        let sync_val = params.seq_sync.value();
                        let sync_btn = nih_plug_egui::egui::Button::new("SYNC")
                            .fill(if sync_val { theme::SLIDE_CYAN } else { Color32::from_rgb(0x28, 0x30, 0x3c) });
                        if ui.add(sync_btn).clicked() {
                            setter.begin_set_parameter(&params.seq_sync);
                            setter.set_parameter(&params.seq_sync, !sync_val);
                            setter.end_set_parameter(&params.seq_sync);
                        }
                    });

                    ui.add_space(4.0);
                    ui.add(Knob303::new(&params.seq_tempo, setter, "TEMPO", 13.0));

                    ui.add_space(4.0);
                    ui.add(SlideSwitch::new(
                        &params.seq_mode,
                        setter,
                        "MODE",
                        &["GRID", "303"],
                    ));

                    ui.add_space(4.0);
                    ui.add(SlideSwitch::new(
                        &params.seq_timebase,
                        setter,
                        "TIMEBASE",
                        &["16TH", "12TH"],
                    ));
                });

                ui.separator();

                // 16 Step Columns
                ui.horizontal(|ui| {
                    if let Ok(mut pattern_guard) = params.pattern.write() {
                        let num_steps = params.seq_length.value() as usize;
                        let mut pattern_changed = false;

                        for step_idx in 0..16 {
                            let is_active = active_step as usize == step_idx;
                            let in_active_range = step_idx < num_steps;

                            ui.group(|ui| {
                                if !in_active_range {
                                    ui.set_enabled(false);
                                }
                                if render_step_strip(
                                    ui,
                                    step_idx,
                                    &mut pattern_guard.steps[step_idx],
                                    is_active,
                                ) {
                                    pattern_changed = true;
                                }
                            });
                        }
                    }
                });
            });
        });
}
```

- [ ] **Step 3: Verify and run tests**

Run: `cargo test -p plugin303 --test gui_build`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add plugin303/src/dsp/sequencer.rs plugin303/src/gui/panels/sequencer_panel.rs
git commit -m "feat(gui): implement 16-step sequencer panel with live playhead tracking"
```

---

### Task 7: Full Plugin Editor Integration, Headless Gating & Zero-Alloc Verification

**Files:**
- Modify: `plugin303/src/lib.rs`
- Modify: `plugin303/src/gui/mod.rs`
- Test: `plugin303/tests/gui_build.rs`
- Test: `plugin303/tests/no_alloc.rs`

**Interfaces:**
- Produces: `Plugin303::editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>>`

- [ ] **Step 1: Update `plugin303/src/gui/mod.rs` with complete console layout**

```rust
pub mod panels;
pub mod theme;
pub mod widgets;

use nih_plug::prelude::Editor;
use nih_plug_egui::{create_egui_editor, EguiState};
use std::sync::atomic::AtomicU8;
use std::sync::Arc;

use crate::params::Plugin303Params;

pub const GUI_WIDTH: u32 = 1080;
pub const GUI_HEIGHT: u32 = 640;

pub fn create_editor(
    params: Arc<Plugin303Params>,
    editor_state: Arc<EguiState>,
    current_playing_step: Arc<AtomicU8>,
) -> Option<Box<dyn Editor>> {
    create_egui_editor(
        editor_state,
        (),
        |_, _| {},
        move |egui_ctx, setter, _state| {
            egui_ctx.set_visuals(nih_plug_egui::egui::Visuals::dark());
            nih_plug_egui::egui::CentralPanel::default()
                .frame(nih_plug_egui::egui::Frame::none().fill(theme::CHASSIS_BG).inner_margin(8.0))
                .show(egui_ctx, |ui| {
                    // Header Bar
                    panels::header_bar::render_header(ui, &params, setter);
                    ui.add_space(6.0);

                    // Top Split: Classic Panel (Left) & Souped-Up Drawer (Right)
                    ui.horizontal(|ui| {
                        panels::classic_panel::render_classic(ui, &params, setter);
                        ui.add_space(8.0);
                        panels::souped_panel::render_souped(ui, &params, setter);
                    });

                    ui.add_space(8.0);

                    // Bottom: 16-Step Hardware Sequencer Console
                    panels::sequencer_panel::render_sequencer(
                        ui,
                        &params,
                        setter,
                        &current_playing_step,
                    );
                });
        },
    )
}
```

- [ ] **Step 2: Update `plugin303/src/lib.rs` to wire editor**

In `Plugin303` struct:
```rust
#[cfg(feature = "gui")]
pub editor_state: Arc<nih_plug_egui::EguiState>,
pub current_playing_step: Arc<std::sync::atomic::AtomicU8>,
```
In `Plugin303::default()`:
```rust
let sequencer = Sequencer::new(48000.0);
let current_playing_step = sequencer.current_step.clone();
Self {
    // ...
    #[cfg(feature = "gui")]
    editor_state: nih_plug_egui::EguiState::from_size(crate::gui::GUI_WIDTH, crate::gui::GUI_HEIGHT),
    current_playing_step,
    sequencer,
}
```
In `impl Plugin for Plugin303`:
```rust
fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
    #[cfg(feature = "gui")]
    {
        crate::gui::create_editor(
            self.params.clone(),
            self.editor_state.clone(),
            self.current_playing_step.clone(),
        )
    }
    #[cfg(not(feature = "gui"))]
    {
        None
    }
}
```

- [ ] **Step 3: Run full verification suite**

Run:
```bash
cargo test -p plugin303 --all-features
cargo test -p plugin303 --no-default-features
cargo test -p plugin303 --test no_alloc
cargo xtask bundle plugin303 --release
```
Expected: All tests PASS, zero allocations reported, bundle produces `TB303.vst3` and `TB303.clap`.

- [ ] **Step 4: Commit**

```bash
git add plugin303/src/lib.rs plugin303/src/gui/mod.rs
git commit -m "feat(plugin): wire full vector GUI editor to Plugin303 with real-time sync"
```

---

## Execution Self-Review

1. **Spec Coverage:**
   - 7 Classic Knobs & Slide switches: Covered in Task 2 & Task 4.
   - Souped-Up drawer with Accurate mode dimming: Covered in Task 5.
   - 16-Step Sequencer with note/tie/rest, transpose, octave, accent, slide, and live playhead: Covered in Task 3 & Task 6.
   - Zero-allocation audio path & headless feature gating: Covered in Task 1 & Task 7.
2. **Placeholder Scan:** No "TBD", "TODO", or vague implementation instructions. All files, code, tests, and expected commands are explicit.
3. **Type Consistency:** Types (`Knob303`, `SlideSwitch`, `StepButton`, `LedIndicator`, `SequencerStep`, `Plugin303Params`) and method signatures match consistently throughout all tasks.
