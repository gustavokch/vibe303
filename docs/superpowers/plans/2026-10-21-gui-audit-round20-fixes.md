# GUI Audit Round 20 Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix every finding of the round-20 GUI audit (2 HIGH, 4 MEDIUM, 6 LOW, 1 flagged defect) in the TECHNEO 303 editor, and keep the design contract in step with each change.

**Architecture:** The editor is `nih_plug_egui` (egui 0.31.1) in `plugin303/src/gui/`. Disabled controls stop using egui's fade: they paint theme tokens through custom widgets (a new `Readout`, the existing `ToggleButton`, `Knob303`, `MiniFader`, `SlideSwitch`). Booleans become raised tiles with an LED. Knob readouts come from the parameter's own `value_to_string`, or from DSP mapping functions shared with `dsp/voice.rs` where the readout depends on another parameter. The ZOOM control and its QA state are removed.

**Tech Stack:** Rust 2021, nih-plug `f36931f`, `nih_plug_egui`, egui 0.31.1; Python 3.12 QA tools (`scripts/gui_qa_measure.py`, `scripts/crop_gui_sections.py`); offscreen renderer `tools/gui_render` (egui_kittest + wgpu).

**Spec:** `docs/superpowers/plans/2026-10-21-gui-audit-round20-report.md` (the findings, issue numbers 1-12 and the "Flagged" list) and `.claude/skills/gui-visual-audit/references/design-contract.md` (the rules each fix must follow or change). Read both before Task 1.

## Global Constraints

- Issue numbers are the report's numbers (#1-#12). Never renumber them in commits or comments.
- Design direction (contract): no outlines, no shadows, no gradients; every widget state has `bg_stroke = NONE`.
- `ACCENT_ORANGE` means live state only. A disabled control never shows orange.
- Disabled text ≥ 3:1; enabled text ≥ 4.5:1; non-text marks ≥ 3:1 (contract, Contrast).
- A disabled control shows itself through tokens, never through egui's fade (contract, Dimming).
- Window stays 1216 × 640 pt (19:10 const assert in `gui/mod.rs`); `plugin303/tests/gui_layout_test.rs` must stay green: no overflow, no dead band.
- When a task changes a rule, update `design-contract.md` in the same commit.
- Audio path untouched: DSP edits in Task 7 are pure function extractions, bit-exact.
- Rust tests run in release mode: `cargo test -p plugin303 --release …`.
- Python: ruff, line length 100. Run Python tests with `python3 -m pytest … -q`.
- Commit messages: standard English, conventional prefix (`fix(gui):`, `refactor(gui):`, `test(gui):`, `docs(gui-qa):`).

## Decisions taken with the user (2026-10-21)

| Finding | Decision |
|---|---|
| #2 ZOOM 75% dead band | Remove the ZOOM control. `EguiState::set_requested_size` is private in nih-plug `f36931f` and in upstream master `de42101`, so the window cannot follow the zoom without a fork. |
| #4 static orange, #5 toggle vs action, flagged "STOP lit while running" | LED-slot toggles: every boolean is a raised tile with a small LED left of a state-free label; the LED is orange when on; the tile never fills orange. GENERATE becomes a raised action. RUN/STOP becomes a RUN toggle. |
| #6 playhead | Keep the `PANEL_HEADER_BG` plate, draw it only while the sequencer runs; LED 4 pt tall. (A plate at ≥ 1.3:1 on `PANEL_BG` needs ≥ `#393939`, which is `SURFACE_ELEVATED` itself.) |
| #9 CLEAR | Two-click confirm: the first click arms the tile (label `CONFIRM CLEAR`) for 3 s; a second click clears. |

Decided by the plan, not asked:

- **Flagged: knob value text.** Confirmed a defect. `knob.rs:194-250` picks a format by label substring. CUTOFF shows `300·50^f` Hz while the voice uses `314·(2394/314)^f` Hz (classic) or `200·20^f` Hz (ADV ENV) (`voice.rs:959, 1584, 1605`). DECAY shows `200 + 1800·f` ms while the voice uses the `decay_map` table (100 ms … 2.1 s). The SOUPED-UP DRIVE knob (`vcf_drive`, 0-24 dB) prints `12%` at 12 dB. Task 7 fixes it.
- **#10 extended.** `voice.rs:916-941` reads the VEG ADSR faders only when ADV ENV is on, and `voice.rs:813-833` forces ADV ENV off in ACCURATE. Today the VEG bank stays enabled without ADV ENV, and the ADV ENV tile stays live in ACCURATE. Task 3 dims both banks and the tile accordingly, so "what ADV ENV enables" is shown truthfully.
- **CALIBRATE in ACCURATE.** It renders as a faded egui `Button` (`#898989`, off-token). Task 2 hides it in ACCURATE, as SOUPED-UP already hides its header switches.
- **Not acted on:** flagged "TRANSPORT labels 6.2-6.7:1" (glyph-core measurement, not drift); flagged "zoom75 greys" (the state goes away in Task 1).

## File structure

| File | Responsibility | Tasks |
|---|---|---|
| `plugin303/src/gui/panels/header_bar.rs` | app bar | 1, 3 |
| `plugin303/src/gui/mod.rs` | layout, editor creation, QA helpers | 1 |
| `plugin303/src/params.rs` | params, `QaState` | 1, 7 |
| `plugin303/src/gui/widgets/readout.rs` (new) | read-only value well painted with tokens | 2 |
| `plugin303/src/gui/widgets/mod.rs` | widget exports | 2 |
| `plugin303/src/gui/theme.rs` | tokens; contrast helpers become public | 2 |
| `plugin303/src/gui/panels/tweaks_panel.rs` | ANALOG TWEAKS | 2, 3, 8 |
| `plugin303/src/gui/panels/generator_panel.rs` | ACID generator, GENERATION | 2, 4, 8, 9 |
| `plugin303/src/gui/panels/top_controls.rs` | VOICE, ENVELOPES | 3, 7 |
| `plugin303/src/gui/widgets/toggle.rs` | `ToggleButton` | 4 |
| `plugin303/src/gui/panels/sequencer_panel.rs` | SEQUENCER, TRANSPORT | 4, 6 |
| `plugin303/src/gui/widgets/step_button.rs` | step column, playhead | 5 |
| `plugin303/src/value_text.rs` (new) | value display text shared by params and knobs | 7 |
| `plugin303/src/dsp/voice.rs` | public mapping fns (`decay_ms`, cutoff, glide) | 7 |
| `plugin303/src/gui/widgets/knob.rs` | `Knob303` value text, tooltip | 7, 8 |
| `plugin303/src/gui/panels/souped_panel.rs` | SOUPED-UP ENGINE | 7, 8, 9 |
| `plugin303/src/gui/panels/output_clipper.rs` | OUTPUT CLIPPER | 8 |
| `plugin303/tests/gui_common/mod.rs` (new) | test rig: frames, text colours, hover, env guard | 2 |
| `plugin303/tests/gui_text_contrast.rs` (new) | every text colour is a token | 2 |
| `plugin303/tests/gui_tooltips.rs` (new) | disabled hints, control tooltips | 3, 8 |
| `plugin303/tests/param_text.rs` (new) | host/knob text matches the voice | 7 |
| `plugin303/tests/gui_layout_test.rs`, `gui_build.rs` | existing | 1, 2 |
| `tools/gui_render/src/main.rs` | offscreen renderer | 1 |
| `scripts/capture_standalone_gui.sh`, `scripts/crop_gui_sections.py`, `scripts/gui_qa_measure.py` | QA tools | 1 |
| `tests/test_crop_gui_sections.py`, `tests/test_gui_qa_measure.py` | QA tool tests | 1 |
| `.claude/skills/gui-visual-audit/SKILL.md`, `references/*.md` | audit skill and contract | 1-9 |

---

### Task 0: Branch and baseline

**Files:** none changed; commits the uncommitted round-20 audit work.

- [ ] **Step 1: Create the branch**

```bash
git switch -c fix/gui-audit-round20
```

- [ ] **Step 2: Commit the round-20 audit tool changes and report**

`git status --short` shows `M scripts/gui_qa_measure.py`, `M tests/test_gui_qa_measure.py`, `M .claude/skills/gui-visual-audit/references/measurement-recipes.md` and the untracked report. Commit exactly these four paths:

```bash
git add scripts/gui_qa_measure.py tests/test_gui_qa_measure.py \
  .claude/skills/gui-visual-audit/references/measurement-recipes.md \
  docs/superpowers/plans/2026-10-21-gui-audit-round20-report.md \
  docs/superpowers/plans/2026-10-21-gui-audit-round20-fixes.md
git commit -m "docs(gui-qa): round-20 audit report, fix plan and colors floor for zoom75"
```

- [ ] **Step 3: Record the baseline**

Run: `cargo test -p plugin303 --release 2>&1 | grep "test result"` and `python3 -m pytest tests/test_gui_qa_measure.py tests/test_crop_gui_sections.py -q`
Expected: all pass. Note the pass counts; later tasks compare against them.

---

### Task 1: Remove ZOOM (#2)

**Files:**
- Modify: `plugin303/src/gui/panels/header_bar.rs:1-21, 62-93`
- Modify: `plugin303/src/gui/mod.rs:121-124, 167-177`
- Modify: `plugin303/src/params.rs:414-450`
- Modify: `plugin303/tests/gui_build.rs:71-85, 187-215`
- Modify: `tools/gui_render/src/main.rs:23-41, 65-99`
- Modify: `scripts/capture_standalone_gui.sh:130-131, 145`
- Modify: `scripts/crop_gui_sections.py:116-137`
- Modify: `tests/test_crop_gui_sections.py:33-71`
- Modify: `scripts/gui_qa_measure.py:120-138, 233-234, 592-594, 899-901, 1129-1134, 1157`
- Modify: `tests/test_gui_qa_measure.py:1-14, 162-192`
- Modify: `.claude/skills/gui-visual-audit/SKILL.md`, `references/measurement-recipes.md`, `references/audit-checklist.md`, `references/fix-prompt-template.md`, `references/design-contract.md`

**Interfaces:**
- Consumes: nothing.
- Produces: `QaState` without `Zoom75`; `gui_render` renders 8 states; `gui_qa_measure.COLORS_MIN_PIXELS` is the only `colors` floor (no `colors_min_pixels`, no `ZOOM_SCALED_STATES`).

- [ ] **Step 1: Delete the ZOOM control from the app bar**

In `plugin303/src/gui/panels/header_bar.rs`:

Replace the import block (lines 1-5) with:

```rust
use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{
    pos2, vec2, Align, Align2, FontFamily, FontId, Layout, Rect, Sense, Ui, UiBuilder,
};
```

Delete line 12 (`const ZOOM_PRESETS …`), the `caption` fn (lines 17-21; its only caller is the ZOOM label), and the whole `// Zoom factor dropdown (rightmost).` block (lines 62-93, from `let current_zoom` through the closing `});` of `ui.scope`). QUALITY is now the rightmost control.

- [ ] **Step 2: Delete the zoom QA path**

In `plugin303/src/gui/mod.rs` delete `qa_zoom_factor` (lines 121-124), the `let qa_zoom = …` line and its comment (lines 167-169), and in the build closure delete:

```rust
            if let Some(zoom) = qa_zoom {
                egui_ctx.set_zoom_factor(zoom);
            }
```

Keep the `qa_state` read: `qa_generator_params(qa_state.as_deref())` still uses it.

In `plugin303/src/params.rs` delete the `Zoom75,` variant (line 431) and the `"zoom75" => Self::Zoom75,` arm (line 447).

- [ ] **Step 3: Update the Rust tests that named zoom75**

In `plugin303/tests/gui_build.rs`:
- Delete the line `        Some("zoom75"),` (line 76).
- Rename `test_qa_generator_and_zoom_state_editor_build` to `test_qa_generator_state_editor_build`, and delete lines 207-214 (`drop(_guard_gen);` through the zoom `assert!`). Change `create_editor(params.clone(), state.clone(), current_step.clone())` to `create_editor(params, state, current_step)`.

- [ ] **Step 4: Update the offscreen renderer**

In `tools/gui_render/src/main.rs`:

```rust
use plugin303::gui::{qa_generator_params, render_editor, theme, GUI_HEIGHT, GUI_WIDTH, MARGIN};
```

```rust
const STATES: [(&str, &str); 8] = [
    ("default", "latest"),
    ("accurate", "accurate"),
    ("advanced", "advanced"),
    ("running", "running"),
    ("running_note", "running_note"),
    ("knobs", "knobs"),
    ("quality_low", "quality_low"),
    ("generator", "generator"),
];
```

In `render`, delete `let zoom = qa_zoom_factor(Some(state));`, the four comment lines above `let zoom = zoom.unwrap_or(1.0);`, and that line. Replace the harness builder start with:

```rust
    let mut first_frame = true;
    let mut harness = Harness::builder()
        .with_size(vec2(GUI_WIDTH as f32, GUI_HEIGHT as f32))
        .with_pixels_per_point(PIXELS_PER_POINT)
```

- [ ] **Step 5: Update the capture and crop scripts**

`scripts/capture_standalone_gui.sh`: delete lines 130-131 (`# Scaling / Zoom state …` and `capture_single_state "zoom75" …`) and the `"zoom75.png"` entry of `REQUIRED_CROPS`.

`scripts/crop_gui_sections.py`: delete the comment block and `ZOOM_SCALED_STATES = {"zoom75"}` (lines 116-120), the `"zoom75",` list entry, and the two lines `if stem in ZOOM_SCALED_STATES:` / `continue`.

`tests/test_crop_gui_sections.py`: delete `"zoom75",` from the stem list in `test_generate_all_crops`, and delete the closing block `# Verify zoom-scaled states do not produce per-section crops` with its `for` loop (lines 65-71).

- [ ] **Step 6: Update `gui_qa_measure.py`**

- Delete `ZOOM_SCALED_STATES` with its comment (lines 120-123) and the whole `colors_min_pixels` function (lines 129-138). Keep `COLORS_MIN_PIXELS = 150` and its comment.
- In `report`, replace `min_pixels=colors_min_pixels(path),` with `min_pixels=COLORS_MIN_PIXELS,`.
- In `report`, delete the two lines `scaled = …` and `note = …`, change the print to `print(f"  {os.path.basename(path):20s} {w}x{h}{flag}")`, and replace `comparable = [i for i in images if stem(i) not in ZOOM_SCALED_STATES]` with `comparable = images`. Delete the `stem` helper if nothing else in `report` calls it (check with `grep -n "stem(" scripts/gui_qa_measure.py`).
- Line 234: replace the comment with `# Below min_size a region is a gutter or a control, not a card.`
- `knob_scale` docstring: `"""Pixels per point: capture width over GUI_WIDTH, unless --scale overrides it."""`
- Line 901: `print("  Compare one group with --box instead.")`

- [ ] **Step 7: Update `tests/test_gui_qa_measure.py`**

Delete `colors_report`, `foreground_of` and `class TestColors` (lines 162-192). Then run `ruff check tests/test_gui_qa_measure.py` and delete every import it reports as unused (F401; expected: `argparse`, `contextlib`, `io`, `json`, `tempfile` unless another test uses them).

- [ ] **Step 8: Update the audit skill and contract**

- `SKILL.md`: delete the `zoom75.png` row of the capture table (line 89); delete the two sentences starting "In the offscreen `zoom75` the ZOOM combo reads `100%`" (lines 92-95; keep the `quality_high`/`quality_best` sentence); delete ", the window under ZOOM" from line 67; delete the common-mistakes row "Diffing or accent-tabling `zoom75` …" (line 199).
- `references/measurement-recipes.md`: delete "`zoom75` draws at 1.5 px/pt." (line 9); cut lines 28-30 down to "then the accent table."; delete the paragraph at lines 79-80; delete the paragraph at lines 134-137 (the floor no longer scales); delete the bullet at line 203; delete "It skips `zoom75`." (line 264).
- `references/audit-checklist.md`: delete the bullet "Content that only fits at 100% zoom." (line 22).
- `references/fix-prompt-template.md` line 93: "(title bar, GL text)".
- `references/design-contract.md` line 151: "(TOPOLOGY, QUALITY)".

- [ ] **Step 9: Verify nothing names zoom any more**

Run: `grep -rniE "zoom" plugin303/src plugin303/tests tools/gui_render/src scripts tests .claude/skills/gui-visual-audit`
Expected: no output.

- [ ] **Step 10: Build, test, render**

```bash
cargo test -p plugin303 --release --test gui_build --test gui_layout_test
cargo build --release -p gui_render
rm -f target/gui-qa-offscreen/zoom75*.png
bash scripts/render_gui_offscreen.sh
python3 -m pytest tests/test_gui_qa_measure.py tests/test_crop_gui_sections.py -q
ruff check scripts/ tests/
```

Expected: all green; `ls target/gui-qa-offscreen/*.png` lists 8 state PNGs plus crops and no `zoom75`. Read `target/gui-qa-offscreen/latest.png`: the app bar ends with QUALITY flush right.

- [ ] **Step 11: Commit**

```bash
git add -A plugin303 tools/gui_render scripts tests .claude/skills/gui-visual-audit
git commit -m "fix(gui): remove the ZOOM control (#2)

The window cannot follow egui's zoom: nih_plug_egui keeps
set_requested_size private, so 75% left a 217 pt dead band.
Remove the combo, the zoom75 QA state and every zoom-scaled
branch in the QA tools."
```

---

### Task 2: Token-painted values in disabled groups (#1, #7, GENERATION SEED)

**Files:**
- Create: `plugin303/src/gui/widgets/readout.rs`
- Modify: `plugin303/src/gui/widgets/mod.rs`
- Modify: `plugin303/src/gui/theme.rs:150-171` (contrast helpers move out of `mod tests`)
- Modify: `plugin303/src/gui/panels/tweaks_panel.rs:1-287`
- Modify: `plugin303/src/gui/panels/generator_panel.rs:355-364`
- Create: `plugin303/tests/gui_common/mod.rs`
- Create: `plugin303/tests/gui_text_contrast.rs`
- Modify: `plugin303/tests/gui_layout_test.rs:3-32` (use the shared `DummyGuiContext`)
- Modify: `.claude/skills/gui-visual-audit/references/design-contract.md` (Dimming, Components)

**Interfaces:**
- Consumes: nothing new.
- Produces:
  - `plugin303::gui::widgets::Readout::new(text: &str, width: f32, height: f32) -> Readout<'_>`, a `Widget` that senses hover only.
  - `plugin303::gui::theme::rel_luminance(c: Color32) -> f64`, `plugin303::gui::theme::contrast_ratio(a: Color32, b: Color32) -> f64`.
  - `plugin303::gui::panels::tweaks_panel::FIELD_W: f32 = 80.0`.
  - Test rig in `plugin303/tests/gui_common/mod.rs`: `DummyGuiContext`, `ENV_LOCK`, `EnvGuard::set(key, val)`, `Rig::new(size: Vec2)`, `Rig::run(&mut self, frames: usize, pointer: Option<Pos2>, add: &mut dyn FnMut(&mut Ui)) -> Vec<(String, Color32)>`, `Rig::widgets(&self) -> Vec<egui::WidgetRect>`, `Rig::hover(&mut self, rect: Rect, add: &mut dyn FnMut(&mut Ui)) -> Vec<String>`, `TEXT_TOKENS: [Color32; 5]`.

Background: a throwaway probe on the current tree listed the off-token text colours per QA state. `default`, `advanced`, `running`: only `0042` (GENERATION SEED, `#898989`). `accurate`: TOL, AGE, NOISE, CMP SEED, CALIBRATED at `#6F6F6F`, and CALIBRATE, `0%`, `0 yrs`, `0%`, `1337`, `0042` at `#898989`. egui's fade halves each colour towards `PANEL_BG`; the rasterised glyph lands at 2.92:1.

- [ ] **Step 1: Make the contrast helpers public**

In `plugin303/src/gui/theme.rs`, cut `rel_luminance` and `contrast_ratio` out of `mod tests` (lines 154-171) and paste them above `#[cfg(test)]` as public functions:

```rust
/// WCAG relative luminance of an sRGB colour.
pub fn rel_luminance(c: Color32) -> f64 {
    let to_linear = |v: u8| {
        let s = v as f64 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * to_linear(c.r()) + 0.7152 * to_linear(c.g()) + 0.0722 * to_linear(c.b())
}

/// WCAG contrast ratio of two colours, in either order.
pub fn contrast_ratio(c1: Color32, c2: Color32) -> f64 {
    let l1 = rel_luminance(c1);
    let l2 = rel_luminance(c2);
    let (hi, lo) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (hi + 0.05) / (lo + 0.05)
}
```

`mod tests` keeps `use super::*;`, so its tests compile unchanged.

- [ ] **Step 2: Create the shared GUI test rig**

Create `plugin303/tests/gui_common/mod.rs`:

```rust
//! Shared rig for editor tests: drive egui frames, read painted text and its
//! colour, hover a widget. Each test binary uses a subset.
#![allow(dead_code)]

use nih_plug::prelude::{GuiContext, ParamPtr, PluginApi, PluginState};
use nih_plug_egui::egui::{
    self, epaint::Shape, vec2, CentralPanel, Color32, Context, Frame, Pos2, RawInput, Rect, Ui,
    Vec2,
};
use plugin303::gui::theme;
use std::sync::Mutex;

/// Text colours the contract allows (Colour roles). Anything else is a faded
/// or hard-coded colour.
pub const TEXT_TOKENS: [Color32; 5] = [
    theme::TEXT_PRIMARY,
    theme::TEXT_SECONDARY,
    theme::TEXT_MUTED,
    theme::TEXT_ON_ACCENT,
    theme::ACCENT_ORANGE,
];

/// Serialises `VIBE303_QA_STATE` edits: the tests of one binary share a process.
pub static ENV_LOCK: Mutex<()> = Mutex::new(());

pub struct EnvGuard<'a> {
    key: &'a str,
}

impl<'a> EnvGuard<'a> {
    pub fn set(key: &'a str, val: &str) -> Self {
        unsafe { std::env::set_var(key, val) };
        Self { key }
    }
}

impl Drop for EnvGuard<'_> {
    fn drop(&mut self) {
        unsafe { std::env::remove_var(self.key) };
    }
}

/// The editor never asks its host for state while drawing.
pub struct DummyGuiContext;

impl GuiContext for DummyGuiContext {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Standalone
    }
    fn request_resize(&self) -> bool {
        false
    }
    unsafe fn raw_begin_set_parameter(&self, _param: ParamPtr) {}
    unsafe fn raw_set_parameter_normalized(&self, _param: ParamPtr, _normalized: f32) {}
    unsafe fn raw_end_set_parameter(&self, _param: ParamPtr) {}
    fn get_state(&self) -> PluginState {
        unimplemented!("state query not exercised by editor tests")
    }
    fn set_state(&self, _state: PluginState) {}
}

/// Every text run in `shape` with the colour it is painted in. egui's fade
/// rewrites the glyph vertex colours, so this is the colour on screen.
fn collect_texts(shape: &Shape, out: &mut Vec<(String, Color32)>) {
    match shape {
        Shape::Text(t) => {
            let color = t
                .override_text_color
                .or_else(|| {
                    t.galley
                        .rows
                        .iter()
                        .find_map(|r| r.visuals.mesh.vertices.first())
                        .map(|v| v.color)
                        .filter(|c| *c != Color32::PLACEHOLDER)
                })
                .unwrap_or(t.fallback_color);
            out.push((t.galley.text().to_string(), color));
        }
        Shape::Vec(v) => v.iter().for_each(|s| collect_texts(s, out)),
        _ => {}
    }
}

pub struct Rig {
    pub ctx: Context,
    size: Vec2,
    time: f64,
}

impl Rig {
    pub fn new(size: Vec2) -> Self {
        let ctx = Context::default();
        theme::apply(&ctx);
        Self { ctx, size, time: 0.0 }
    }

    /// Run `frames` frames, 0.5 s apart, with the pointer at `pointer` (or gone).
    /// Returns the text runs of the last frame.
    pub fn run(
        &mut self,
        frames: usize,
        pointer: Option<Pos2>,
        add: &mut dyn FnMut(&mut Ui),
    ) -> Vec<(String, Color32)> {
        let mut found = Vec::new();
        for _ in 0..frames {
            self.time += 0.5;
            let mut input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
                time: Some(self.time),
                ..Default::default()
            };
            input.events.push(match pointer {
                Some(p) => egui::Event::PointerMoved(p),
                None => egui::Event::PointerGone,
            });
            let out = self.ctx.run(input, |ctx| {
                CentralPanel::default()
                    .frame(Frame::new())
                    .show(ctx, |ui| add(ui));
            });
            found.clear();
            out.shapes.iter().for_each(|c| collect_texts(&c.shape, &mut found));
        }
        found
    }

    /// Widgets laid out in the last frame.
    pub fn widgets(&self) -> Vec<egui::WidgetRect> {
        self.ctx.viewport(|vp| {
            vp.prev_pass
                .widgets
                .layers()
                .flat_map(|(_, ws)| ws.iter().cloned())
                .collect()
        })
    }

    /// Hover the widget at `rect` past egui's 0.5 s tooltip delay; return the
    /// texts on screen. Moves the pointer away afterwards so the tooltip closes.
    pub fn hover(&mut self, rect: Rect, add: &mut dyn FnMut(&mut Ui)) -> Vec<String> {
        let at = rect.lerp_inside(vec2(0.3, 0.5));
        let texts = self.run(4, Some(at), add);
        self.run(2, None, add);
        texts.into_iter().map(|(t, _)| t).collect()
    }
}
```

In `plugin303/tests/gui_layout_test.rs`, delete the local `DummyGuiContext` (lines 12-32) and the now-unused `GuiContext, PluginApi` import (line 3); add at the top, after `#![cfg(feature = "gui")]`:

```rust
mod gui_common;
use gui_common::DummyGuiContext;
```

- [ ] **Step 3: Write the failing text-colour test**

Create `plugin303/tests/gui_text_contrast.rs`:

```rust
#![cfg(feature = "gui")]

mod gui_common;

use gui_common::{DummyGuiContext, EnvGuard, Rig, ENV_LOCK, TEXT_TOKENS};
use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::vec2;
use plugin303::gui::{qa_generator_params, render_editor, theme, GUI_HEIGHT, GUI_WIDTH};
use plugin303::params::Plugin303Params;
use std::sync::atomic::AtomicU8;

/// Every text the editor paints uses a text token at full strength. egui's
/// disabled fade blends a built-in widget's colour halfway to PANEL_BG, which
/// drops TEXT_SECONDARY labels to 2.9:1 on screen (round-20 #1). A colour
/// outside the token set is that fade or a hard-coded colour.
#[test]
fn every_text_uses_a_token_in_every_mode() {
    let _lock = ENV_LOCK.lock().unwrap();
    for state in ["default", "accurate", "advanced", "running"] {
        let _env = EnvGuard::set("VIBE303_QA_STATE", state);
        let params = Plugin303Params::default();
        let host = DummyGuiContext;
        let setter = ParamSetter::new(&host);
        let mut gen_params = qa_generator_params(Some(state));
        let step = AtomicU8::new(3);
        let mut rig = Rig::new(vec2(GUI_WIDTH as f32, GUI_HEIGHT as f32));
        let texts = rig.run(2, None, &mut |ui| {
            render_editor(ui, &params, &setter, &mut gen_params, &step)
        });
        assert!(texts.len() > 100, "{state}: editor painted only {} texts", texts.len());
        let off: Vec<String> = texts
            .iter()
            .filter(|(_, c)| !TEXT_TOKENS.contains(c))
            .map(|(t, c)| {
                format!("{t:?} {c:?} = {:.2}:1 on PANEL_BG", theme::contrast_ratio(*c, theme::PANEL_BG))
            })
            .collect();
        assert!(off.is_empty(), "{state}: text outside the token set: {off:#?}");
    }
}
```

- [ ] **Step 4: Run it to see it fail**

Run: `cargo test -p plugin303 --release --test gui_text_contrast`
Expected: FAIL in `default` with `"0042" #89_89_89_FF`. (The `accurate` list matches the background note above once `default` passes.)

- [ ] **Step 5: Create the `Readout` widget**

Create `plugin303/src/gui/widgets/readout.rs`:

```rust
use nih_plug_egui::egui::{vec2, Align2, FontId, Response, Sense, Ui, Vec2, Widget};

use crate::gui::theme;

/// A value well that cannot be edited: a value in a disabled group, or a
/// read-only figure such as the generator seed. egui's disabled fade would dim
/// a built-in DragValue or ComboBox on top of its token, so this paints the
/// tokens itself: a `WELL_BG` well with `TEXT_SECONDARY` monospace text.
pub struct Readout<'a> {
    text: &'a str,
    size: Vec2,
}

impl<'a> Readout<'a> {
    pub fn new(text: &'a str, width: f32, height: f32) -> Self {
        Self {
            text,
            size: vec2(width, height),
        }
    }
}

impl Widget for Readout<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, response) = ui.allocate_exact_size(self.size, Sense::hover());
        if ui.is_rect_visible(rect) {
            let mut painter = ui.painter().clone();
            painter.set_fade_to_color(None);
            painter.rect_filled(rect, theme::radius(), theme::WELL_BG);
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                self.text,
                FontId::monospace(9.0),
                theme::TEXT_SECONDARY,
            );
        }
        response
    }
}
```

In `plugin303/src/gui/widgets/mod.rs` add `pub mod readout;` (alphabetical, after `piano_roll_popup`) and `pub use readout::Readout;` (after the `piano_roll_popup` re-export).

- [ ] **Step 6: GENERATION SEED becomes a Readout**

In `plugin303/src/gui/panels/generator_panel.rs`, add `Readout` to the widgets import (`use crate::gui::widgets::{DevicePanel, Readout, ToggleButton};`) and replace lines 356-364 (`let mut seed_val …` through the closing `});` of `add_enabled_ui`) with:

```rust
                    let seed_text = format!("{:04}", gen_params.seed % 10000);
                    ui.add(Readout::new(&seed_text, G_FIELD_W, G_ROW_H));
```

- [ ] **Step 7: Rewrite the ANALOG TWEAKS header and field column**

In `plugin303/src/gui/panels/tweaks_panel.rs`:

Imports:

```rust
use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{
    pos2, vec2, Align, Align2, Button, Color32, ComboBox, DragValue, FontId, Layout, Rect,
    RichText, Sense, Ui,
};

use crate::gui::theme;
use crate::gui::widgets::panel::DevicePanel;
use crate::gui::widgets::{Knob303, Readout};
use crate::params::{CircuitTopology, CompTolerance, Plugin303Params};

/// Width of every value in the field column: the TOL combo's width, so the
/// four fields share one right edge (round-20 #7).
pub const FIELD_W: f32 = 80.0;
```

Header closure: replace `header_ui.add_enabled_ui(!is_accurate, |ui| { … });` (lines 25-56) with the same body without the enabled scope, behind an early return. ACCURATE shows no CALIBRATE: the model it rebuilds is FUN-only, and a disabled egui `Button` can only render faded.

```rust
            |header_ui| {
                if is_accurate {
                    return;
                }
                let btn_text = if calibrating {
                    "CALIBRATING..."
                } else {
                    "CALIBRATE"
                };

                let mut cal_btn = Button::new(
                    RichText::new(btn_text)
                        .font(FontId::proportional(8.5))
                        .color(if needs_rebuild || calibrating {
                            theme::TEXT_ON_ACCENT
                        } else {
                            theme::TEXT_PRIMARY
                        }),
                )
                .min_size(vec2(80.0, crate::gui::widgets::toggle::TOGGLE_H));

                if needs_rebuild || calibrating {
                    cal_btn = cal_btn.fill(theme::ACCENT_ORANGE);
                }

                if header_ui
                    .add(cal_btn)
                    .on_hover_text("Rebuild analog model with current component tolerances")
                    .clicked()
                {
                    setter.begin_set_parameter(&params.calibrate_sync);
                    setter.set_parameter(&params.calibrate_sync, true);
                    setter.end_set_parameter(&params.calibrate_sync);
                }
            },
```

Body: inside `ui.scope(|ui| { theme::field_style(ui); … })` (lines 80-246), replace everything from `const LABEL_W: f32 = 60.0;` through the end of the status `ui.horizontal(…);` with:

```rust
                                    const LABEL_W: f32 = 60.0;
                                    let row_h = crate::gui::widgets::toggle::TOGGLE_H;
                                    let status_h = 14.0;
                                    let content_h = 4.0 * row_h + status_h;
                                    // Even gaps: above the first row, between rows, below the status.
                                    let v_gap = ((total_h - content_h) / 6.0).max(3.0);

                                    // Labels are painted, not egui Labels, so the
                                    // disabled group cannot fade them (round-20 #1).
                                    let label_color = if is_accurate {
                                        theme::TEXT_MUTED
                                    } else {
                                        theme::TEXT_SECONDARY
                                    };
                                    let label_font = FontId::proportional(8.5);
                                    let paint_left = |ui: &Ui, rect: Rect, text: &str, color: Color32| {
                                        let mut painter = ui.painter().clone();
                                        painter.set_fade_to_color(None);
                                        painter.text(
                                            pos2(rect.left(), rect.center().y),
                                            Align2::LEFT_CENTER,
                                            text,
                                            label_font.clone(),
                                            color,
                                        );
                                    };

                                    let field_row = |ui: &mut Ui,
                                                     label: &str,
                                                     add_content: &mut dyn FnMut(&mut Ui)| {
                                        ui.horizontal(|ui| {
                                            let (rect, _) = ui.allocate_exact_size(
                                                vec2(LABEL_W, row_h),
                                                Sense::hover(),
                                            );
                                            paint_left(ui, rect, label, label_color);
                                            add_content(ui);
                                        });
                                    };

                                    ui.add_space(v_gap);

                                    // TOL
                                    field_row(ui, "TOL", &mut |ui| {
                                        let tol = params.tolerance.value();
                                        let tol_text = match tol {
                                            CompTolerance::Zero => "0%",
                                            CompTolerance::PointFive => "0.5%",
                                            CompTolerance::Two => "2%",
                                            CompTolerance::Five => "5%",
                                            CompTolerance::Ten => "10%",
                                        };
                                        if is_accurate {
                                            ui.add(Readout::new(tol_text, FIELD_W, row_h));
                                            return;
                                        }
                                        ComboBox::from_id_salt("tweaks_tolerance_combo")
                                            .selected_text(
                                                RichText::new(tol_text)
                                                    .font(FontId::proportional(8.5)),
                                            )
                                            .width(FIELD_W)
                                            .show_ui(ui, |ui| {
                                                for (variant, opt_label) in [
                                                    (CompTolerance::Zero, "0% (Ideal)"),
                                                    (CompTolerance::PointFive, "0.5% (Tight)"),
                                                    (CompTolerance::Two, "2% (Stock)"),
                                                    (CompTolerance::Five, "5% (Loose)"),
                                                    (CompTolerance::Ten, "10% (Wild)"),
                                                ] {
                                                    if ui
                                                        .selectable_label(
                                                            tol == variant,
                                                            RichText::new(opt_label)
                                                                .font(FontId::proportional(8.5)),
                                                        )
                                                        .clicked()
                                                    {
                                                        setter
                                                            .begin_set_parameter(&params.tolerance);
                                                        setter.set_parameter(
                                                            &params.tolerance,
                                                            variant,
                                                        );
                                                        setter.end_set_parameter(&params.tolerance);
                                                    }
                                                }
                                            });
                                    });

                                    ui.add_space(v_gap);

                                    // AGE
                                    field_row(ui, "AGE", &mut |ui| {
                                        let age = params.comp_age.value();
                                        if is_accurate {
                                            ui.add(Readout::new(&format!("{age} yrs"), FIELD_W, row_h));
                                            return;
                                        }
                                        let mut age_val = age;
                                        if ui
                                            .add_sized(
                                                vec2(FIELD_W, row_h),
                                                DragValue::new(&mut age_val)
                                                    .range(0..=100)
                                                    .speed(0.2)
                                                    .custom_formatter(|v, _| {
                                                        format!("{:.0} yrs", v)
                                                    }),
                                            )
                                            .on_hover_text("Component Aging (0-100 years drag)")
                                            .changed()
                                        {
                                            setter.begin_set_parameter(&params.comp_age);
                                            setter.set_parameter(&params.comp_age, age_val);
                                            setter.end_set_parameter(&params.comp_age);
                                        }
                                    });

                                    ui.add_space(v_gap);

                                    // NOISE
                                    field_row(ui, "NOISE", &mut |ui| {
                                        let noise = params.thermal_noise.value();
                                        if is_accurate {
                                            ui.add(Readout::new(&format!("{noise}%"), FIELD_W, row_h));
                                            return;
                                        }
                                        let mut noise_val = noise;
                                        if ui
                                            .add_sized(
                                                vec2(FIELD_W, row_h),
                                                DragValue::new(&mut noise_val)
                                                    .range(0..=100)
                                                    .speed(0.2)
                                                    .custom_formatter(|v, _| format!("{:.0}%", v)),
                                            )
                                            .on_hover_text("Thermal Noise floor (0-100% drag)")
                                            .changed()
                                        {
                                            setter.begin_set_parameter(&params.thermal_noise);
                                            setter.set_parameter(&params.thermal_noise, noise_val);
                                            setter.end_set_parameter(&params.thermal_noise);
                                        }
                                    });

                                    ui.add_space(v_gap);

                                    // CMP SEED
                                    field_row(ui, "CMP SEED", &mut |ui| {
                                        let seed = params.comp_seed.value();
                                        if is_accurate {
                                            ui.add(Readout::new(&format!("{seed:04}"), FIELD_W, row_h));
                                            return;
                                        }
                                        let mut seed_val = seed;
                                        if ui
                                            .add_sized(
                                                vec2(FIELD_W, row_h),
                                                DragValue::new(&mut seed_val)
                                                    .range(0..=9999)
                                                    .speed(1.0)
                                                    .custom_formatter(|v, _| {
                                                        format!("{:04}", v as u32)
                                                    }),
                                            )
                                            .on_hover_text(
                                                "Component Variation Seed (0000-9999 drag)",
                                            )
                                            .changed()
                                        {
                                            setter.begin_set_parameter(&params.comp_seed);
                                            setter.set_parameter(&params.comp_seed, seed_val);
                                            setter.end_set_parameter(&params.comp_seed);
                                        }
                                    });

                                    ui.add_space(v_gap);

                                    // Status under the fields. In ACCURATE it says the
                                    // card is FUN-only (round-20 #3).
                                    let (status_text, status_color) = if is_accurate {
                                        ("FUN MODE ONLY", theme::TEXT_MUTED)
                                    } else if needs_rebuild {
                                        ("SYNC PENDING", theme::ACCENT_ORANGE)
                                    } else {
                                        ("CALIBRATED", theme::TEXT_SECONDARY)
                                    };
                                    ui.horizontal(|ui| {
                                        ui.add_space(LABEL_W);
                                        let (rect, _) = ui.allocate_exact_size(
                                            vec2(ui.available_width(), status_h),
                                            Sense::hover(),
                                        );
                                        paint_left(ui, rect, status_text, status_color);
                                    });
```

- [ ] **Step 8: Run the test to see it pass**

Run: `cargo test -p plugin303 --release --test gui_text_contrast --test gui_layout_test`
Expected: PASS.

- [ ] **Step 9: Update the design contract**

In `.claude/skills/gui-visual-audit/references/design-contract.md`:

- Contrast, the bullet "Below 3:1 is a hard defect …": append "`plugin303/tests/gui_text_contrast.rs` fails on any text outside the text tokens."
- Dimming table: add a row `| Readout (`widgets/readout.rs`) | always `WELL_BG` well, `TEXT_SECONDARY` 9 pt monospace; a disabled group's labels turn `TEXT_MUTED` |`.
- Below the table, add the rule: "Inside a disabled group, a built-in egui widget (Label, DragValue, ComboBox, Button) is drift: egui fades it. Values become `Readout`s; labels are painted with the fade cleared; an action the mode cannot use is hidden."
- Disabled groups paragraph: replace "the ANALOG TWEAKS header and body in ACCURATE, and the read-only SEED field in GENERATION" with "and the ANALOG TWEAKS body in ACCURATE: its fields become Readouts, its status reads FUN MODE ONLY, and its header hides CALIBRATE. GENERATION SEED is a Readout in every mode."
- Components table, Value field row: append "A value that cannot be edited is a `Readout`. The ANALOG TWEAKS fields share one width, `FIELD_W` = 80 pt."

- [ ] **Step 10: Commit**

```bash
git add plugin303/src/gui plugin303/tests .claude/skills/gui-visual-audit/references/design-contract.md
git commit -m "fix(gui): paint disabled tweaks values with tokens, not egui's fade (#1, #7)

ANALOG TWEAKS labels fell to 2.92:1 in ACCURATE because egui
faded TEXT_SECONDARY towards PANEL_BG. Values in a disabled
group and the GENERATION seed are now Readouts, labels are
painted with the fade cleared, the four fields share one width,
and ACCURATE hides CALIBRATE. gui_text_contrast pins every text
colour to a token."
```

---

### Task 3: Disabled controls say why (#3, #10, QUALITY)

**Files:**
- Modify: `plugin303/src/gui/panels/tweaks_panel.rs` (hint const; readouts and knobs)
- Modify: `plugin303/src/gui/panels/header_bar.rs:95-112`
- Modify: `plugin303/src/gui/panels/top_controls.rs:1-72, 147-223`
- Create: `plugin303/tests/gui_tooltips.rs`
- Modify: `.claude/skills/gui-visual-audit/references/design-contract.md` (Dimming)

**Interfaces:**
- Consumes: `Rig`, `EnvGuard`, `ENV_LOCK`, `DummyGuiContext` (Task 2); `tweaks_panel::FIELD_W` (Task 2).
- Produces: `pub const tweaks_panel::ACCURATE_HINT: &str`, `pub const header_bar::QUALITY_ACCURATE_HINT: &str`, `pub const top_controls::ADV_ENV_OFF_HINT: &str`, `pub const top_controls::ADV_ENV_ACCURATE_HINT: &str`; `render_adsr_bank` takes a `disabled_hint: &str` argument.

Background: `Response::on_hover_text` never shows on a disabled widget; `on_disabled_hover_text` does. A probe on the current tree confirmed both with a disabled `Knob303`. `header_bar.rs:109` and every tweaks tooltip use `on_hover_text`.

- [ ] **Step 1: Write the failing test**

Create `plugin303/tests/gui_tooltips.rs`:

```rust
#![cfg(feature = "gui")]

mod gui_common;

use gui_common::{DummyGuiContext, EnvGuard, Rig, ENV_LOCK};
use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{self, vec2, Rect, Ui};
use plugin303::gui::panels::{header_bar, top_controls, tweaks_panel};
use plugin303::gui::widgets::toggle::TOGGLE_H;
use plugin303::gui::{split_columns, GUI_WIDTH, HEADER_BAR_H, MARGIN, TWEAKS_H, VOICE_ROW_H};
use plugin303::params::Plugin303Params;

/// Room below a device for its tooltip.
const TOOLTIP_ROOM: f32 = 200.0;

/// A control: something a user clicks or drags, or a read-only value well.
fn is_control(w: &egui::WidgetRect) -> bool {
    let readout = (w.rect.width() - tweaks_panel::FIELD_W).abs() < 0.5
        && (w.rect.height() - TOGGLE_H).abs() < 0.5;
    w.sense.senses_click() || w.sense.senses_drag() || readout
}

/// Every disabled control the device draws explains, on hover, why it is
/// disabled. egui shows `on_hover_text` only on enabled widgets.
fn assert_disabled_controls_explain(
    rig: &mut Rig,
    add: &mut dyn FnMut(&mut Ui),
    hint: &str,
    at_least: usize,
) {
    rig.run(2, None, add);
    let targets: Vec<Rect> = rig
        .widgets()
        .into_iter()
        .filter(|w| !w.enabled && is_control(w))
        .map(|w| w.rect)
        .collect();
    assert!(
        targets.len() >= at_least,
        "expected at least {at_least} disabled controls, found {targets:?}"
    );
    for rect in targets {
        let texts = rig.hover(rect, add);
        assert!(
            texts.iter().any(|t| t == hint),
            "disabled control at {rect:?} does not show {hint:?}; on screen: {texts:?}"
        );
    }
}

fn right_column_width() -> f32 {
    split_columns(GUI_WIDTH as f32 - 2.0 * MARGIN).1
}

#[test]
fn accurate_tweaks_explain_why_they_are_inert() {
    let _lock = ENV_LOCK.lock().unwrap();
    let _env = EnvGuard::set("VIBE303_QA_STATE", "accurate");
    let params = Plugin303Params::default();
    let host = DummyGuiContext;
    let setter = ParamSetter::new(&host);
    let mut rig = Rig::new(vec2(right_column_width(), TWEAKS_H + TOOLTIP_ROOM));
    // 4 Readouts + DRIFT + RECAL.
    assert_disabled_controls_explain(
        &mut rig,
        &mut |ui| tweaks_panel::render_tweaks_panel(ui, &params, &setter),
        tweaks_panel::ACCURATE_HINT,
        6,
    );
}

#[test]
fn accurate_quality_explains_why_it_is_inert() {
    let _lock = ENV_LOCK.lock().unwrap();
    let _env = EnvGuard::set("VIBE303_QA_STATE", "accurate");
    let params = Plugin303Params::default();
    let host = DummyGuiContext;
    let setter = ParamSetter::new(&host);
    let width = GUI_WIDTH as f32 - 2.0 * MARGIN;
    let mut rig = Rig::new(vec2(width, HEADER_BAR_H + TOOLTIP_ROOM));
    assert_disabled_controls_explain(
        &mut rig,
        &mut |ui| header_bar::render_header(ui, &params, &setter),
        header_bar::QUALITY_ACCURATE_HINT,
        1,
    );
}

#[test]
fn envelope_banks_without_adv_env_say_how_to_enable_them() {
    let _lock = ENV_LOCK.lock().unwrap();
    let params = Plugin303Params::default(); // FUN, ADV ENV off
    let host = DummyGuiContext;
    let setter = ParamSetter::new(&host);
    let mut rig = Rig::new(vec2(right_column_width(), VOICE_ROW_H + TOOLTIP_ROOM));
    // MEG A D S R + VEG A D S R: the voice reads neither bank without ADV ENV.
    assert_disabled_controls_explain(
        &mut rig,
        &mut |ui| top_controls::render_envelope_panel(ui, &params, &setter),
        top_controls::ADV_ENV_OFF_HINT,
        8,
    );
}

#[test]
fn accurate_envelopes_explain_that_adv_env_is_fun_only() {
    let _lock = ENV_LOCK.lock().unwrap();
    let _env = EnvGuard::set("VIBE303_QA_STATE", "accurate");
    let params = Plugin303Params::default();
    let host = DummyGuiContext;
    let setter = ParamSetter::new(&host);
    let mut rig = Rig::new(vec2(right_column_width(), VOICE_ROW_H + TOOLTIP_ROOM));
    // 8 faders + the ADV ENV tile.
    assert_disabled_controls_explain(
        &mut rig,
        &mut |ui| top_controls::render_envelope_panel(ui, &params, &setter),
        top_controls::ADV_ENV_ACCURATE_HINT,
        9,
    );
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p plugin303 --release --test gui_tooltips`
Expected: compile error, `ACCURATE_HINT` / `QUALITY_ACCURATE_HINT` / `ADV_ENV_OFF_HINT` / `ADV_ENV_ACCURATE_HINT` not found.

- [ ] **Step 3: ANALOG TWEAKS hints**

In `tweaks_panel.rs`, below `FIELD_W`:

```rust
/// Hover text of every ANALOG TWEAKS control in ACCURATE (round-20 #3).
pub const ACCURATE_HINT: &str =
    "Analog tweaks apply to FUN mode only. Set TOPOLOGY to FUN to edit them.";
```

Chain `.on_disabled_hover_text(ACCURATE_HINT)` onto each of the four Readouts, for example:

```rust
                                        if is_accurate {
                                            ui.add(Readout::new(tol_text, FIELD_W, row_h))
                                                .on_disabled_hover_text(ACCURATE_HINT);
                                            return;
                                        }
```

and onto both knobs:

```rust
                            ui.add(Knob303::new(
                                &params.comp_drift,
                                setter,
                                "DRIFT",
                                theme::KNOB_RADIUS_LARGE,
                            ))
                            .on_disabled_hover_text(ACCURATE_HINT);
```

(same for `RECAL`).

- [ ] **Step 4: QUALITY hint**

In `header_bar.rs`:

```rust
/// Hover text of the QUALITY switch in ACCURATE.
pub const QUALITY_ACCURATE_HINT: &str =
    "Quality tier applies to FUN mode (ACCURATE uses fixed physical modeling)";
```

Replace lines 97-112 with:

```rust
    ui.add_enabled_ui(!is_accurate, |ui| {
        ui.add(
            SlideSwitch::new(
                &params.tier,
                setter,
                "QUALITY",
                &["DRAFT", "LOW", "HIGH", "BEST"],
            )
            .caption_left(),
        )
        .on_disabled_hover_text(QUALITY_ACCURATE_HINT);
    });
```

- [ ] **Step 5: ENVELOPES gating and hints**

In `top_controls.rs`:

Imports: `use crate::params::{CircuitTopology, Plugin303Params};`

Add:

```rust
/// Hover text of an ADSR fader while ADV ENV is off (round-20 #10).
pub const ADV_ENV_OFF_HINT: &str = "Turn on ADV ENV to edit the MEG and VEG envelopes.";
/// Hover text of the ADV ENV tile and the ADSR faders in ACCURATE.
pub const ADV_ENV_ACCURATE_HINT: &str =
    "ADV ENV applies to FUN mode only. Set TOPOLOGY to FUN to use it.";
```

`render_adsr_bank`: add a parameter `disabled_hint: &str` after `setter`, and chain it onto each fader:

```rust
                        ui.add(MiniFader::new(
                            param,
                            setter,
                            label,
                            track_h,
                            theme::ACCENT_ORANGE,
                        ))
                        .on_disabled_hover_text(disabled_hint);
```

`render_envelope_panel`: the voice reads both ADSR banks only in FUN with ADV ENV on (`voice.rs:813-833` forces ADV ENV off in ACCURATE; `voice.rs:916-941` reads `meg_*` and `veg_*` only then). Replace the function body with:

```rust
pub fn render_envelope_panel(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let is_accurate = params.topology.value() == CircuitTopology::Accurate;
    // Header and body closures both live while the card is built, so the ADV ENV
    // state is shared through a Cell instead of a `&mut` the header would hold.
    let adv = Cell::new(params.advanced_env.value());
    DevicePanel::new("ENVELOPES", crate::gui::VOICE_ROW_H).show(
        ui,
        |header_ui| {
            header_ui.add_enabled_ui(!is_accurate, |ui| {
                let mut on = adv.get();
                if ui
                    .add(ToggleButton::new(&mut on, "ADV ENV"))
                    .on_hover_text("Lit: the MEG and VEG ADSR banks replace the classic 303 decay.")
                    .on_disabled_hover_text(ADV_ENV_ACCURATE_HINT)
                    .changed()
                {
                    adv.set(on);
                    setter.begin_set_parameter(&params.advanced_env);
                    setter.set_parameter(&params.advanced_env, on);
                    setter.end_set_parameter(&params.advanced_env);
                }
            });
        },
        |body_ui| {
            let banks_live = adv.get() && !is_accurate;
            let hint = if is_accurate {
                ADV_ENV_ACCURATE_HINT
            } else {
                ADV_ENV_OFF_HINT
            };
            let body_h = body_ui.available_height();
            body_ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                let track_h = (body_h - CAPTION_H - fader::CHROME_H).max(10.0);

                let total_w = ui.available_width();
                // 13 px of inter-bank separator area: 6 px + 1 px separator + 6 px (<= 8 px each side)
                const SEP_PAD: f32 = 6.0;
                let bank_w = (total_w - (2.0 * SEP_PAD + 1.0)).max(0.0) * 0.5;
                let bank_h = body_h;

                ui.allocate_ui_with_layout(
                    vec2(bank_w, bank_h),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.add_enabled_ui(banks_live, |ui| {
                            render_adsr_bank(
                                ui,
                                "MEG ADSR (Mod Envelope)",
                                [
                                    (&params.meg_attack, "A"),
                                    (&params.meg_decay_adj, "D"),
                                    (&params.meg_sustain, "S"),
                                    (&params.meg_release, "R"),
                                ],
                                track_h,
                                setter,
                                hint,
                            );
                        });
                    },
                );

                ui.add_space(SEP_PAD);
                ui.separator();
                ui.add_space(SEP_PAD);

                ui.allocate_ui_with_layout(
                    vec2(bank_w, bank_h),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.add_enabled_ui(banks_live, |ui| {
                            render_adsr_bank(
                                ui,
                                "VEG ADSR (Voice Envelope)",
                                [
                                    (&params.veg_attack, "A"),
                                    (&params.veg_decay_adj, "D"),
                                    (&params.veg_sustain, "S"),
                                    (&params.veg_release, "R"),
                                ],
                                track_h,
                                setter,
                                hint,
                            );
                        });
                    },
                );
            });
        },
    );
}
```

- [ ] **Step 6: Run the test to see it pass**

Run: `cargo test -p plugin303 --release --test gui_tooltips --test gui_text_contrast --test gui_layout_test`
Expected: PASS (4 + 1 + 3 tests).

- [ ] **Step 7: Update the design contract**

Dimming section:
- Add: "A disabled control says why on hover with `on_disabled_hover_text`; `on_hover_text` never shows on a disabled widget. `plugin303/tests/gui_tooltips.rs` checks QUALITY and ANALOG TWEAKS in ACCURATE and both ADSR banks."
- Disabled groups: replace "the MEG ADSR bank without ADV ENV" with "both ADSR banks (MEG and VEG) unless ADV ENV is on in FUN; the ADV ENV tile in ACCURATE".

- [ ] **Step 8: Commit**

```bash
git add plugin303/src/gui plugin303/tests/gui_tooltips.rs \
  .claude/skills/gui-visual-audit/references/design-contract.md
git commit -m "fix(gui): disabled controls explain themselves on hover (#3, #10)

on_hover_text never shows on a disabled widget. ANALOG TWEAKS and
QUALITY in ACCURATE, and the ADSR banks without ADV ENV, now use
on_disabled_hover_text. The VEG bank and the ADV ENV tile dim
when the voice ignores them (voice.rs reads both ADSR banks only
with ADV ENV on in FUN)."
```

---

### Task 4: LED-slot toggles; GENERATE raised (#4, #5, flagged STOP)

**Files:**
- Modify: `plugin303/src/gui/widgets/toggle.rs` (whole file)
- Modify: `plugin303/src/gui/panels/sequencer_panel.rs:107-243`
- Modify: `plugin303/src/gui/panels/generator_panel.rs:370-377`
- Modify: `.claude/skills/gui-visual-audit/references/design-contract.md` (Colour roles, Dimming, Components)

**Interfaces:**
- Consumes: `theme::contrast_ratio` (Task 2).
- Produces: `ToggleButton` keeps its API (`new`, `min_width`, `height`, `TOGGLE_H`); its paint changes.

- [ ] **Step 1: Write the failing tests**

Replace the `#[cfg(test)] mod tests` of `toggle.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const MODES: [(bool, bool); 3] = [(false, true), (true, true), (false, false)]; // (hovered, enabled)

    /// A disabled toggle never shows the accent: lit orange means "live".
    #[test]
    fn disabled_toggle_never_claims_accent() {
        for on in [true, false] {
            for hovered in [true, false] {
                let c = toggle_colors(on, hovered, false);
                for accent in [theme::ACCENT_ORANGE, theme::ACCENT_ORANGE_HOVER] {
                    assert_ne!(c.fill, accent);
                    assert_ne!(c.led, accent);
                }
            }
        }
    }

    /// Only the LED carries the state: the tile is the same plane on and off,
    /// so a boolean never paints a block of orange (round-20 #4).
    #[test]
    fn only_the_led_shows_state() {
        for (hovered, enabled) in MODES {
            let on = toggle_colors(true, hovered, enabled);
            let off = toggle_colors(false, hovered, enabled);
            assert_eq!(on.fill, off.fill, "hovered={hovered} enabled={enabled}");
            assert_ne!(on.led, off.led, "hovered={hovered} enabled={enabled}");
        }
    }

    /// A lit LED is a non-text mark: ≥ 3:1 on its tile, disabled included, so a
    /// disabled selection still shows.
    #[test]
    fn lit_led_clears_the_mark_floor_on_its_tile() {
        for (hovered, enabled) in MODES {
            let c = toggle_colors(true, hovered, enabled);
            let ratio = theme::contrast_ratio(c.led, c.fill);
            assert!(ratio >= 3.0, "hovered={hovered} enabled={enabled}: {ratio:.2}");
        }
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p plugin303 --release --lib gui::widgets::toggle`
Expected: compile error, `toggle_colors` returns a tuple without `fill`/`led` fields.

- [ ] **Step 3: Implement the LED toggle**

Replace `toggle.rs` above the tests with:

```rust
use nih_plug_egui::egui::{
    pos2, vec2, Align2, Color32, CornerRadius, FontFamily, FontId, Rect, Response, Sense, Ui,
    Widget,
};

use crate::gui::theme;

/// Default height of a toggle and flat button: one height, one row rhythm.
pub const TOGGLE_H: f32 = 18.0;

/// The state LED, a slot left of the label.
const LED_W: f32 = 3.0;
const LED_H: f32 = 10.0;
const LED_GAP: f32 = 5.0;
const PAD_X: f32 = 8.0;

/// A raised on/off tile with an LED: orange LED when on, an empty `WELL_BG`
/// slot when off. The tile itself never fills orange, so an off toggle still
/// reads as a toggle (its slot) and not as an action, and the lit area on
/// screen stays small (round-20 #4, #5). Replaces egui's checkbox.
pub struct ToggleButton<'a> {
    on: &'a mut bool,
    label: &'a str,
    min_width: f32,
    height: f32,
}

impl<'a> ToggleButton<'a> {
    pub fn new(on: &'a mut bool, label: &'a str) -> Self {
        Self {
            on,
            label,
            min_width: 0.0,
            height: TOGGLE_H,
        }
    }

    /// Floor for the tile width, so a group of toggles can share one width.
    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }

    /// Tile height, for rows that run taller than the default.
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }
}

/// Paint of one toggle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ToggleColors {
    fill: Color32,
    led: Color32,
    text: Color32,
}

fn toggle_colors(on: bool, hovered: bool, enabled: bool) -> ToggleColors {
    if !enabled {
        return ToggleColors {
            fill: theme::SURFACE_PRESSED,
            led: if on { theme::TEXT_MUTED } else { theme::WELL_BG },
            text: theme::TEXT_MUTED,
        };
    }
    let led = match (on, hovered) {
        (true, true) => theme::ACCENT_ORANGE_HOVER,
        (true, false) => theme::ACCENT_ORANGE,
        (false, _) => theme::WELL_BG,
    };
    ToggleColors {
        fill: if hovered {
            theme::SURFACE_HOVER
        } else {
            theme::SURFACE_ELEVATED
        },
        led,
        text: if on || hovered {
            theme::TEXT_PRIMARY
        } else {
            theme::TEXT_SECONDARY
        },
    }
}

impl Widget for ToggleButton<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let font = FontId::new(8.5, FontFamily::Proportional);
        let text_w = ui.fonts(|f| {
            f.layout_no_wrap(self.label.to_string(), font.clone(), Color32::WHITE)
                .size()
                .x
        });
        let group_w = LED_W + LED_GAP + text_w;
        let width = (group_w + 2.0 * PAD_X).max(self.min_width);
        let (rect, mut response) = ui.allocate_exact_size(vec2(width, self.height), Sense::click());
        if response.clicked() {
            *self.on = !*self.on;
            response.mark_changed();
        }
        if ui.is_rect_visible(rect) {
            let mut painter = ui.painter().clone();
            painter.set_fade_to_color(None);
            let c = toggle_colors(*self.on, response.hovered(), ui.is_enabled());
            painter.rect_filled(rect, theme::radius(), c.fill);
            // LED and label are centred as one group.
            let x0 = rect.center().x - group_w * 0.5;
            let led = Rect::from_center_size(
                pos2(x0 + LED_W * 0.5, rect.center().y),
                vec2(LED_W, LED_H),
            );
            painter.rect_filled(led, CornerRadius::same(1), c.led);
            painter.text(
                pos2(x0 + LED_W + LED_GAP, rect.center().y),
                Align2::LEFT_CENTER,
                self.label,
                font,
                c.text,
            );
        }
        response
    }
}
```

- [ ] **Step 4: Run the toggle tests**

Run: `cargo test -p plugin303 --release --lib gui::widgets::toggle`
Expected: PASS (3 tests).

- [ ] **Step 5: TRANSPORT booleans become toggles**

In `sequencer_panel.rs`, replace Row 1 (lines 107-150) with:

```rust
            // Row 1: RUN and SYNC
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(SLOT_GAP, 0.0);

                let mut run = params.seq_run.value();
                if ui
                    .add(ToggleButton::new(&mut run, "RUN").min_width(slot_w).height(ROW_H))
                    .on_hover_text("Lit: the internal sequencer runs.")
                    .changed()
                {
                    setter.begin_set_parameter(&params.seq_run);
                    setter.set_parameter(&params.seq_run, run);
                    setter.end_set_parameter(&params.seq_run);
                }

                let mut sync = params.seq_sync.value();
                if ui
                    .add(ToggleButton::new(&mut sync, "SYNC").min_width(slot_w).height(ROW_H))
                    .on_hover_text("Lit: follow the host transport and tempo. Unlit: internal clock at BPM.")
                    .changed()
                {
                    setter.begin_set_parameter(&params.seq_sync);
                    setter.set_parameter(&params.seq_sync, sync);
                    setter.end_set_parameter(&params.seq_sync);
                }
            });
```

In Row 2, replace the MIDI button block (lines 165-188) with:

```rust
                        let mut midi = params.midi_out.value();
                        let midi_btn_w = (slot_w * 0.55).floor();
                        if ui
                            .add(
                                ToggleButton::new(&mut midi, "MIDI")
                                    .min_width(midi_btn_w)
                                    .height(ROW_H),
                            )
                            .on_hover_text("Lit: send the sequence as MIDI notes on CH.")
                            .changed()
                        {
                            setter.begin_set_parameter(&params.midi_out);
                            setter.set_parameter(&params.midi_out, midi);
                            setter.end_set_parameter(&params.midi_out);
                        }
```

and the SYNTH block (lines 218-242) with:

```rust
                // Right slot: SYNTH voice on/off
                let mut synth_on = !params.synth_bypass.value();
                if ui
                    .add(ToggleButton::new(&mut synth_on, "SYNTH").min_width(slot_w).height(ROW_H))
                    .on_hover_text("Lit: the internal voice plays. Unlit: muted (silence, 0% CPU).")
                    .changed()
                {
                    setter.begin_set_parameter(&params.synth_bypass);
                    setter.set_parameter(&params.synth_bypass, !synth_on);
                    setter.end_set_parameter(&params.synth_bypass);
                }
```

- [ ] **Step 6: GENERATE becomes a raised action**

In `generator_panel.rs`, replace lines 370-377 with:

```rust
            // Row 2: GENERATE, a raised action like MUTATE. Orange is for live
            // state, and nothing is live about an action at rest (round-20 #4).
            let gen_btn = Button::new(
                nih_plug_egui::egui::RichText::new("GENERATE")
                    .font(FontId::proportional(8.5))
                    .color(theme::TEXT_PRIMARY),
            )
            .min_size(vec2(ui.available_width(), 22.0));
```

- [ ] **Step 7: Run the GUI tests**

Run: `cargo test -p plugin303 --release --test gui_layout_test --test gui_text_contrast --test gui_tooltips`
Expected: PASS. `transport_rows_share_one_seam_and_tile_height` still finds six rows of 28 pt tiles (a `ToggleButton` senses click).

- [ ] **Step 8: Measure the orange on screen**

```bash
bash scripts/render_gui_offscreen.sh
python3 - <<'EOF'
import numpy as np
from PIL import Image
D = "target/gui-qa-offscreen"
o = np.array([0xF6, 0x66, 0x1E])
for s in ("latest", "running"):
    a = np.asarray(Image.open(f"{D}/{s}.png").convert("RGB")).astype(int)
    m = np.abs(a - o).max(axis=2) <= 24
    print(s, int(m.sum()), "GENERATION", int(m[600:804, 1620:2424].sum()),
          "TRANSPORT", int(m[810:1272, 1620:2424].sum()))
EOF
```

Expected (round 20 before: `latest` 119,491 / GENERATE 31,953 / TRANSPORT 41,839): `latest` total about 46,000; GENERATION 0; TRANSPORT a few hundred (two 3 × 10 pt LEDs: SYNC, SYNTH). Read `latest.png`: TRANSPORT shows six raised rows; SYNC and SYNTH carry an orange LED; RUN, MIDI, JITTER an empty slot; CLEAR, < SHIFT, > SHIFT no slot.

- [ ] **Step 9: Update the design contract**

- Colour roles, `ACCENT_ORANGE`: replace "a lit toggle" with "a toggle's LED when on"; replace "lit actions (GENERATE, ENABLE FUN MODE, CALIBRATE when a rebuild is due)" with "lit actions (ENABLE FUN MODE, CALIBRATE when a rebuild is due)".
- Dimming table, the two "Segment, toggle" rows: split toggles out: `| Toggle: on | SURFACE_PRESSED tile, TEXT_MUTED LED and text |` and `| Toggle: off | SURFACE_PRESSED tile, WELL_BG LED, TEXT_MUTED text |`; the segment rows keep their current text with "toggle" removed.
- Components, Boolean row: "every on/off (RUN, SYNC, MIDI, SYNTH, JITTER, ADV ENV, LINK, ON, SLD CHAIN) is a raised tile with a 3 × 10 pt LED left of a state-free label: `WELL_BG` slot off, orange on (`ACCENT_ORANGE_HOVER` on hover). The tile never fills orange. 18 pt tall (28 pt in TRANSPORT). An egui checkbox is drift; a label that spells its state (`MIDI: OFF`) is drift."
- Components, Action row: append "An action at rest is never lit; GENERATE is raised."

- [ ] **Step 10: Commit**

```bash
git add plugin303/src/gui .claude/skills/gui-visual-audit/references/design-contract.md
git commit -m "fix(gui): LED toggles and a raised GENERATE (#4, #5)

Booleans are raised tiles with an LED: orange when on, an empty
slot when off, so an off toggle no longer looks like an action and
the lit area tracks live state. RUN, SYNC, MIDI and SYNTH become
ToggleButtons with state-free labels; GENERATE stops being lit."
```

---

### Task 5: Playhead plate only while running; 4 pt LED (#6)

**Files:**
- Modify: `plugin303/src/gui/widgets/step_button.rs:9-13, 45-62`
- Modify: `.claude/skills/gui-visual-audit/references/design-contract.md` (Planes, Components)

**Interfaces:**
- Consumes: nothing.
- Produces: `step_button::LED_H = 4.0`, `step_button::HEADER_H = 11.0`; `STRIP_HEIGHT` unchanged (99 pt), so `SEQUENCER_H` and the window layout do not move.

- [ ] **Step 1: Record the before state**

```bash
python3 - <<'EOF'
from PIL import Image
D = "target/gui-qa-offscreen"
# Gap between the ACC and SLD pills of a step: plate colour shows here.
for s, step, x in (("latest", "01", 120), ("running", "01", 120), ("running", "04", 712)):
    print(s, step, "#%02X%02X%02X" % Image.open(f"{D}/{s}.png").convert("RGB").getpixel((x, 913)))
EOF
python3 scripts/gui_qa_measure.py strokes target/gui-qa-offscreen/running.png --box 616,856,808,880 --color ACCENT_ORANGE
```

Expected now: `latest 01 #2F2F2F` (plate on a stopped step), `running 01 #262626`, `running 04 #2F2F2F`; the LED is one 6 px bar. If a probe reads `#3A3A3A`, it hit a pill: move x by ±1.

- [ ] **Step 2: Implement**

In `step_button.rs`:

```rust
pub const LED_H: f32 = 4.0;
pub const HEADER_H: f32 = 11.0;
```

Update the comment above `CONTROL_ROW_H` if it mentions 12 pt. Replace lines 45-50 with:

```rust
        // The plate marks the playing step only: while stopped no step plays,
        // so none is marked (round-20 #6).
        let strip_rect = Rect::from_min_size(strip_min, vec2(width, cell_height));
        if is_active && ui.is_rect_visible(strip_rect) {
            ui.painter()
                .rect_filled(strip_rect, theme::radius(), theme::PANEL_HEADER_BG);
        }
```

Change the LED comment to `// 1. Step LED: a LED_H bar across the column; ACCENT_ORANGE on the playing step, WELL_BG otherwise.`

- [ ] **Step 3: Test and render**

```bash
cargo test -p plugin303 --release --test gui_layout_test --test gui_text_contrast
bash scripts/render_gui_offscreen.sh
```

Re-run the two probes of Step 1.
Expected: `latest 01 #262626`, `running 01 #262626`, `running 04 #2F2F2F`; `strokes` reports one 8 px orange bar (4 pt). Layout test green (strip height unchanged).

- [ ] **Step 4: Update the design contract**

- Planes table, `PANEL_HEADER_BG`: "the current-step plate while the sequencer runs".
- Components, Step column: "4 pt LED bar, 11 pt step number (8.5 pt monospace)".
- Components, Playhead: "while the sequencer runs, the playing step's column gets a `PANEL_HEADER_BG` plate, its 4 pt LED turns orange and its number `TEXT_PRIMARY`. Stopped: no step is marked. No frame, no outline."

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/gui/widgets/step_button.rs .claude/skills/gui-visual-audit/references/design-contract.md
git commit -m "fix(gui): mark the playhead only while running, 4 pt LED (#6)"
```

---

### Task 6: CLEAR takes two clicks (#9)

**Files:**
- Modify: `plugin303/src/gui/panels/sequencer_panel.rs:22-26, 566-592, 598-611`
- Modify: `.claude/skills/gui-visual-audit/references/design-contract.md` (Components, Action)

**Interfaces:**
- Consumes: nothing.
- Produces: private `fn clear_armed(armed_at: Option<f64>, now: f64) -> bool`, `const CLEAR_CONFIRM_S: f64 = 3.0`.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` of `sequencer_panel.rs`:

```rust
    /// CLEAR wipes all 16 steps, so one click only arms it; a second click
    /// within CLEAR_CONFIRM_S clears. After the window the next click re-arms.
    #[test]
    fn clear_confirms_within_the_window_only() {
        assert!(!clear_armed(None, 10.0), "a first click must only arm");
        assert!(clear_armed(Some(10.0), 12.0));
        assert!(clear_armed(Some(10.0), 10.0 + CLEAR_CONFIRM_S), "boundary is inclusive");
        assert!(!clear_armed(Some(10.0), 13.5), "an expired arm must not clear");
    }
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p plugin303 --release --lib gui::panels::sequencer_panel`
Expected: compile error, `clear_armed` not found.

- [ ] **Step 3: Implement**

Below `SLOT_GAP`:

```rust
/// How long a first CLEAR click stays armed, in seconds.
const CLEAR_CONFIRM_S: f64 = 3.0;

/// Whether CLEAR, armed at `armed_at`, is still armed at `now`.
fn clear_armed(armed_at: Option<f64>, now: f64) -> bool {
    armed_at.is_some_and(|t| now - t <= CLEAR_CONFIRM_S)
}
```

Replace the CLEAR block (lines 566-592) with:

```rust
                // CLEAR wipes the pattern: the first click arms it, a second
                // click within CLEAR_CONFIRM_S clears (round-20 #9).
                let clear_id = ui.id().with("clear_armed_at");
                let now = ui.input(|i| i.time);
                let armed_at = ui.ctx().data(|d| d.get_temp::<f64>(clear_id));
                let armed = clear_armed(armed_at, now);
                let clear_btn = Button::new(
                    RichText::new(if armed { "CONFIRM CLEAR" } else { "CLEAR" })
                        .font(FontId::proportional(8.5))
                        .color(theme::TEXT_PRIMARY),
                )
                .fill(if armed {
                    theme::SURFACE_HOVER
                } else {
                    theme::SURFACE_ELEVATED
                })
                .min_size(vec2(slot_w, ROW_H));

                if ui
                    .add(clear_btn)
                    .on_hover_text("Reset all 16 steps to C3 notes. Click twice to confirm.")
                    .clicked()
                {
                    if armed {
                        ui.ctx().data_mut(|d| d.remove::<f64>(clear_id));
                        if let Ok(mut pattern_guard) = params.pattern.write() {
                            for step in pattern_guard.steps.iter_mut() {
                                *step = SequencerStep {
                                    semitone: 0,
                                    octave: 0,
                                    gate: StepGate::Note,
                                    accent: false,
                                    slide: false,
                                    gate_len: 0.5,
                                    velocity: 0.7,
                                };
                            }
                        }
                    } else {
                        ui.ctx().data_mut(|d| d.insert_temp(clear_id, now));
                    }
                }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p plugin303 --release --lib gui::panels::sequencer_panel` and `cargo test -p plugin303 --release --test gui_layout_test`
Expected: PASS.

- [ ] **Step 5: Smoke test in the standalone**

Run `cargo run --release --bin standalone -p plugin303`. Click CLEAR once: the tile reads `CONFIRM CLEAR`, the pattern is unchanged, and after 3 s it reads `CLEAR` again. Click CLEAR twice within 3 s: all 16 steps become C3 NOTE. Close the window.

- [ ] **Step 6: Update the design contract**

Components, Action row: append "A destructive action takes two clicks: CLEAR relabels to CONFIRM CLEAR on `SURFACE_HOVER` for 3 s; a second click within that window acts."

- [ ] **Step 7: Commit**

```bash
git add plugin303/src/gui/panels/sequencer_panel.rs .claude/skills/gui-visual-audit/references/design-contract.md
git commit -m "fix(gui): CLEAR asks for a second click before wiping the pattern (#9)"
```

---

### Task 7: Knob readouts come from the parameter (flagged CUTOFF)

**Files:**
- Create: `plugin303/src/value_text.rs`
- Modify: `plugin303/src/lib.rs:5-8` (`pub mod value_text;`)
- Modify: `plugin303/src/dsp/voice.rs:400-434, 932-959, 1493, 1575, 1584, 1605, 1819`
- Modify: `plugin303/src/params.rs:504-618, 714-815`
- Modify: `plugin303/src/gui/widgets/knob.rs:9-38, 146-253, 284-364`
- Modify: `plugin303/src/gui/panels/top_controls.rs:106-111`
- Modify: `plugin303/src/gui/panels/souped_panel.rs:135-140`
- Create: `plugin303/tests/param_text.rs`
- Modify: `.claude/skills/gui-visual-audit/references/design-contract.md` (Components, Knob)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces:
  - `plugin303::dsp::voice::{decay_ms(k: f32) -> f32, adv_base_cutoff_hz(k: f32) -> f32, classic_base_cutoff_hz(k: f64) -> f64, adv_glide_ms(k: f32) -> f32, calculate_classic_slide_tau_ms(topology: CircuitTopology, glide_knob: f32) -> f64}` (all `pub`).
  - `plugin303::value_text::{signed(v: f32) -> String, ms_or_s(ms: f32) -> String, hz(hz: f32) -> String, cutoff(k: f32, topology: CircuitTopology, advanced_env: bool) -> String, glide(k: f32, topology: CircuitTopology, advanced_env: bool) -> String}`.
  - `Knob303::value_text(self, text: String) -> Self`.

Evidence: `knob.rs:194-250` formats by label substring. CUTOFF prints `300·50^f` Hz; the voice uses `lin_to_exp(f, 314, 2394)` classic (`voice.rs:959, 1605`) and `200·20^f` with ADV ENV (`voice.rs:1584`); ACCURATE forces ADV ENV off (`voice.rs:829`). DECAY prints `200 + 1800·f` ms; the voice uses `decay_map` (100 ms … 2.1 s, `voice.rs:401-434`). GLIDE prints `500·f` ms; the voice uses `10 + 400·f` ms with ADV ENV (`voice.rs:1493`) and `2·55^(f^1.5)` ms classic in FUN (`voice.rs:1819-1824`). SOUPED-UP DRIVE (`vcf_drive`, 0-24 dB) matches the `contains("DRIVE")` percent branch and prints `12%` at 12 dB. The host shows raw numbers for most of these (no `value_to_string`).

- [ ] **Step 1: Write the failing tests**

Create `plugin303/tests/param_text.rs`:

```rust
use nih_plug::prelude::{FloatParam, Param};
use plugin303::params::Plugin303Params;

fn text(param: &FloatParam, plain: f32) -> String {
    param.normalized_value_to_string(param.preview_normalized(plain), true)
}

/// The host and the knob show one string per value, and it says what the voice
/// does with that value.
#[test]
fn knob_params_print_what_the_voice_does() {
    let p = Plugin303Params::default();
    // DECAY uses the voice's decay table: 100 ms .. 2.1 s.
    assert_eq!(text(&p.decay, 0.0), "100 ms");
    assert_eq!(text(&p.decay, 0.5), "400 ms");
    assert_eq!(text(&p.decay, 1.0), "2.1 s");
    // VCF drive is dB, not a percentage.
    assert_eq!(text(&p.vcf_drive, 12.0), "12.0 dB");
    // Bipolar values are signed, +0.0 on the detent.
    assert_eq!(text(&p.tuning, 25.0), "+25.0 ct");
    assert_eq!(text(&p.tuning, 0.0), "+0.0 ct");
    assert_eq!(text(&p.osc1_pitch, -12.0), "-12.0 st");
    assert_eq!(text(&p.volume, -6.0), "-6.0 dB");
    // Unipolar amounts are percentages of their range.
    assert_eq!(text(&p.resonance, 0.5), "50%");
    assert_eq!(text(&p.osc1_pw, 0.5), "50%");
    assert_eq!(text(&p.clipper_drive, 0.5), "50%");
}
```

Create `plugin303/src/value_text.rs` with only the tests for now, so they fail to compile against missing functions:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::CircuitTopology::{Accurate, Fun};

    #[test]
    fn ms_switches_to_seconds_at_the_rounding_boundary() {
        // `{:.0}` would print 999.6 ms as "1000 ms".
        assert_eq!(ms_or_s(999.4), "999 ms");
        assert_eq!(ms_or_s(999.6), "1.0 s");
        assert_eq!(ms_or_s(5.51), "5.5 ms");
        assert_eq!(ms_or_s(2100.0), "2.1 s");
    }

    #[test]
    fn signed_reads_plus_zero_on_the_detent() {
        assert_eq!(signed(0.04), "+0.0");
        assert_eq!(signed(-0.04), "+0.0");
        assert_eq!(signed(-3.5), "-3.5");
        assert_eq!(signed(25.0), "+25.0");
    }

    #[test]
    fn cutoff_follows_the_mode_the_voice_uses() {
        assert_eq!(cutoff(0.0, Fun, false), "314 Hz");
        assert_eq!(cutoff(0.5, Fun, false), "867 Hz");
        assert_eq!(cutoff(1.0, Fun, false), "2.4 kHz");
        assert_eq!(cutoff(0.0, Fun, true), "200 Hz");
        assert_eq!(cutoff(1.0, Fun, true), "4.0 kHz");
        // ACCURATE forces ADV ENV off in the voice.
        assert_eq!(cutoff(1.0, Accurate, true), "2.4 kHz");
    }

    #[test]
    fn glide_follows_the_mode_the_voice_uses() {
        assert_eq!(glide(0.4, Fun, true), "170 ms");
        assert_eq!(glide(0.4, Fun, false), "5.5 ms");
        assert_eq!(glide(1.0, Fun, false), "110 ms");
    }
}
```

Add `pub mod value_text;` to `plugin303/src/lib.rs` after `pub mod params;`.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p plugin303 --release --lib value_text` and `cargo test -p plugin303 --release --test param_text`
Expected: compile errors (`ms_or_s`, `signed`, `cutoff`, `glide` not found); `param_text` fails, for example `left: "0.5"` vs `right: "400 ms"`.

- [ ] **Step 3: Extract the DSP mappings (bit-exact)**

In `voice.rs`:
- Move the body of `Voice::decay_map` (lines 401-434) to a free function above `impl Voice`, and delete `decay_map`:

```rust
/// Decay time in ms for DECAY knob position `k` (0..=1): 100 ms .. 2.1 s, finer
/// between 200 and 700 ms. Classic and ADV ENV share it; so does the DECAY text.
pub fn decay_ms(k: f32) -> f32 {
    const P: [(f32, f32); 13] = [
        (0.0, 100.0),
        (0.1, 150.0),
        (0.2, 200.0),
        (0.3, 250.0),
        (0.4, 300.0),
        (0.5, 400.0),
        (0.6, 500.0),
        (0.7, 600.0),
        (0.8, 700.0),
        (0.85, 1000.0),
        (0.9, 1400.0),
        (0.95, 1750.0),
        (1.0, 2100.0),
    ];
    let mut lo = 0usize;
    let mut hi = P.len() - 1;
    while lo + 1 < hi {
        let mid = (lo + hi) / 2;
        if P[mid].0 < k {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let (x0, y0) = P[lo];
    let (x1, y1) = P[hi];
    if x1 - x0 < 1e-6 {
        y0
    } else {
        y0 + (y1 - y0) * (k - x0) / (x1 - x0)
    }
}
```

- Replace `Self::decay_map(` with `decay_ms(` at lines 932, 938, 945 and 1575.
- Add next to `lin_to_exp`:

```rust
/// Base VCF cutoff of CUTOFF position `k` with ADV ENV: 200 Hz .. 4 kHz.
#[inline]
pub fn adv_base_cutoff_hz(k: f32) -> f32 {
    200.0 * 20.0f32.powf(k)
}

/// Base VCF cutoff of CUTOFF position `k` in classic mode: 314 Hz .. 2394 Hz.
#[inline]
pub fn classic_base_cutoff_hz(k: f64) -> f64 {
    lin_to_exp(k, 314.0, 2394.0)
}

/// Pitch-glide time constant in ms of GLIDE position `k` with ADV ENV.
#[inline]
pub fn adv_glide_ms(k: f32) -> f32 {
    10.0 + k * 400.0
}
```

- Line 959: `let cutoff_hz = classic_base_cutoff_hz(cutoff as f64);`
- Line 1493: `let glide_time_ms = adv_glide_ms(self.glide_knob);`
- Line 1584: `let base_cutoff_hz = adv_base_cutoff_hz(self.cutoff_knob); // 200 Hz to 4000 Hz`
- Line 1605: `let base_cutoff_hz = classic_base_cutoff_hz(self.cutoff_knob as f64);`
- Line 1819: make `calculate_classic_slide_tau_ms` `pub` and add a doc line: `/// Classic slide time constant in ms: 2·55^(k^1.5) in FUN, the hardware 22 ms in ACCURATE.`

- [ ] **Step 4: Implement `value_text.rs`**

Above the tests:

```rust
//! Display text for parameter values. The host's `value_to_string` and the
//! knob readouts both come from here, so they cannot disagree with each other
//! or with the voice's mappings in `dsp::voice`.

use crate::dsp::voice;
use crate::params::CircuitTopology;

/// One decimal with a sign, `+0.0` on the centre detent. The unit comes from
/// the parameter's `with_unit`.
pub fn signed(v: f32) -> String {
    if v.abs() < 0.05 {
        "+0.0".to_string()
    } else {
        format!("{v:+.1}")
    }
}

/// Milliseconds; seconds from 999.5 ms, where `{:.0}` would round to "1000 ms";
/// one decimal below 10 ms.
pub fn ms_or_s(ms: f32) -> String {
    if ms >= 999.5 {
        format!("{:.1} s", ms / 1000.0)
    } else if ms < 9.95 {
        format!("{ms:.1} ms")
    } else {
        format!("{ms:.0} ms")
    }
}

/// Hz below 1 kHz, kHz with one decimal above.
pub fn hz(hz: f32) -> String {
    if hz >= 999.5 {
        format!("{:.1} kHz", hz / 1000.0)
    } else {
        format!("{hz:.0} Hz")
    }
}

/// The voice reads ADV ENV only in FUN (`voice.rs` forces it off in ACCURATE).
fn adv_env_live(topology: CircuitTopology, advanced_env: bool) -> bool {
    topology == CircuitTopology::Fun && advanced_env
}

/// CUTOFF readout: the base cutoff the voice uses in the active mode.
pub fn cutoff(k: f32, topology: CircuitTopology, advanced_env: bool) -> String {
    let base = if adv_env_live(topology, advanced_env) {
        voice::adv_base_cutoff_hz(k)
    } else {
        voice::classic_base_cutoff_hz(k as f64) as f32
    };
    hz(base)
}

/// GLIDE readout: the pitch-glide time constant the voice uses in the active mode.
pub fn glide(k: f32, topology: CircuitTopology, advanced_env: bool) -> String {
    let ms = if adv_env_live(topology, advanced_env) {
        voice::adv_glide_ms(k)
    } else {
        voice::calculate_classic_slide_tau_ms(topology, k) as f32
    };
    ms_or_s(ms)
}
```

- [ ] **Step 5: Give the knob params their text**

In `params.rs`, add `use nih_plug::formatters;` if not already imported (check `grep -n "formatters" plugin303/src/params.rs`), and `use std::sync::Arc;` if missing. Define one helper above `impl Default for Plugin303Params`:

```rust
/// Text of a 0..=1 amount: "50%". Typed host input "50" or "50%" parses back.
fn percent(param: FloatParam) -> FloatParam {
    param
        .with_unit("%")
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}
```

Then change the constructors:

| Param | New builder chain |
|---|---|
| `tuning` | `.with_unit(" ct").with_value_to_string(Arc::new(crate::value_text::signed))` |
| `cutoff` | wrap in `percent(…)` (the host gets a percentage; the knob shows Hz via `value_text`, Step 6) |
| `resonance`, `env_mod`, `accent`, `osc2_vol`, `osc1_pw`, `sub_vol`, `sweep_pos`, `reso_comp`, `glide` | wrap in `percent(…)` |
| `decay` | `.with_value_to_string(Arc::new(\|v\| crate::value_text::ms_or_s(crate::dsp::voice::decay_ms(v))))` |
| `volume` | keep `.with_unit(" dB")`, add `.with_value_to_string(Arc::new(crate::value_text::signed))` |
| `osc1_pitch`, `osc2_pitch` | keep `.with_unit(" st")`, add `.with_value_to_string(Arc::new(crate::value_text::signed))` |
| `vcf_drive` | keep `.with_unit(" dB")`, add `.with_value_to_string(Arc::new(\|v\| format!("{v:.1}")))` |
| `clipper_in_gain`, `clipper_out_gain`, `clipper_tilt` | keep `.with_unit(" dB")`, add `.with_value_to_string(Arc::new(crate::value_text::signed))` |
| `comp_drift`, `recalibration_scale`, `clipper_drive`, `clipper_threshold`, `clipper_bass`, `clipper_tone`, `clipper_mix`, `clipper_dynamics` | replace `.with_value_to_string(std::sync::Arc::new(\|v\| format!("{:.0} %", v * 100.0)))` with `percent(…)` around the `FloatParam::new(…)` |

The `\|` in the table escapes the table's column separator; the code is `|v| …`. The `decay` constructor in full:

```rust
            decay: FloatParam::new("Decay", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| {
                    crate::value_text::ms_or_s(crate::dsp::voice::decay_ms(v))
                })),
```

Example:

```rust
            resonance: percent(FloatParam::new(
                "Resonance",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )),
```

- [ ] **Step 6: The knob shows the parameter's text**

In `knob.rs`:
- Add a field `value_text: Option<String>` to `Knob303`, initialise it to `None` in `new`, and add:

```rust
    /// Readout text when the parameter's own text cannot know it: a value whose
    /// meaning depends on another parameter (CUTOFF and GLIDE follow ADV ENV).
    pub fn value_text(mut self, text: String) -> Self {
        self.value_text = Some(text);
        self
    }
```

- Delete lines 146-149 (`raw_str`, `plain_value` and their comment). `let current_angle = …` stays.
- Replace lines 192-253 (`// 5. Monospace Value Badge` through the end of the `value_str` `if let … else` expression) with:

```rust
            // 5. Monospace value badge: the parameter's own text, the same
            // string the host shows.
            let value_y = label_y + 11.0;
            let value_str = match &self.value_text {
                Some(text) => text.clone(),
                None => self.param.to_string(),
            };
```

- Delete `parse_plain_value`, `format_zero_centered`, `format_decay_label` and the `parse_tests` and `decay_tests` modules (lines 284-364). Keep `compute_needle_angle` and `needle_tests`.

In `top_controls.rs`, the CUTOFF knob (lines 106-111):

```rust
                render_knob(
                    Knob303::new(&params.cutoff, setter, "CUTOFF", theme::KNOB_RADIUS_LARGE)
                        .value_text(crate::value_text::cutoff(
                            params.cutoff.unmodulated_plain_value(),
                            params.topology.value(),
                            params.advanced_env.value(),
                        )),
                );
```

In `souped_panel.rs`, the GLIDE knob (lines 135-140):

```rust
                        render_knob(
                            Knob303::new(&params.glide, setter, "GLIDE", theme::KNOB_RADIUS_MEDIUM)
                                .value_text(crate::value_text::glide(
                                    params.glide.unmodulated_plain_value(),
                                    params.topology.value(),
                                    params.advanced_env.value(),
                                )),
                        );
```

Keep the radius argument each call used before (check the original lines; VOICE knobs use `KNOB_RADIUS_LARGE`).

- [ ] **Step 7: Run the tests**

```bash
cargo test -p plugin303 --release --lib value_text
cargo test -p plugin303 --release --test param_text
cargo test -p plugin303 --release
```

Expected: all pass, including the spectral and no-alloc gates: the DSP edits are extractions of the same arithmetic.

- [ ] **Step 8: Check the readouts on screen**

```bash
bash scripts/render_gui_offscreen.sh
python3 scripts/gui_qa_measure.py crop target/gui-qa-offscreen/knobs.png --box 8,62,1614,262 --scale 1 -o /tmp/voice_knobs.png
python3 scripts/gui_qa_measure.py crop target/gui-qa-offscreen/latest.png --box 8,268,1614,428 --scale 1 -o /tmp/souped.png
python3 scripts/gui_qa_measure.py knobs target/gui-qa-offscreen/knobs.png
```

Read both PNGs. Expected in `knobs`: CUTOFF `1.4 kHz` (0.75, classic; was `5.6 kHz`), TUNING `+25.0 ct`, DECAY `400 ms`, VOLUME `+0.0 dB`. In `latest` SOUPED-UP: DRIVE `0.0 dB`, GLIDE `5.5 ms`, PW `50%`. Every value fits its pill (no text crossing the well edge). The `knobs` report shows no arc/pointer mismatch.

- [ ] **Step 9: Update the design contract**

Components, Knob row: append "The value text is the parameter's own text (`value_to_string`, the string the host shows). A knob whose meaning depends on another parameter passes `value_text()` computed from the voice's mapping (`value_text::cutoff`, `value_text::glide`)."

- [ ] **Step 10: Commit**

```bash
git add plugin303/src .claude/skills/gui-visual-audit/references/design-contract.md plugin303/tests/param_text.rs
git commit -m "fix(gui): knob readouts come from the parameter, not the label

The knob guessed a format from its label: CUTOFF printed 300*50^f
Hz while the voice runs 314..2394 Hz (classic) or 200..4000 Hz
(ADV ENV), DECAY ignored the decay table, and DRIVE printed dB as
percent. Params now carry value_to_string; CUTOFF and GLIDE pass
text built from the voice's own mapping functions."
```

---

### Task 8: Tooltips on abbreviated controls (#11)

**Files:**
- Modify: `plugin303/src/gui/widgets/knob.rs` (tooltip builder)
- Modify: `plugin303/src/gui/panels/souped_panel.rs:16-33, 123-176`
- Modify: `plugin303/src/gui/panels/output_clipper.rs:19-60, 79-130`
- Modify: `plugin303/src/gui/panels/tweaks_panel.rs` (TOL combo, DRIFT, RECAL)
- Modify: `plugin303/src/gui/panels/generator_panel.rs:30-33, 149-167, 305-319`
- Modify: `plugin303/tests/gui_tooltips.rs`
- Modify: `.claude/skills/gui-visual-audit/references/design-contract.md` (Components)

**Interfaces:**
- Consumes: `Rig` (Task 2); `gui_tooltips.rs` (Task 3).
- Produces: `Knob303::tooltip(self, text: &'a str) -> Self`; `generator_panel::field_drag` returns `Response`.

- [ ] **Step 1: Write the failing test**

Add these imports to the top of `plugin303/tests/gui_tooltips.rs`:

```rust
use plugin303::gui::panels::{output_clipper, souped_panel};
use plugin303::gui::BANK_ROW_H;
use std::collections::HashSet;
```

Append to the end of the file:

```rust
/// Every enabled control the device draws shows a tooltip on hover.
fn assert_controls_explain(rig: &mut Rig, add: &mut dyn FnMut(&mut Ui), at_least: usize) {
    let baseline: HashSet<String> = rig.run(2, None, add).into_iter().map(|(t, _)| t).collect();
    let targets: Vec<Rect> = rig
        .widgets()
        .into_iter()
        .filter(|w| w.enabled && (w.sense.senses_click() || w.sense.senses_drag()))
        .map(|w| w.rect)
        .collect();
    assert!(
        targets.len() >= at_least,
        "expected at least {at_least} controls, found {targets:?}"
    );
    for rect in targets {
        let texts = rig.hover(rect, add);
        assert!(
            texts.iter().any(|t| !baseline.contains(t)),
            "control at {rect:?} shows no tooltip"
        );
    }
}

fn left_column_width() -> f32 {
    split_columns(GUI_WIDTH as f32 - 2.0 * MARGIN).0
}

/// The engine devices label their controls with abbreviations (MIX2, R-COMP,
/// DYN, …); each control says what it is on hover (round-20 #11).
#[test]
fn engine_device_controls_explain_themselves() {
    let _lock = ENV_LOCK.lock().unwrap();
    let params = Plugin303Params::default(); // FUN, clipper on
    let host = DummyGuiContext;
    let setter = ParamSetter::new(&host);

    let mut rig = Rig::new(vec2(left_column_width(), BANK_ROW_H + TOOLTIP_ROOM));
    // 9 knobs + VCA ENV + VCF TAPS.
    assert_controls_explain(
        &mut rig,
        &mut |ui| souped_panel::render_souped_panel(ui, &params, &setter),
        11,
    );

    let mut rig = Rig::new(vec2(left_column_width(), BANK_ROW_H + TOOLTIP_ROOM));
    // 9 knobs + ON + OS + DIODE + LINK.
    assert_controls_explain(
        &mut rig,
        &mut |ui| output_clipper::render_output_clipper(ui, &params, &setter),
        13,
    );

    let mut rig = Rig::new(vec2(right_column_width(), TWEAKS_H + TOOLTIP_ROOM));
    // CALIBRATE + TOL + AGE + NOISE + CMP SEED + DRIFT + RECAL.
    assert_controls_explain(
        &mut rig,
        &mut |ui| tweaks_panel::render_tweaks_panel(ui, &params, &setter),
        7,
    );
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p plugin303 --release --test gui_tooltips engine_device_controls_explain_themselves`
Expected: FAIL, "control at Rect … shows no tooltip" on the first SOUPED-UP knob.

- [ ] **Step 3: Knob tooltip builder**

In `knob.rs` add a field `tooltip: Option<&'a str>` (initialised `None` in `new`) and:

```rust
    /// Hover text: what the knob does. Device banks use short labels (MIX2,
    /// R-COMP, DYN), so their knobs carry one (round-20 #11).
    pub fn tooltip(mut self, text: &'a str) -> Self {
        self.tooltip = Some(text);
        self
    }
```

At the end of `ui`, replace `response` with:

```rust
        match self.tooltip {
            Some(text) => response.on_hover_text(text),
            None => response,
        }
```

- [ ] **Step 4: Add the tooltips**

SOUPED-UP (`souped_panel.rs`), chain `.tooltip(…)` onto each `Knob303::new(…)`:

| Knob | Tooltip |
|---|---|
| DRIVE | `"VCF input drive (dB)"` |
| SUB | `"Sub-oscillator level"` |
| GLIDE | `"Pitch glide time on slid notes"` |
| PITCH1 | `"Oscillator 1 pitch offset (semitones)"` |
| PITCH2 | `"Oscillator 2 pitch offset (semitones)"` |
| MIX2 | `"Oscillator 2 level"` |
| PW | `"Oscillator 1 pulse width"` |
| SWEEP | `"Oscillator 1 waveform morph, saw to square"` |
| R-COMP | `"Resonance gain compensation: makes up the level lost at high resonance"` |

Header switches: chain onto each `header_ui.add(SlideSwitch…)`:
- VCA ENV: `.on_hover_text("GATE: the VCA is open while the gate is held. ENV: the VCA follows the amplitude envelope.")`
- VCF TAPS: `.on_hover_text("Filter slope: 2, 3 or 4 poles")`

OUTPUT CLIPPER (`output_clipper.rs`):

| Control | Tooltip |
|---|---|
| ON (toggle) | `"Lit: the clipper is in the signal path."` |
| OS (switch) | `"Clipper oversampling: 4X aliases less and costs more CPU"` |
| DRIVE | `"Drive into the clipper"` |
| THRESH | `"Clipping threshold"` |
| BASS | `"Bass into the clipper"` |
| TONE | `"Clipper tone"` |
| IN | `"Gain into the clipper (dB)"` |
| OUT | `"Gain after the clipper (dB)"` |
| TILT | `"Spectral tilt after the clipper (dB)"` |
| DYN | `"Dynamics: loud passages duck the drive, so attacks pass through"` |
| MIX | `"Dry/wet mix of the clipper"` |

DIODE keeps its per-option tooltips (`switch.rs:141-145`). If the test reports the DIODE switch as having no tooltip, the hover lands on the switch body, not a slot: replace `.with_option_tooltips([…])` with `.on_hover_text("SI: silicon, symmetric clipping. GE: germanium, asymmetric clipping.")` on the `ui.add(BoolSwitch…)` response.

ANALOG TWEAKS (`tweaks_panel.rs`):
- TOL combo: bind the `ComboBox … .show_ui(…)` result and chain `.response.on_hover_text("Component tolerance: how far part values stray from nominal");`
- DRIFT: `.tooltip("Component drift: slow random wander of pitch, cutoff and resonance")`
- RECAL: `.tooltip("Recalibration: share of the component pitch error the VCO calibration removes")`

ACID generator (`generator_panel.rs`): make `field_drag` return the response:

```rust
/// Numeric field cell with configurable width.
fn field_drag<'a>(ui: &mut Ui, drag: DragValue<'a>, w: f32) -> Response {
    ui.add_sized(vec2(w, G_ROW_H), drag)
}
```

(add `Response` to the egui import), then chain:
- PULSES (k): `.on_hover_text("Euclidean pulses: notes spread evenly over STEPS");`
- STEPS (n): `.on_hover_text("Euclidean pattern length in steps");`
- ACC BIAS: `.on_hover_text("Chance that an accented note moves to the upper half of the scale");`
- SLD CHAIN: `ui.add(ToggleButton::new(&mut gen_params.slide_chaining, "SLD CHAIN")).on_hover_text("Lit: slides may follow slides. Unlit: never two slides in a row.");`

Every other `field_drag(…)` call ignores the returned `Response`; end those statements with `;` (they already do).

- [ ] **Step 5: Run the tests**

Run: `cargo test -p plugin303 --release --test gui_tooltips --test gui_text_contrast --test gui_layout_test`
Expected: PASS.

- [ ] **Step 6: Update the design contract**

Components: add a row `| Tooltip | every control | Every control in SOUPED-UP ENGINE, OUTPUT CLIPPER and ANALOG TWEAKS, and every abbreviated generator field, has a hover tooltip (`plugin303/tests/gui_tooltips.rs`). A disabled control uses on_disabled_hover_text to say why |`.

- [ ] **Step 7: Commit**

```bash
git add plugin303/src/gui plugin303/tests/gui_tooltips.rs .claude/skills/gui-visual-audit/references/design-contract.md
git commit -m "fix(gui): tooltips on abbreviated engine and generator controls (#11)"
```

---

### Task 9: Type drift: banner title, OCT RANGE words (#8, #12)

**Files:**
- Modify: `plugin303/src/gui/panels/souped_panel.rs:43`
- Modify: `plugin303/src/gui/panels/generator_panel.rs:14-16, 211-231`
- Modify: `.claude/skills/gui-visual-audit/references/design-contract.md` (Type)

**Interfaces:**
- Consumes: `field_label`, `field_drag`, `G_FIELD_W` in `generator_panel.rs`.
- Produces: nothing new. `G_OCT_FIELD_W` is deleted.

- [ ] **Step 1: Record the before state**

```bash
python3 scripts/gui_qa_measure.py crop target/gui-qa-offscreen/accurate.png --box 8,268,1614,428 --scale 1 -o /tmp/banner.png
python3 scripts/gui_qa_measure.py crop target/gui-qa-offscreen/generator.png --box 8,600,1614,804 --scale 1 -o /tmp/acid.png
```

Read both. Find the x range of "ACCURATE MODE ACTIVE" in `/tmp/banner.png` and run `python3 scripts/gui_qa_measure.py text target/gui-qa-offscreen/accurate.png --box <x0>,330,<x1>,370` around it. Expected: `h=15` (10.5 pt). `/tmp/acid.png` shows `MIN 0` / `MAX +1` in monospace wells.

- [ ] **Step 2: Banner title 9.5 pt**

`souped_panel.rs:43`: `let font_title = FontId::proportional(9.5);`

- [ ] **Step 3: OCT RANGE words outside the wells**

In `generator_panel.rs` delete `const G_OCT_FIELD_W: f32 = 44.0;` and replace the OCT RANGE row (lines 211-231) with:

```rust
                        ui.horizontal(|ui| {
                            field_label(ui, "OCT RANGE:", G2_LBL_W);
                            // Words are proportional captions; the wells hold
                            // numbers only (contract, Type; round-20 #12).
                            field_label(ui, "MIN", 20.0);
                            let oct_max = gen_params.octave_max;
                            field_drag(
                                ui,
                                DragValue::new(&mut gen_params.octave_min)
                                    .range(-2..=oct_max)
                                    .custom_formatter(|v, _| fmt_octave(v)),
                                G_FIELD_W,
                            );
                            field_label(ui, "MAX", 22.0);
                            let oct_min = gen_params.octave_min;
                            field_drag(
                                ui,
                                DragValue::new(&mut gen_params.octave_max)
                                    .range(oct_min..=2)
                                    .custom_formatter(|v, _| fmt_octave(v)),
                                G_FIELD_W,
                            );
                        });
```

- [ ] **Step 4: Test and render**

```bash
cargo test -p plugin303 --release --test gui_layout_test --test gui_text_contrast --test gui_tooltips
bash scripts/render_gui_offscreen.sh
```

Re-run the Step 1 crops and the `text --box`. Expected: banner title `h` 13-14 px; OCT RANGE reads `MIN` `[0]` `MAX` `[+1]` with the words in the proportional caption face, inside the OCTAVE BEHAVIOR column (no overlap with the separator).

- [ ] **Step 5: Update the design contract**

Type table: change the first row to `| App bar identity (BASS SYNTH, MONO), ACCURATE banner title | 9.5 pt | proportional |`.

- [ ] **Step 6: Commit**

```bash
git add plugin303/src/gui .claude/skills/gui-visual-audit/references/design-contract.md
git commit -m "fix(gui): banner title on the type scale, OCT RANGE words out of the wells (#8, #12)"
```

---

### Task 10: Full verification

**Files:** none changed unless a check fails.

- [ ] **Step 1: All gates**

```bash
cargo test -p plugin303 --release 2>&1 | grep -E "test result|FAILED|panicked"
cargo clippy -p plugin303 --release -- -D warnings
cargo build --release -p gui_render
python3 -m pytest tests/test_gui_qa_measure.py tests/test_crop_gui_sections.py -q
ruff check scripts/ tests/
```

Expected: every suite green; test counts ≥ the Task 0 baseline plus the new tests (`gui_text_contrast` +1, `gui_tooltips` +5, `param_text` +1, `value_text` +4, `toggle` +2, `sequencer_panel` +1); clippy clean.

- [ ] **Step 2: Render and measure**

```bash
bash scripts/render_gui_offscreen.sh
python3 scripts/gui_qa_measure.py report target/gui-qa-offscreen > /tmp/r20_after.txt
grep -c "BELOW 3:1" /tmp/r20_after.txt
```

Expected: 0 `BELOW 3:1` lines. In the report's geometry section for every state: 10 cards, every gap 6 px, seam 1614 | 1620 (landmarks unchanged).

- [ ] **Step 3: Per-issue checks**

| Issue | Check | Expected |
|---|---|---|
| #1 | `gui_text_contrast` green; read `accurate.png` ANALOG TWEAKS | labels `TEXT_MUTED`, values in `WELL_BG` wells, status `FUN MODE ONLY`, no CALIBRATE |
| #2 | `ls target/gui-qa-offscreen/*.png`; read `latest.png` app bar | no `zoom75`; no ZOOM combo |
| #3 | `gui_tooltips` green | ACCURATE hints on every tweaks control and QUALITY |
| #4 | Task 4 Step 8 snippet | `latest` orange about 46 k px; GENERATION 0 |
| #5 | read `latest.png` TRANSPORT | toggles carry an LED slot; actions do not |
| #6 | Task 5 Step 1 probes | `latest` step 01 `#262626`; `running` step 04 `#2F2F2F`; LED 8 px |
| #7 | `python3 scripts/gui_qa_measure.py edges target/gui-qa-offscreen/latest.png --box 1760,330,2000,530` | one right edge (1915 ± 1) for all four fields |
| #8 | Task 9 Step 4 | banner title h 13-14 px |
| #9 | Task 6 Step 5 smoke | first click arms, second clears |
| #10 | `gui_tooltips`; read `latest.png` ENVELOPES | both banks dimmed with ADV ENV off; `advanced.png` both live |
| #11 | `gui_tooltips` green | every engine control has a tooltip |
| #12 | Task 9 Step 4 | `MIN`/`MAX` are captions |
| Flagged knob text | Task 7 Step 8 | CUTOFF `1.4 kHz` in `knobs` |

- [ ] **Step 4: DAW smoke (human)**

Follow `docs/DAW_SMOKE_TEST.md` for the editor: open the plugin, toggle TOPOLOGY, hover a dimmed ANALOG TWEAKS field (hint shows), start the host transport with SYNC lit (RUN LED, playhead plate), click CLEAR once and twice. Record the result in the PR description.

- [ ] **Step 5: Record the round status**

Run the `/gui-visual-audit` skill for round 21 on the fresh offscreen set; it reports each round-20 issue as FIXED or OPEN. Open the pull request only when that audit lists no HIGH item.
