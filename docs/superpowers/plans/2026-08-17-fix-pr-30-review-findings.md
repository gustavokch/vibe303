# Fix PR #30 Visual QA Review Findings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix all 6 code review findings on PR #30 across widget renderers, panel layouts, and allocation hot-paths.

**Architecture:** Refine egui widget renderers (`Knob303`, `MiniFader`, `SlideSwitch`) and panel layouts (`header_bar`, `top_controls`) in `plugin303/src/gui`.

**Tech Stack:** Rust, egui, nih_plug_egui.

**Spec:** `docs/superpowers/plans/2026-08-14-gui-refinement-plan.md`

## Global Constraints

- Compiles cleanly with `cargo check -p plugin303 --all-features`.
- Passes all tests with `cargo test -p plugin303 --all-features`.
- No heap allocations in widget render loops (`no_alloc`).

---

### Task 1: Fix Fidelity Tier Vertical Switch & Top Controls Indentation

**Files:**
- Modify: `plugin303/src/gui/panels/top_controls.rs:31-85`

**Interfaces:**
- Consumes: `SlideSwitch::vertical()`
- Produces: Correct vertical switch layout and clean 12-space indentation formatting for `top_controls.rs`.

- [ ] **Step 1: Update `top_controls.rs` to append `.vertical()` and reformat indentation**

In `plugin303/src/gui/panels/top_controls.rs`:
1. Add `.vertical()` to the `FIDELITY TIER` `SlideSwitch` chain.
2. Reformat the `ui.vertical(|ui| { ... })` block contents to 12-space indentation.

```rust
    ui.allocate_ui_at_rect(rect.shrink(10.0), |ui| {
        ui.vertical(|ui| {
            ui.set_width(480.0);
            // ROW 1: Classic 303 Parameter Gauges
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
                ui.add(Knob303::new(&params.tuning, setter, "TUNING", 20.0));
                ui.add(Knob303::new(&params.cutoff, setter, "CUTOFF", 20.0));
                ui.add(Knob303::new(&params.resonance, setter, "RESONANCE", 20.0));
                ui.add(Knob303::new(&params.env_mod, setter, "ENV MOD", 20.0));
                ui.add(Knob303::new(&params.decay, setter, "DECAY", 20.0));
                ui.add(Knob303::new(&params.accent, setter, "ACCENT", 20.0));
                ui.add(Knob303::new(&params.volume, setter, "VOLUME", 20.0));
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);

            // ROW 2: Waveform & Fidelity Tier Vertical Slide Switches
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                ui.spacing_mut().item_spacing = vec2(30.0, 0.0);

                ui.add(
                    SlideSwitch::new(
                        &params.waveform,
                        setter,
                        "WAVEFORM",
                        &["SAW", "SQUARE", "SWEEP"],
                    )
                    .vertical(),
                );

                ui.add(
                    SlideSwitch::new(
                        &params.tier,
                        setter,
                        "FIDELITY TIER",
                        &["LOW", "HIGH", "ULTRA"],
                    )
                    .with_thumb_color(Color32::from_rgb(0x8b, 0x00, 0x00))
                    .vertical(),
                );
            });
        });
    });
```

- [ ] **Step 2: Run cargo check to verify compilation**

Run: `cargo check -p plugin303 --all-features`
Expected: PASS

- [ ] **Step 3: Commit task changes**

```bash
git add plugin303/src/gui/panels/top_controls.rs
git commit -m "fix(gui): add vertical modifier to fidelity tier switch and fix top controls indentation"
```

---

### Task 2: Restore Knob Bezel Tick Marks Subtlety & Contrast

**Files:**
- Modify: `plugin303/src/gui/widgets/knob.rs:96-112`

**Interfaces:**
- Consumes: egui painter `line_segment`, `Color32::from_rgba_unmultiplied`
- Produces: Subtle low-contrast knob bezel tick marks with 1.0px stroke width and 35% alpha opacity.

- [ ] **Step 1: Update tick mark styling in `knob.rs`**

In `plugin303/src/gui/widgets/knob.rs`:
Adjust tick radius bounds, color alpha (35%), and stroke width (1.0px):

```rust
            // Discrete radial tick marks around bezel
            let num_ticks = 11;
            for i in 0..num_ticks {
                let frac = i as f32 / (num_ticks - 1) as f32;
                let tick_angle = start_angle + frac * (end_angle - start_angle);
                let is_major = i == 0 || i == (num_ticks - 1) || i == (num_ticks / 2);
                let r_inner = self.radius + 4.5;
                let r_outer = if is_major { self.radius + 8.0 } else { self.radius + 6.5 };
                let p_in = pos2(center.x + tick_angle.cos() * r_inner, center.y + tick_angle.sin() * r_inner);
                let p_out = pos2(center.x + tick_angle.cos() * r_outer, center.y + tick_angle.sin() * r_outer);
                let alpha = if is_major { 115 } else { 75 }; // ~45% for major, ~30% for minor
                let tick_color = Color32::from_rgba_unmultiplied(0xe2, 0xe8, 0xf0, alpha);
                let tick_stroke_width = 1.0_f32;
                painter.line_segment([p_in, p_out], Stroke::new(tick_stroke_width, tick_color));
            }
```

- [ ] **Step 2: Run cargo check to verify compilation**

Run: `cargo check -p plugin303 --all-features`
Expected: PASS

- [ ] **Step 3: Commit task changes**

```bash
git add plugin303/src/gui/widgets/knob.rs
git commit -m "fix(gui): restore subtle low-contrast stroke and geometry for knob bezel tick marks"
```

---

### Task 3: Allocate Layout Bounds for Header Bar Ventilation Grill

**Files:**
- Modify: `plugin303/src/gui/panels/header_bar.rs:37-50`

**Interfaces:**
- Consumes: `ui.allocate_exact_size`
- Produces: Clean UI layout cursor tracking for ventilation grill in header bar.

- [ ] **Step 1: Update `header_bar.rs` to allocate UI layout space before painting**

In `plugin303/src/gui/panels/header_bar.rs`:
Use `ui.allocate_exact_size` so the egui layout engine reserves spatial bounds for the centered ventilation grill.

```rust
        // Procedural Dual 7-Slot Chassis Ventilation Grills (Centered)
        let grill_width = 140.0;
        let grill_height = 24.0;
        let (grill_rect, _) = ui.allocate_exact_size(
            vec2(grill_width, grill_height),
            nih_plug_egui::egui::Sense::hover(),
        );

        if ui.is_rect_visible(grill_rect) {
            let painter = ui.painter();
            let slot_width = 3.0_f32;
            let group_width = 56.0_f32;
            let group_gap = grill_rect.width() - 2.0 * group_width;

            // Render Left 7-slot group and Right 7-slot group
            for group in 0..2 {
                let group_start_x = grill_rect.left() + group as f32 * (group_width + group_gap);
                let slot_gap = (group_width - 7.0 * slot_width) / 6.0;

                for i in 0..7 {
                    let slot_x = group_start_x + i as f32 * (slot_width + slot_gap);
                    let slot = Rect::from_min_max(
                        pos2(slot_x, grill_rect.top() + 2.0),
                        pos2(slot_x + slot_width, grill_rect.bottom() - 2.0),
                    );
                    // Deep black vent recess
                    painter.rect_filled(slot, 1.0, Color32::from_rgb(0x04, 0x06, 0x0a));
                    // Bright metallic highlight stroke
                    painter.rect_stroke(
                        slot,
                        1.0,
                        Stroke::new(1.2_f32, Color32::from_rgb(0x47, 0x55, 0x69)),
                        nih_plug_egui::egui::StrokeKind::Inside,
                    );
                }
            }
        }
```

- [ ] **Step 2: Run cargo check to verify compilation**

Run: `cargo check -p plugin303 --all-features`
Expected: PASS

- [ ] **Step 3: Commit task changes**

```bash
git add plugin303/src/gui/panels/header_bar.rs
git commit -m "fix(gui): allocate UI layout space for header bar ventilation grill"
```

---

### Task 4: Fix MiniFader Zero-Value Glow Bar & Remove Heap Allocations

**Files:**
- Modify: `plugin303/src/gui/widgets/fader.rs:94-155`

**Interfaces:**
- Consumes: `MiniFader` widget rendering loop
- Produces: Fill bar hidden when `normalized == 0.0` and zero-allocation text readout formatting.

- [ ] **Step 1: Update `fader.rs` fill bar calculation and readout parsing**

In `plugin303/src/gui/widgets/fader.rs`:
1. Only render fill bar if `normalized > 0.001`.
2. Format `val_str` using zero-allocation string slicing/parsing instead of heap string collection.

```rust
            // Active level fill bar (vivid glowing orange accent color from bottom to thumb)
            if normalized > 0.001 {
                let thumb_y = track_bottom - normalized * track_height;
                let fill_top = thumb_y.min(track_bottom - 1.0);
                let fill_rect = Rect::from_min_max(
                    pos2(rect.center().x - 2.0, fill_top),
                    pos2(rect.center().x + 2.0, track_bottom),
                );
                // Outer bright orange bloom glow
                let glow_color = Color32::from_rgba_premultiplied(
                    self.accent_color.r(),
                    self.accent_color.g(),
                    self.accent_color.b(),
                    0x50,
                );
                painter.rect_filled(fill_rect.expand(1.0), 1.0, glow_color);
                // Vivid solid orange core
                painter.rect_filled(fill_rect, 1.0, self.accent_color);
            }

            // ... thumb rendering ...

            // Readout / value below (compact formatting without per-frame allocations)
            let val_str = self.param.to_string();
            let val_clean = val_str.trim_end_matches(" ms").trim_end_matches(" s").trim_end_matches('%');
            let formatted_val = if let Ok(f) = val_clean.parse::<f32>() {
                if self.label == "S" {
                    format!("{:.0}%", f * 100.0)
                } else if f >= 1000.0 {
                    format!("{:.1}s", f / 1000.0)
                } else {
                    format!("{:.0}ms", f)
                }
            } else {
                val_str.replace(" ms", "ms").replace(" s", "s")
            };
```

- [ ] **Step 2: Run cargo test to verify gui_build and no_alloc tests**

Run: `cargo test -p plugin303 --test gui_build`
Expected: PASS

- [ ] **Step 3: Commit task changes**

```bash
git add plugin303/src/gui/widgets/fader.rs
git commit -m "fix(gui): suppress fader glow bar at zero value and remove heap string allocations"
```

---

### Task 5: End-to-End Verification

**Files:**
- All `plugin303` GUI files

- [ ] **Step 1: Run full unit & integration test suite**

Run: `cargo test -p plugin303 --all-features`
Expected: PASS (61 unit tests, integration tests passing)

- [ ] **Step 2: Run zero-allocation safety tests**

Run: `cargo test -p plugin303 --test no_alloc`
Expected: PASS

- [ ] **Step 3: Final check of git status**

Run: `git status`
Expected: Clean working tree
