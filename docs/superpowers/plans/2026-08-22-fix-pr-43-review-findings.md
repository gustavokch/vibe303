# Fix PR #43 Review Findings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve all 5 code review findings identified on PR #43 across tests, widgets, panels, and generation logic.

**Architecture:** Harden test isolation with static mutex locking, accumulate fractional drag deltas in egui widget memory for responsive pitch edits, replace per-frame dynamic allocations with static arrays and `line_segment` primitives, pre-allocate UI lookup tables, and decouple mutation random stream seeds.

**Tech Stack:** Rust (edition 2021), `nih_plug` (vst3/clap audio plugin framework), `nih_plug_egui` / `egui` 0.31, `cargo test`.

**Spec:** Pull Request #43 Code Review (`https://github.com/gustavokch/vibe303/pull/43#issuecomment-5382187866`).

## Global Constraints

- Never allocate heap memory on real-time audio thread paths.
- Avoid per-frame heap allocations (`Vec`, dynamic `String` formatting) inside egui immediate mode GUI draw loops.
- Maintain test concurrency safety: test cases mutating global process environment variables must synchronize with a mutex.
- Preserve exact 1280x720 canvas layout and DSP invariants.

---

### Task 1: Guard Environment Variable Mutation in GUI Build Test

**Files:**
- Modify: `plugin303/tests/gui_build.rs:25-45`
- Test: `plugin303/tests/gui_build.rs`

**Interfaces:**
- Consumes: `std::sync::Mutex`, `plugin303::Plugin303`
- Produces: Thread-safe RAII-scoped environment test isolation

- [ ] **Step 1: Write the failing / strict concurrency test**

Verify that `test_qa_running_state_playhead_preserved` acquires a static mutex before setting `VIBE303_QA_STATE` and removes the variable in an RAII guard so parallel test threads are not poisoned even on panic.

Edit `plugin303/tests/gui_build.rs`:

```rust
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard<'a> {
    key: &'a str,
}

impl<'a> EnvGuard<'a> {
    fn set(key: &'a str, val: &str) -> Self {
        unsafe { std::env::set_var(key, val) };
        Self { key }
    }
}

impl<'a> Drop for EnvGuard<'a> {
    fn drop(&mut self) {
        unsafe { std::env::remove_var(self.key) };
    }
}

#[test]
fn test_qa_running_state_playhead_preserved() {
    use nih_plug::prelude::Plugin;
    let _lock = ENV_LOCK.lock().unwrap();
    let _guard = EnvGuard::set("VIBE303_QA_STATE", "running");

    let mut plugin = plugin303::Plugin303::default();
    assert_eq!(
        plugin.current_playing_step.load(std::sync::atomic::Ordering::Relaxed),
        3,
        "Running QA state must initialize playhead to step index 3"
    );
    plugin.reset();
    assert_eq!(
        plugin.current_playing_step.load(std::sync::atomic::Ordering::Relaxed),
        3,
        "Reset in running QA state must preserve active playhead index"
    );
}
```

- [ ] **Step 2: Run test suite to verify passes cleanly**

Run: `cargo test --test gui_build`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add plugin303/tests/gui_build.rs
git commit -m "test(gui): synchronize environment variable access in QA state test"
```

---

### Task 2: Fix Continuous Sub-Pixel Pitch Dragging in Sequencer Step Button

**Files:**
- Modify: `plugin303/src/gui/widgets/step_button.rs:180-205`
- Test: `plugin303/tests/gui_build.rs`

**Interfaces:**
- Consumes: `pitch_resp.drag_delta()`, `ui.ctx().data_mut()`, `egui::Id`
- Produces: Smooth multi-frame drag accumulation for step pitch nudging

- [ ] **Step 1: Implement drag accumulator in `step_button.rs`**

Update `render_step_strip` in `plugin303/src/gui/widgets/step_button.rs` around line 180 to accumulate drag delta across frames in egui temporary storage:

```rust
        let is_rest = step.gate == StepGate::Rest;
        let pitch_str = format_pitch(step.semitone, step.octave);
        let (pitch_rect, pitch_resp) = ui.allocate_exact_size(vec2(width, 15.0), Sense::click_and_drag());
        let pitch_resp = pitch_resp.on_hover_text("Click: piano roll | Drag/R-Click: nudge pitch");

        let drag_id = ui.make_persistent_id(format!("step_drag_{}", step_idx));

        if pitch_resp.clicked() {
            crate::gui::widgets::piano_roll_popup::toggle_open_state(
                ui.ctx(),
                step_idx,
                pitch_resp.rect,
            );
        } else if pitch_resp.secondary_clicked() {
            step.semitone = if step.semitone <= 0 { 12 } else { step.semitone - 1 };
            changed = true;
        } else if pitch_resp.dragged() {
            let dy = -pitch_resp.drag_delta().y;
            let mut accum: f32 = ui.ctx().data(|d| d.get_temp(drag_id)).unwrap_or(0.0);
            accum += dy;

            const PIXELS_PER_SEMITONE: f32 = 6.0;
            if accum.abs() >= PIXELS_PER_SEMITONE {
                let steps = (accum / PIXELS_PER_SEMITONE).trunc() as i8;
                if steps != 0 {
                    step.semitone = (step.semitone + steps).clamp(0, 12);
                    accum -= steps as f32 * PIXELS_PER_SEMITONE;
                    changed = true;
                }
            }
            ui.ctx().data_mut(|d| d.insert_temp(drag_id, accum));
        }

        if pitch_resp.drag_stopped() {
            ui.ctx().data_mut(|d| d.remove_temp::<f32>(drag_id));
        }
```

- [ ] **Step 2: Run GUI tests to verify build & responsiveness**

Run: `cargo test -p plugin303 --all-features`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add plugin303/src/gui/widgets/step_button.rs
git commit -m "fix(gui): accumulate drag delta across frames for step pitch nudge"
```

---

### Task 3: Eliminate Per-Frame Vec Allocations in Knob Arc Drawing

**Files:**
- Modify: `plugin303/src/gui/widgets/knob.rs:90-125`
- Test: `plugin303/tests/no_alloc.rs`, `plugin303/tests/gui_build.rs`

**Interfaces:**
- Consumes: `painter.line_segment`, `pos2`, `start_angle`, `end_angle`
- Produces: Zero-allocation rotary perimeter arc rendering

- [ ] **Step 1: Replace `Shape::line(Vec<Pos2>)` with segmented line drawing**

Edit `plugin303/src/gui/widgets/knob.rs` lines 90-125 to render arcs using consecutive `line_segment` calls with stack-allocated points:

```rust
            // 2. 270° Perimeter Track (135° to 405°)
            let start_angle = 135.0 * PI / 180.0;
            let end_angle = 405.0 * PI / 180.0;
            let arc_r = self.radius - 2.5;

            // Background inactive arc (24 segments, 0 heap allocations)
            const ARC_SEGMENTS: usize = 24;
            let mut prev_bg_pt = pos2(
                center.x + start_angle.cos() * arc_r,
                center.y + start_angle.sin() * arc_r,
            );
            for i in 1..=ARC_SEGMENTS {
                let t = i as f32 / ARC_SEGMENTS as f32;
                let a = start_angle + t * (end_angle - start_angle);
                let cur_pt = pos2(center.x + a.cos() * arc_r, center.y + a.sin() * arc_r);
                painter.line_segment([prev_bg_pt, cur_pt], Stroke::new(2.0_f32, theme::SURFACE_ELEVATED));
                prev_bg_pt = cur_pt;
            }

            // Active value arc
            let is_enabled = ui.is_enabled();
            let accent = if is_enabled {
                self.accent_color.unwrap_or(theme::ACCENT_ORANGE)
            } else {
                theme::TEXT_MUTED
            };
            if normalized > 0.001 {
                let mut prev_active_pt = pos2(
                    center.x + start_angle.cos() * arc_r,
                    center.y + start_angle.sin() * arc_r,
                );
                for i in 1..=ARC_SEGMENTS {
                    let t = i as f32 / ARC_SEGMENTS as f32;
                    let a = start_angle + t * normalized * (end_angle - start_angle);
                    let cur_pt = pos2(center.x + a.cos() * arc_r, center.y + a.sin() * arc_r);
                    painter.line_segment([prev_active_pt, cur_pt], Stroke::new(2.5_f32, accent));
                    prev_active_pt = cur_pt;
                }
            }
```

- [ ] **Step 2: Run compiler and test checks**

Run: `cargo test -p plugin303 --all-features`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add plugin303/src/gui/widgets/knob.rs
git commit -m "perf(gui): replace heap-allocated knob arc vectors with stack-based segments"
```

---

### Task 4: Static String Table for Zoom Preset Labels in Header Bar

**Files:**
- Modify: `plugin303/src/gui/panels/header_bar.rs:88-112`
- Test: `plugin303/tests/gui_build.rs`

**Interfaces:**
- Consumes: `ZOOM_PRESETS: &[(f32, &str)]`
- Produces: Allocation-free zoom selector combo box options

- [ ] **Step 1: Use static preset table in `header_bar.rs`**

Edit `plugin303/src/gui/panels/header_bar.rs`:

```rust
                        // 1. Zoom factor dropdown
                        let current_zoom = ui.ctx().zoom_factor();
                        const ZOOM_PRESETS: [(f32, &str); 5] = [
                            (0.75, "75%"),
                            (1.0, "100%"),
                            (1.25, "125%"),
                            (1.5, "150%"),
                            (2.0, "200%"),
                        ];
                        let zoom_label = ZOOM_PRESETS
                            .iter()
                            .find(|(z, _)| (current_zoom - z).abs() < 0.01)
                            .map(|(_, l)| *l)
                            .unwrap_or("100%");

                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing = vec2(0.0, 1.0);
                            ui.label(
                                nih_plug_egui::egui::RichText::new("ZOOM")
                                    .font(FontId::new(8.5, FontFamily::Proportional))
                                    .color(theme::TEXT_SECONDARY),
                            );
                            ComboBox::from_id_salt("header_zoom_selector")
                                .selected_text(zoom_label)
                                .width(54.0)
                                .show_ui(ui, |ui| {
                                    for &(preset, label) in &ZOOM_PRESETS {
                                        let is_selected = (current_zoom - preset).abs() < 0.01;
                                        if ui.selectable_label(is_selected, label).clicked() {
                                            ui.ctx().set_zoom_factor(preset);
                                        }
                                    }
                                });
                        });
```

- [ ] **Step 2: Run compiler and test checks**

Run: `cargo test -p plugin303 --all-features`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add plugin303/src/gui/panels/header_bar.rs
git commit -m "perf(gui): use static string table for zoom preset labels"
```

---

### Task 5: Decouple Mutation PRNG Stream in Acid Generator Panel

**Files:**
- Modify: `plugin303/src/gui/panels/generator_panel.rs:405-425`
- Test: `plugin303/tests/test_pattern_shift.rs`, `plugin303/tests/gui_build.rs`

**Interfaces:**
- Consumes: `AcidGenerator::mutate`, `gen_params.seed`
- Produces: Independent 64-bit Weyl sequence stepping for pattern mutation

- [ ] **Step 1: Use independent seed mixing constant for mutation**

Edit `plugin303/src/gui/panels/generator_panel.rs` line 411:

```rust
                            if ui.add(mutate_btn).clicked() {
                                if let Ok(mut seq_pat) = params.pattern.write() {
                                    let mode = params.seq_mode.value();
                                    let num_steps = params.seq_length.value() as u8;
                                    let mut pat = seq_pat.compile(num_steps, mode);
                                    // Independent Weyl sequence constant for mutation stream (distinct from NEW SEED)
                                    gen_params.seed = gen_params.seed.wrapping_add(0x517C_C1B7_2722_0A95);
                                    AcidGenerator::mutate(&mut pat, gen_params, gen_params.seed);

                                    for i in 0..16 {
                                        seq_pat.steps[i].accent = pat.pitches[i].accent;
                                        seq_pat.steps[i].slide = pat.pitches[i].slide;
                                        seq_pat.steps[i].semitone = pat.pitches[i].key;
                                        seq_pat.steps[i].octave = pat.pitches[i].octave;
                                        seq_pat.steps[i].gate = match pat.timing[i] {
                                            crate::dsp::sequencer::TimeEntry::Note => StepGate::Note,
                                            crate::dsp::sequencer::TimeEntry::Tie => StepGate::Tie,
                                            crate::dsp::sequencer::TimeEntry::Rest => StepGate::Rest,
                                        };
                                    }
                                }
                            }
```

- [ ] **Step 2: Run test suite to verify whole project**

Run: `cargo test --workspace --all-features`
Expected: PASS with 0 failures

- [ ] **Step 3: Commit**

```bash
git add plugin303/src/gui/panels/generator_panel.rs
git commit -m "fix(generator): use independent PRNG seed constant for pattern mutation"
```

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-22-fix-pr-43-review-findings.md`.

Two execution options:

1. **Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration
2. **Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach?
