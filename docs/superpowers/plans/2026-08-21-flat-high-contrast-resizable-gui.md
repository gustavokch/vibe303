# Flat, High-Contrast, Resizable GUI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Overhaul the `plugin303` graphical user interface into a modern, high-contrast, resizable vector interface with 1280x720 base canvas, flat design tokens (Deep Charcoal `#131315`, Safety Orange `#FF7A00`, Tech Cyan `#00F0FF`), anti-skeuomorphic widgets, 4-column Euclidean generator, and pattern shift rotation.

**Architecture:** The UI is authored in `nih_plug_egui` using purely procedural vector rendering (`egui::Painter` shapes and strokes) on a 1280×720 base canvas. Dynamic scaling is applied via `egui_ctx.set_zoom_factor()` tied to host window resizing and a manual zoom dropdown (75% to 200%). All skeuomorphic meshes (triangle-fan gradients, specular bevels, fake plastic knurling, bloom blur halos) are replaced with flat, high-contrast affordances.

**Tech Stack:** Rust 2021, `nih_plug` (vst3/clap), `nih_plug_egui` (`egui` 0.29), vector geometry.

**Spec:** `docs/superpowers/specs/2026-08-21-gui-flat-high-contrast-resizable-design.md`

## Global Constraints
- Base Canvas Dimensions: `1280px` width × `720px` height.
- Base Background Token: Deep Charcoal `CHASSIS_BG = #131315`.
- Primary Core Accent: Safety Orange `ACCENT_ORANGE = #FF7A00`.
- Secondary Accent: Tech Cyan `ACCENT_CYAN = #00F0FF`.
- Gold Accent: Tech Gold `ACCENT_GOLD = #FFC83B`.
- Slate Rest Token: Neutral Slate `ACCENT_SLATE = #2C2D36`.
- Ruby Warning Token: Signal Ruby `ACCENT_RUBY = #FF3B56`.
- Border Token: Crisp 1.0px hairline `BORDER_DEFAULT = #2B2B34`.
- Text Tokens: `TEXT_PRIMARY = #FFFFFF`, `TEXT_SECONDARY = #C4C4D0`, `TEXT_MUTED = #6E6E80`, `TEXT_ON_ACCENT = #0B0B0D`.
- Anti-Skeuomorphic Rule: No `gradient_circle`, `bevel_ring`, pseudo-bloom alpha halos, or faux 3D shadows.
- Zero audio-thread lock contention: GUI modifications acquire `params.pattern.write()` cleanly while audio thread reads cached snapshot.

---

### Task 1: Flat Design System Tokens & Color Palette

**Files:**
- Modify: `plugin303/src/gui/theme.rs`
- Test: `plugin303/tests/test_theme.rs`

**Interfaces:**
- Consumes: `nih_plug_egui::egui::{Color32, Stroke}`
- Produces: `CHASSIS_BG`, `PANEL_BG`, `WELL_BG`, `SURFACE_ELEVATED`, `SURFACE_HOVER`, `BORDER_DEFAULT`, `BORDER_SUBTLE`, `BORDER_FOCUS`, `BORDER_ACCENT`, `TEXT_PRIMARY`, `TEXT_SECONDARY`, `TEXT_MUTED`, `TEXT_ON_ACCENT`, `ACCENT_ORANGE`, `ACCENT_CYAN`, `ACCENT_GOLD`, `ACCENT_SLATE`, `ACCENT_RUBY`, `border_stroke()`, `highlight_stroke()`.

- [ ] **Step 1: Write the failing unit test for theme tokens**

Create `plugin303/tests/test_theme.rs`:
```rust
#[cfg(feature = "gui")]
#[test]
fn test_theme_color_tokens() {
    use nih_plug_egui::egui::Color32;
    use plugin303::gui::theme::*;

    assert_eq!(CHASSIS_BG, Color32::from_rgb(0x13, 0x13, 0x15));
    assert_eq!(PANEL_BG, Color32::from_rgb(0x1a, 0x1a, 0x1e));
    assert_eq!(WELL_BG, Color32::from_rgb(0x16, 0x16, 0x19));
    assert_eq!(ACCENT_ORANGE, Color32::from_rgb(0xff, 0x7a, 0x00));
    assert_eq!(ACCENT_CYAN, Color32::from_rgb(0x00, 0xf0, 0xff));
    assert_eq!(ACCENT_GOLD, Color32::from_rgb(0xff, 0xc8, 0x3b));
    assert_eq!(ACCENT_SLATE, Color32::from_rgb(0x2c, 0x2d, 0x36));
    assert_eq!(ACCENT_RUBY, Color32::from_rgb(0xff, 0x3b, 0x56));
    assert_eq!(TEXT_PRIMARY, Color32::from_rgb(0xff, 0xff, 0xff));
    assert_eq!(TEXT_SECONDARY, Color32::from_rgb(0xc4, 0xc4, 0xd0));
    assert_eq!(TEXT_MUTED, Color32::from_rgb(0x6e, 0x6e, 0x80));
    assert_eq!(TEXT_ON_ACCENT, Color32::from_rgb(0x0b, 0x0b, 0x0d));
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test --test test_theme`  
Expected: FAIL (missing or mismatched constants in `theme.rs`).

- [ ] **Step 3: Update `theme.rs` with new design tokens**

In `plugin303/src/gui/theme.rs`:
```rust
use nih_plug_egui::egui::{Color32, Stroke};

pub const CHASSIS_BG: Color32 = Color32::from_rgb(0x13, 0x13, 0x15);
pub const PANEL_BG: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1e);
pub const PANEL_BG_SOUPED: Color32 = Color32::from_rgb(0x18, 0x18, 0x1c);
pub const WELL_BG: Color32 = Color32::from_rgb(0x16, 0x16, 0x19);
pub const SURFACE_ELEVATED: Color32 = Color32::from_rgb(0x22, 0x22, 0x28);
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x33);

pub const BORDER_DEFAULT: Color32 = Color32::from_rgb(0x2b, 0x2b, 0x34);
pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(0x1e, 0x1e, 0x24);
pub const BORDER_FOCUS: Color32 = Color32::from_rgb(0x40, 0x40, 0x50);
pub const BORDER_ACCENT: Color32 = Color32::from_rgb(0xff, 0x7a, 0x00);

pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xff, 0xff, 0xff);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0xc4, 0xc4, 0xd0);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x6e, 0x6e, 0x80);
pub const TEXT_ON_ACCENT: Color32 = Color32::from_rgb(0x0b, 0x0b, 0x0d);

pub const ACCENT_ORANGE: Color32 = Color32::from_rgb(0xff, 0x7a, 0x00);
pub const ACCENT_CYAN: Color32 = Color32::from_rgb(0x00, 0xf0, 0xff);
pub const ACCENT_GOLD: Color32 = Color32::from_rgb(0xff, 0xc8, 0x3b);
pub const ACCENT_SLATE: Color32 = Color32::from_rgb(0x2c, 0x2d, 0x36);
pub const ACCENT_RUBY: Color32 = Color32::from_rgb(0xff, 0x3b, 0x56);

// Backward-compatibility aliases during transition
pub const PANEL_BORDER: Color32 = BORDER_DEFAULT;
pub const PANEL_BORDER_HIGHLIGHT: Color32 = BORDER_FOCUS;
pub const BANK_FRAME_BG: Color32 = WELL_BG;
pub const BANK_FRAME_BORDER: Color32 = BORDER_DEFAULT;
pub const BTN_ACTIVE_BG: Color32 = SURFACE_ELEVATED;
pub const BTN_INACTIVE_BG: Color32 = WELL_BG;
pub const ACCENT_AMBER: Color32 = ACCENT_ORANGE;
pub const ACCENT_AMBER_GLOW: Color32 = Color32::from_rgba_premultiplied(0xff, 0x7a, 0x00, 0x40);
pub const SLIDE_CYAN: Color32 = ACCENT_CYAN;
pub const SLIDE_CYAN_GLOW: Color32 = Color32::from_rgba_premultiplied(0x00, 0xf0, 0xff, 0x40);
pub const PLAYHEAD_RED: Color32 = ACCENT_ORANGE;
pub const PLAYHEAD_RED_GLOW: Color32 = Color32::from_rgba_premultiplied(0xff, 0x7a, 0x00, 0x40);
pub const SOUPED_ORANGE: Color32 = ACCENT_ORANGE;
pub const GATE_NOTE_COLOR: Color32 = ACCENT_CYAN;
pub const GATE_TIE_COLOR: Color32 = ACCENT_GOLD;
pub const GATE_REST_COLOR: Color32 = ACCENT_SLATE;

pub fn border_stroke() -> Stroke {
    Stroke::new(1.0_f32, BORDER_DEFAULT)
}

pub fn highlight_stroke() -> Stroke {
    Stroke::new(1.0_f32, BORDER_FOCUS)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_theme`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/gui/theme.rs plugin303/tests/test_theme.rs
git commit -m "feat(gui): implement flat high-contrast theme color tokens"
```

---

### Task 2: FlatKnob Widget with Monospace Value Badge

**Files:**
- Modify: `plugin303/src/gui/widgets/knob.rs`
- Test: `cargo test -p plugin303 --all-features`

**Interfaces:**
- Consumes: `Param`, `ParamSetter`, `theme::*`
- Produces: `Knob303::new(param, setter, label, radius)` rendering flat circular base, 270° arc, solid pointer needle, and monospace value pill badge.

- [ ] **Step 1: Replace skeuomorphic rendering in `knob.rs`**

Remove `gradient_circle` and `bevel_ring` functions. Implement `FlatKnob` rendering:
```rust
use nih_plug::prelude::{Param, ParamSetter};
use nih_plug_egui::egui::{
    pos2, vec2, CornerRadius, FontFamily, FontId, Pos2, Rect, Response, Sense, Stroke, Ui, Widget,
};
use std::f32::consts::PI;

use crate::gui::theme;

pub struct Knob303<'a, P: Param> {
    param: &'a P,
    setter: &'a ParamSetter<'a>,
    label: &'a str,
    radius: f32,
    accent_color: Option<nih_plug_egui::egui::Color32>,
}

impl<'a, P: Param> Knob303<'a, P> {
    pub fn new(param: &'a P, setter: &'a ParamSetter<'a>, label: &'a str, radius: f32) -> Self {
        Self {
            param,
            setter,
            label,
            radius,
            accent_color: None,
        }
    }

    pub fn with_accent(mut self, color: nih_plug_egui::egui::Color32) -> Self {
        self.accent_color = Some(color);
        self
    }
}

impl<'a, P: Param> Widget for Knob303<'a, P> {
    fn ui(self, ui: &mut Ui) -> Response {
        let desired_size = vec2(self.radius * 2.0 + 16.0, self.radius * 2.0 + 38.0);
        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());

        let center = pos2(rect.center().x, rect.top() + self.radius + 6.0);
        let normalized = self.param.unmodulated_normalized_value();

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
        if response.drag_stopped() {
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
            let is_hovered = response.hovered();

            // 1. Background disc (flat well)
            painter.circle_filled(center, self.radius, theme::WELL_BG);
            painter.circle_stroke(
                center,
                self.radius,
                Stroke::new(
                    1.0_f32,
                    if is_hovered {
                        theme::BORDER_FOCUS
                    } else {
                        theme::BORDER_DEFAULT
                    },
                ),
            );

            // 2. 270° Perimeter Track (135° to 405°)
            let start_angle = 135.0 * PI / 180.0;
            let end_angle = 405.0 * PI / 180.0;
            let arc_r = self.radius - 2.5;

            // Background inactive arc
            let bg_arc_points: Vec<_> = (0..=24)
                .map(|i| {
                    let t = i as f32 / 24.0;
                    let a = start_angle + t * (end_angle - start_angle);
                    pos2(center.x + a.cos() * arc_r, center.y + a.sin() * arc_r)
                })
                .collect();
            painter.add(nih_plug_egui::egui::Shape::line(
                bg_arc_points,
                Stroke::new(2.0_f32, theme::SURFACE_ELEVATED),
            ));

            // Active value arc
            let accent = self.accent_color.unwrap_or(theme::ACCENT_ORANGE);
            if normalized > 0.001 {
                let active_arc_points: Vec<_> = (0..=24)
                    .map(|i| {
                        let t = i as f32 / 24.0;
                        let a = start_angle + t * normalized * (end_angle - start_angle);
                        pos2(center.x + a.cos() * arc_r, center.y + a.sin() * arc_r)
                    })
                    .collect();
                painter.add(nih_plug_egui::egui::Shape::line(
                    active_arc_points,
                    Stroke::new(2.5_f32, accent),
                ));
            }

            // 3. Pointer Needle
            let current_angle = start_angle + normalized * (end_angle - start_angle);
            let needle_inner = pos2(
                center.x + current_angle.cos() * (self.radius * 0.2),
                center.y + current_angle.sin() * (self.radius * 0.2),
            );
            let needle_outer = pos2(
                center.x + current_angle.cos() * (self.radius * 0.75),
                center.y + current_angle.sin() * (self.radius * 0.75),
            );
            painter.line_segment([needle_inner, needle_outer], Stroke::new(1.5_f32, theme::TEXT_PRIMARY));

            // 4. Parameter Label
            let label_y = center.y + self.radius + 6.0;
            painter.text(
                pos2(rect.center().x, label_y),
                nih_plug_egui::egui::Align2::CENTER_CENTER,
                self.label.to_uppercase(),
                FontId::new(8.5, FontFamily::Proportional),
                theme::TEXT_SECONDARY,
            );

            // 5. Monospace Value Badge
            let value_y = label_y + 11.0;
            let value_str = self.param.to_string();
            let pill_rect = Rect::from_center_size(pos2(rect.center().x, value_y), vec2(desired_size.x - 4.0, 11.0));
            painter.rect_filled(pill_rect, CornerRadius::same(2), theme::WELL_BG);
            painter.rect_stroke(pill_rect, CornerRadius::same(2), Stroke::new(1.0_f32, theme::BORDER_SUBTLE), nih_plug_egui::egui::StrokeKind::Inside);
            painter.text(
                pos2(rect.center().x, value_y),
                nih_plug_egui::egui::Align2::CENTER_CENTER,
                value_str,
                FontId::new(8.0, FontFamily::Monospace),
                theme::TEXT_PRIMARY,
            );
        }

        response
    }
}
```

- [ ] **Step 2: Run compiler check & tests**

Run: `cargo test -p plugin303 --all-features`  
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add plugin303/src/gui/widgets/knob.rs
git commit -m "feat(gui): convert Knob303 to flat non-skeuomorphic vector dial with value pill"
```

---

### Task 3: FlatSwitch and FlatFader Widgets

**Files:**
- Modify: `plugin303/src/gui/widgets/switch.rs`
- Modify: `plugin303/src/gui/widgets/fader.rs`
- Test: `cargo test -p plugin303 --all-features`

**Interfaces:**
- Consumes: `EnumParam`, `FloatParam`, `ParamSetter`, `theme::*`
- Produces: `SlideSwitch` and `MiniFader` styled as flat segmented selectors and flat level faders without faux metal ribs or highlights.

- [ ] **Step 1: Simplify `switch.rs` to clean segmented tab layout**

In `plugin303/src/gui/widgets/switch.rs`:
- Remove knurled metal lines and 3D shadows.
- Render recessed container in `WELL_BG` with `BORDER_DEFAULT`.
- Render active selected detent as flat pill in `SURFACE_ELEVATED` with solid 2.0px indicator bar in `ACCENT_ORANGE` / `ACCENT_CYAN`.

- [ ] **Step 2: Simplify `fader.rs` to clean flat vertical bar**

In `plugin303/src/gui/widgets/fader.rs`:
- Remove bevels and drop-shadows.
- Slot width: 6.0px in `WELL_BG`.
- Level fill: Solid `ACCENT_ORANGE` vertical bar.
- Thumb: Flat 2.0px horizontal line.

- [ ] **Step 3: Run tests**

Run: `cargo test -p plugin303 --all-features`  
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add plugin303/src/gui/widgets/switch.rs plugin303/src/gui/widgets/fader.rs
git commit -m "feat(gui): simplify switch and fader to flat modern studio widgets"
```

---

### Task 4: StepCard Widget (56px Wide Sequencer Column)

**Files:**
- Modify: `plugin303/src/gui/widgets/step_button.rs`
- Test: `cargo test -p plugin303 --all-features`

**Interfaces:**
- Consumes: `SequencerStep`, `StepGate`, `theme::*`
- Produces: `render_step_strip(ui, step_idx, step, is_current, is_running)` at 56px width with flat playhead LED, modifier tags, and gate pills.

- [ ] **Step 1: Update `render_step_strip` for 56px flat card**

In `plugin303/src/gui/widgets/step_button.rs`:
- Set `width = 56.0`.
- Outer card: `PANEL_BG` with 1.0px `BORDER_DEFAULT` (active playhead step highlights border in `ACCENT_ORANGE`).
- Playhead LED: 4.0px flat bar. Unlit = `SURFACE_ELEVATED` (`#222228`), Active = solid `ACCENT_ORANGE` (`#FF7A00`) without bloom expansion.
- `ACC` toggle: Solid `ACCENT_ORANGE` with `TEXT_ON_ACCENT` when active; `WELL_BG` with `TEXT_MUTED` when off.
- `SLD` toggle: Solid `ACCENT_CYAN` with `TEXT_ON_ACCENT` when active; `WELL_BG` with `TEXT_MUTED` when off.
- Gate Pill:
  - `NOTE` = Solid `ACCENT_CYAN` (`#00F0FF`) with `TEXT_ON_ACCENT`.
  - `TIE` = Solid `ACCENT_GOLD` (`#FFC83B`) with `TEXT_ON_ACCENT`.
  - `REST` = Deep `WELL_BG` (`#161619`) with `TEXT_MUTED`.
- Note pitch & octave nudges: Clean monospace note label (e.g. `C 3`), clicking triggers `PianoRollPopup`.

- [ ] **Step 2: Run tests**

Run: `cargo test -p plugin303 --all-features`  
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add plugin303/src/gui/widgets/step_button.rs
git commit -m "feat(gui): update step strip to 56px wide flat step card"
```

---

### Task 5: 1280×720 Canvas Architecture & Header Bar Zoom Presets

**Files:**
- Modify: `plugin303/src/gui/mod.rs`
- Modify: `plugin303/src/gui/panels/header_bar.rs`
- Test: `cargo test -p plugin303 --all-features`

**Interfaces:**
- Consumes: `GUI_WIDTH = 1280`, `GUI_HEIGHT = 720`, `EguiState`
- Produces: Proportional canvas zoom scaling and Header Bar zoom preset selector.

- [ ] **Step 1: Update canvas constants and editor creation in `gui/mod.rs`**

In `plugin303/src/gui/mod.rs`:
- Update `GUI_WIDTH = 1280` and `GUI_HEIGHT = 720`.
- Apply master frame fill `theme::CHASSIS_BG` with margin `10.0px`.

- [ ] **Step 2: Update `header_bar.rs` with Flat Status Line & Zoom Dropdown**

In `plugin303/src/gui/panels/header_bar.rs`:
- Render flat vector logotype `TECHNEO 303` with `ACCENT_ORANGE` sub-badge.
- Circuit topology selector (`ACCURATE` / `FUN`) with flat segmented switch.
- Fidelity selector (`LOW` / `HIGH` / `ULTRA`).
- Right-aligned Zoom Preset dropdown: `[ 75% | 100% | 125% | 150% | 200% ]`.

- [ ] **Step 3: Run compiler & tests**

Run: `cargo test -p plugin303 --all-features`  
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add plugin303/src/gui/mod.rs plugin303/src/gui/panels/header_bar.rs
git commit -m "feat(gui): implement 1280x720 base canvas and header zoom menu"
```

---

### Task 6: Classic Panel & Souped-Up Drawer Refactoring

**Files:**
- Modify: `plugin303/src/gui/panels/top_controls.rs`
- Modify: `plugin303/src/gui/panels/souped_panel.rs`
- Test: `cargo test -p plugin303 --all-features`

**Interfaces:**
- Consumes: `Plugin303Params`, `ParamSetter`, `Knob303`, `SlideSwitch`, `MiniFader`
- Produces: Wide balanced top controls (Waveform + 7 vintage dials + ADSR banks) and flat dimmed Accurate mode drawer.

- [ ] **Step 1: Rebalance `top_controls.rs` across 1280px width**

In `plugin303/src/gui/panels/top_controls.rs`:
- Frame height: `144.0px`.
- Left: Vertical waveform switch (`SAW`, `SQR`, `SWP`).
- Center: 7 vintage dials (Tuning, Cutoff, Resonance, Env Mod, Decay, Accent, Volume) with `20.0px` dial radius.
- Right: MEG ADSR and VEG ADSR fader banks with `WELL_BG` frames and `BORDER_DEFAULT` strokes.

- [ ] **Step 2: Update `souped_panel.rs` with clean flat dimming**

In `plugin303/src/gui/panels/souped_panel.rs`:
- Frame height: `116.0px`.
- Accurate mode: Render drawer at uniform 35% opacity with non-interactive overlay badge: `ACCURATE MODE ACTIVE` / `Souped-up engine is bypassed` with `[ENABLE FUN MODE]` button.
- Fun mode: Full vibrancy and interactivity.

- [ ] **Step 3: Run tests**

Run: `cargo test -p plugin303 --all-features`  
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add plugin303/src/gui/panels/top_controls.rs plugin303/src/gui/panels/souped_panel.rs
git commit -m "feat(gui): rebalance classic controls and flat souped drawer for 1280px canvas"
```

---

### Task 7: Output Clipper and Tweaks Consolidated Subpanel

**Files:**
- Modify: `plugin303/src/gui/panels/output_clipper.rs`
- Modify: `plugin303/src/gui/panels/tweaks_panel.rs`
- Test: `cargo test -p plugin303 --all-features`

**Interfaces:**
- Consumes: `Plugin303Params`, `ParamSetter`, `Knob303`, `SlideSwitch`
- Produces: Full-width horizontal row pairing 10 clipper controls with 5 analog component tweak dials.

- [ ] **Step 1: Update `output_clipper.rs` and `tweaks_panel.rs` layout**

- Height: `108.0px`.
- Output Clipper: `BYPASS` flat toggle, `OS` switch (`2X` / `4X`), `DRIVE`, `THRESH`, `BASS`, `TONE`, `IN`, `OUT`, `TILT`, `DYN`, `MIX`.
- Analog Tweaks: `TOLERANCE`, `AGE`, `NOISE`, `SEED`, `DRIFT` with flat dials and monospace readouts.

- [ ] **Step 2: Run tests**

Run: `cargo test -p plugin303 --all-features`  
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add plugin303/src/gui/panels/output_clipper.rs plugin303/src/gui/panels/tweaks_panel.rs
git commit -m "feat(gui): align output clipper and tweaks panels on 1280px horizontal grid"
```

---

### Task 8: Acid Euclidean Generator 4-Column Flat Subpanel

**Files:**
- Modify: `plugin303/src/gui/panels/generator_panel.rs`
- Test: `cargo test -p plugin303 --all-features`

**Interfaces:**
- Consumes: `GeneratorParams`, `MusicalScale`, `AcidGenerator`, `Plugin303Params`
- Produces: 4-column structured parameter grid: Pattern Structure, Octave Behavior, Event Probabilities, and Variation Actions (without icons/emojis).

- [ ] **Step 1: Author 4-column grid layout in `generator_panel.rs`**

In `plugin303/src/gui/panels/generator_panel.rs`:
- Panel height: `110.0px`, `PANEL_BG` with `BORDER_DEFAULT`.
- Header: Title `ACID EUCLIDEAN GENERATOR` with top-right read-only `SEED: {:016X}` monospace pill and `[NEW SEED]` flat button.
- Column 1 (Pattern Structure): `ROOT` dropdown, `SCALE` dropdown, `PULSES (k)` (1..16), `STEPS (n)` (1..16), `SHIFT` (0..15), `PITCH LEN` (1..16).
- Column 2 (Octave Behavior): `OCT MIN` (-2..0), `OCT MAX` (0..+2), `OCT UP %` (0..100%), `OCT DN %` (0..100%).
- Column 3 (Event Probabilities): `REST %`, `ACCENT %`, `SLIDE %`, `ACC PITCH BIAS %`, `[x] SLD CHAIN` checkbox.
- Column 4 (Actions & Variation):
  - `MUTATION %` DragValue.
  - `[GENERATE]` solid `ACCENT_ORANGE` button with dark text.
  - `[MUTATE]` solid `ACCENT_CYAN` button with dark text.

- [ ] **Step 2: Run tests**

Run: `cargo test -p plugin303 --all-features`  
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add plugin303/src/gui/panels/generator_panel.rs
git commit -m "feat(gui): rebuild Acid Euclidean Generator as 4-column flat technical grid"
```

---

### Task 9: Sequencer Transport & Pattern Shift (`< SHIFT` and `> SHIFT`)

**Files:**
- Modify: `plugin303/src/gui/panels/sequencer_panel.rs`
- Test: `plugin303/tests/test_pattern_shift.rs`

**Interfaces:**
- Consumes: `params.pattern`, `params.seq_run`, `params.seq_sync`
- Produces: 16-step grid (`16 × 56px = 896px`) + Transport column with `< SHIFT` and `> SHIFT` cyclic rotation buttons.

- [ ] **Step 1: Write unit test for pattern shift logic**

Create `plugin303/tests/test_pattern_shift.rs`:
```rust
use plugin303::params::{PatternData, SequencerStep, StepGate};

fn shift_pattern_left(pattern: &mut PatternData) {
    pattern.steps.rotate_left(1);
}

fn shift_pattern_right(pattern: &mut PatternData) {
    pattern.steps.rotate_right(1);
}

#[test]
fn test_pattern_cyclic_shift() {
    let mut pattern = PatternData::default();
    pattern.steps[0].note = 12;
    pattern.steps[1].note = 7;

    shift_pattern_left(&mut pattern);
    assert_eq!(pattern.steps[15].note, 12);
    assert_eq!(pattern.steps[0].note, 7);

    shift_pattern_right(&mut pattern);
    assert_eq!(pattern.steps[0].note, 12);
    assert_eq!(pattern.steps[1].note, 7);
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cargo test --test test_pattern_shift`  
Expected: PASS.

- [ ] **Step 3: Implement Shift Buttons and Transport in `sequencer_panel.rs`**

In `plugin303/src/gui/panels/sequencer_panel.rs`:
- Left: 16 step cards in horizontal row (`56.0px` card width).
- Right: Transport subpanel with:
  - `RUN / STOP` block button.
  - `SYNC: HOST / FREE` mode toggle.
  - `< SHIFT` and `> SHIFT` cyclic rotate buttons calling `pattern_guard.steps.rotate_left(1)` and `pattern_guard.steps.rotate_right(1)`.
  - `RANDOM PATTERN` and `CLEAR ALL` action buttons.

- [ ] **Step 4: Run all test suites**

Run: `cargo test -p plugin303 --all-features`  
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/gui/panels/sequencer_panel.rs plugin303/tests/test_pattern_shift.rs
git commit -m "feat(gui): implement pattern shift buttons and wide sequencer transport"
```

---

### Task 10: End-to-End Build & Visual QA Snapshot Verification

**Files:**
- Modify: `scripts/capture_standalone_gui.sh`
- Test: Full cargo test matrix & snapshot capture

**Interfaces:**
- Consumes: Standalone binary, `nih_plug_egui`
- Produces: Clean 1280x720 PNG screenshots verifying flat tokens, 56px step cards, Euclidean generator, and accurate-mode dimming.

- [ ] **Step 1: Update snapshot capture script for 1280×720 canvas**

In `scripts/capture_standalone_gui.sh`:
- Target resolution: `1280x720`.
- Capture `accurate.png`, `fun.png`, and `running.png`.

- [ ] **Step 2: Run full build and test matrix**

Run: `cargo test -p plugin303 --no-default-features` (Headless DSP verification)  
Run: `cargo test -p plugin303 --all-features` (GUI full test suite)  
Expected: All tests pass with zero warnings.

- [ ] **Step 3: Commit**

```bash
git add scripts/capture_standalone_gui.sh
git commit -m "chore(gui): update visual QA capture script for 1280x720 flat canvas"
```
