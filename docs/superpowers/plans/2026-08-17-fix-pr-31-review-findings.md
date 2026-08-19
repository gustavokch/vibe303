# Fix PR 31 Review Findings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Address code review nits on PR #31 by displaying full 64-bit seed formatting and adding `DragValue` for precise/fractional BPM control.

**Architecture:** Update `generator_panel.rs` seed label formatting and `sequencer_panel.rs` tempo control row in `plugin303/src/gui/panels/`. Verify with `cargo check -p plugin303` and `cargo test -p plugin303`.

**Tech Stack:** Rust, nih_plug_egui, egui.

**Spec:** Review comments on PR #31 (https://github.com/gustavokch/vibe303/pull/31#issuecomment-5320454527)

## Global Constraints

- No behavioral regressions in parameters or DSP synthesis.
- Maintain egui styling and flat dark theme conventions (#282828 buttons, TEXT_SECONDARY labels).
- Run `cargo check -p plugin303` after edits.

---

### Task 1: Fix Seed Display Formatting in Generator Panel

**Files:**
- Modify: `plugin303/src/gui/panels/generator_panel.rs:122-130`

**Interfaces:**
- Consumes: `gen_params.seed: u64`
- Produces: `SEED: {:016X}` string display in UI

- [ ] **Step 1: Check existing seed display code**

Run: `cargo check -p plugin303`
Expected: PASS

- [ ] **Step 2: Update seed format string to 16 hex digits without masking**

In `plugin303/src/gui/panels/generator_panel.rs`, replace:
```rust
ui.label(
    nih_plug_egui::egui::RichText::new(format!(
        "SEED: {:08X}",
        gen_params.seed & 0xFFFF_FFFF
    ))
    .font(FontId::monospace(9.0))
    .color(theme::TEXT_MUTED),
);
```
With:
```rust
ui.label(
    nih_plug_egui::egui::RichText::new(format!(
        "SEED: {:016X}",
        gen_params.seed
    ))
    .font(FontId::monospace(9.0))
    .color(theme::TEXT_MUTED),
);
```

- [ ] **Step 3: Verify build passes**

Run: `cargo check -p plugin303`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add plugin303/src/gui/panels/generator_panel.rs
git commit -m "fix(gui): display full 64-bit seed hex string in generator panel"
```

---

### Task 2: Add DragValue to Sequencer Tempo Control

**Files:**
- Modify: `plugin303/src/gui/panels/sequencer_panel.rs:801-860`

**Interfaces:**
- Consumes: `params.seq_tempo: FloatParam`
- Produces: Interactive DragValue with step buttons for `seq_tempo`

- [ ] **Step 1: Replace static BPM label with DragValue control**

In `plugin303/src/gui/panels/sequencer_panel.rs`, update Row B (BPM Adjust) to:
```rust
let mut tempo_val = params.seq_tempo.value();
let dn_btn = nih_plug_egui::egui::Button::new("-")
    .fill(Color32::from_rgb(0x28, 0x28, 0x28))
    .min_size(vec2(20.0, 20.0));
if ui.add(dn_btn).clicked() {
    setter.begin_set_parameter(&params.seq_tempo);
    setter.set_parameter(
        &params.seq_tempo,
        (tempo_val - 1.0).clamp(40.0, 300.0),
    );
    setter.end_set_parameter(&params.seq_tempo);
}

if ui
    .add(
        DragValue::new(&mut tempo_val)
            .range(40.0..=300.0)
            .speed(0.1)
            .custom_formatter(|n, _| format!("{:.1}", n)),
    )
    .changed()
{
    setter.begin_set_parameter(&params.seq_tempo);
    setter.set_parameter(&params.seq_tempo, tempo_val);
    setter.end_set_parameter(&params.seq_tempo);
}

let up_btn = nih_plug_egui::egui::Button::new("+")
    .fill(Color32::from_rgb(0x28, 0x28, 0x28))
    .min_size(vec2(20.0, 20.0));
if ui.add(up_btn).clicked() {
    setter.begin_set_parameter(&params.seq_tempo);
    setter.set_parameter(
        &params.seq_tempo,
        (tempo_val + 1.0).clamp(40.0, 300.0),
    );
    setter.end_set_parameter(&params.seq_tempo);
}
```

- [ ] **Step 2: Verify build and test suite pass**

Run: `cargo check -p plugin303 && cargo test -p plugin303`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add plugin303/src/gui/panels/sequencer_panel.rs
git commit -m "fix(gui): add DragValue for precise fractional tempo input"
```
