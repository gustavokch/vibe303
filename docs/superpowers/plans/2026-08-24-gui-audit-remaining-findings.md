# GUI Audit Phase 4 — Remaining Findings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the four audit findings that still hold after the 2026-08-24 re-audit of fresh captures: sequencer layout breakage (#1, root-caused), playhead verification blocked by #1 (#11), mixed decimal precision (#12), and micro hit targets (#15) — without regressing the ten findings already fixed.

**Architecture:** The sequencer's diagonal cascade and clipped steps come from a single layout poison: `render_step_strip` allocates a background rect of `ui.available_height()` before drawing content, so every strip consumes the row's remaining height and pushes subsequent strips down and right. The fix replaces that allocation with a background-layer paint plus an explicitly sized 2x8 step grid that fills the bottom panel's ~361 px of vertical budget. Decimal consistency is a one-line formatter change. Verification uses the existing capture pipeline (`scripts/capture_standalone_gui.sh`) plus vision inspection, because egui geometry has no unit-test harness in this repo.

**Tech Stack:** Rust, nih-plug, egui (via nih_plug_egui), baseview standalone binary, macOS `screencapture` QA pipeline.

**Spec:** `audit-report.md` (16 findings, statuses re-verified 2026-08-24 against captures in `target/gui-qa/` taken from commit `9950377`). Re-audit verdicts this plan implements:

| # | Finding | Status after re-audit | Action |
|---|---------|----------------------|--------|
| 1 | Sequencer clipped / dead band | PARTIAL — dead band gone, but steps render as diagonal cascade; only steps 01–05(–06) visible; steps 06–16 clipped at window bottom | Fix (Task 1) |
| 2 | Dead column right of Tweaks | FIXED | Preserve |
| 3 | Accurate overlay band | FIXED | Preserve |
| 4 | Ragged panel bottoms | FIXED | Preserve |
| 5 | Knob arc vs pointer | FIXED (all 50% knobs show identical left-half arc) | Preserve |
| 6 | QUALITY ghost disabled | FIXED (uniform dim in ACCURATE) | Preserve |
| 7 | DYN/MIX dim with LINK | Intentional — 0%-value dim styling, consistent across all knobs (soup DRIVE/SUB/SWEEP/R-COMP at 0% dim identically) | Close, no change |
| 8 | PW arc (soup) | FIXED | Preserve |
| 9 | STOP color | RESOLVED (orange, consistent accent) | Preserve |
| 10 | Clipper title color | RESOLVED (white/gray) | Preserve |
| 11 | Playhead vs TIE ambiguity | Colors now distinct (ruby LED bar + ruby badge outline vs gold TIE), but tracking past step 05 impossible while #1 holds | Verify (Task 3) |
| 12 | Mixed decimal precision | HOLDS — DECAY shows `1.10 s` (2 dp) while all other unit formatters use 1 dp / integer | Fix (Task 2) |
| 13 | MEG/VEG naming | FIXED — legends `(Mod Envelope)` / `(Voice Envelope)` present | Preserve |
| 14 | WAVE orphan track | FIXED — gone | Preserve |
| 15 | Micro hit targets | PARTIAL — BPM +/- (24 px) and JIT (28x24) fixed; step-strip sub-controls still allocate 14 px / draw 12 px | Fix (Task 1) |
| 16 | Running feedback | Present and correct | Preserve |

Additional defect found in re-audit (same root cause as #1): in `running.png` a tall dark `SURFACE_ELEVATED` column (~56 x 250 px) floats above the active step badge — this is the poisoned background rect painting itself. It dies with the Task 1 fix. Also: the LED bar comment says "4.0 px" but the code allocates 2.0 px (folded into Task 1).

## Global Constraints

- Presentation layer only. Do not change parameter ranges, DSP behavior, persistence, pattern data model, or `SequencerStep`/`StepGate` definitions.
- Window size stays 1366x960 logical (retina captures are 2732x1984).
- Keep established accent semantics: playhead active = `ACCENT_RUBY` badge outline, playhead LED = `ACCENT_CYAN`, TIE = `ACCENT_GOLD`, NOTE = `ACCENT_CYAN`, ACC = `ACCENT_ORANGE`.
- Do not regress findings 2, 3, 4, 5, 6, 8, 9, 10, 13, 14 (see table above) or finding 16 (running feedback).
- Number formatting rule (applies to Task 2): one decimal for s / kHz / dB / ct / st / BPM; integer for ms / Hz / % / counts.
- Build gate: `cargo build --release -p plugin303 --bin standalone` completes with no new warnings (pre-existing `unused_variables` at `lib.rs:277` is tolerated).
- Visual gate: `scripts/capture_standalone_gui.sh` produces `target/gui-qa/{latest,running,accurate}.png`; verification is vision inspection of those PNGs (no egui unit-test harness exists in this repo — this is the project's established verification loop, used in place of TDD red/green for layout work).

---

### Task 1: Sequencer step grid — fix layout poison, restore all 16 steps, enlarge hit targets

**Files:**
- Modify: `plugin303/src/gui/widgets/step_button.rs` (whole file, ~344 lines)
- Modify: `plugin303/src/gui/panels/sequencer_panel.rs:27-58` (bottom panel frame + step row)

**Interfaces:**
- Consumes: `render_step_strip(ui, step_idx, step, is_current, is_running) -> bool` (existing signature, called from `sequencer_panel.rs:52`).
- Produces: `render_step_strip(ui, step_idx, step, is_current, is_running, cell_width: f32, cell_height: f32) -> bool` — new geometry parameters; the only caller is `sequencer_panel.rs`. Also produces a bottom panel that fills the window's remaining height (no dead band below the panel border).

**Geometry contract (acceptance numbers):**
- Grid: 2 rows x 8 columns, row-major (steps 01–08 top row, 09–16 bottom row).
- Cell width: `(left_column_available_width - 7.0 * 4.0) / 8.0` ≈ 116 px (left column spans the same ~956 px the 16-wide row used).
- Cell height: LED bar 4 + number header 16 + 4 control rows x 30 = 140 px.
- Row gap 12, grid total ≈ 292; panel inner margins as today (top 4, bottom 0, sides 8) plus `set_min_height(available_height - 12.0)` on the frame content so the panel border reaches the window bottom margin. Leftover internal slack (~40–60 px) stays inside the panel below the grid — no external dead band.
- Control pills: allocated 30 px, drawn 26 px tall (ACC/SLD/gate/pitch); octave arrow cells allocated 30 px, triangle centered. Number header allocated 16 px, drawn full. LED bar 4 px.
- Fonts scaled with pills: pitch/number monospace 10.0, gate 9.5, ACC/SLD 8.5.

- [ ] **Step 1: Rewrite the strip geometry in `step_button.rs`**

Replace the background allocation (current lines 35–42) and all fixed sizes. Key changes:

1. Signature gains `cell_width: f32, cell_height: f32`; replace `let width = 56.0;` with `let width = cell_width;`.
2. Delete the poisoned allocation:
```rust
// DELETE — this consumed the row's remaining height and caused the diagonal cascade:
let (bg_rect, _bg_resp) = ui.allocate_exact_size(vec2(width, ui.available_height()), Sense::hover());
```
3. Record the strip origin before content, paint the column background on the background layer after content so it never advances layout and never covers controls:
```rust
use nih_plug_egui::egui::{LayerId, Order, Rect}; // add to imports

// at top of the ui.vertical closure:
let strip_min = ui.cursor().min();

// ... existing content ...

// last statement inside the closure, before it ends:
let strip_rect = Rect::from_min_size(strip_min, vec2(width, cell_height));
let bg_painter = ui.painter().with_layer_id(LayerId::new(Order::Background, ui.id()));
if ui.is_rect_visible(strip_rect) {
    bg_painter.rect_filled(strip_rect, CornerRadius::same(2), column_bg);
}
```
4. Fixed vertical budget (item spacing stays `vec2(0.0, 0.0)`):
   - LED bar: `allocate_exact_size(vec2(width, 4.0), Sense::hover())` (was 2.0; comment already said 4.0).
   - Number header: `vec2(width, 16.0)`, font `FontId::new(10.0, FontFamily::Monospace)`.
   - ACC/SLD row: hit rects `vec2(btn_w, 30.0)`, drawn rects 26.0 tall, font 8.5.
   - Gate pill: hit `vec2(width, 30.0)`, drawn 26.0, font 9.5.
   - Pitch pill: hit `vec2(width, 30.0)`, drawn 26.0, font `FontId::new(10.0, FontFamily::Monospace)`.
   - Octave arrows: hit `vec2(oct_w, 30.0)`, triangle vertices scaled x1.5 (e.g. up triangle `(cx, cy-4.5) (cx-4.5, cy+3.0) (cx+4.5, cy+3.0)`).
5. Drag sensitivity unchanged (`PIXELS_PER_SEMITONE = 6.0`); all click/drag/hover logic untouched.

- [ ] **Step 2: Build to verify compilation**

Run: `cargo build --release -p plugin303 --bin standalone`
Expected: compiles; no new warnings.

- [ ] **Step 3: Switch the panel to the 2x8 grid and make the frame fill the bottom region**

In `sequencer_panel.rs`, inside the frame closure (lines 37–58), replace the single `ui.horizontal` row with:

```rust
ui.set_max_width(1048.0); // keep existing cap for the transport column math
let grid_avail_w = ui.available_width() - 232.0 - 8.0 - 8.0 - 2.0; // minus transport min width, separator, spaces
let cell_w = ((grid_avail_w - 7.0 * 4.0) / 8.0).max(72.0);
let cell_h = 140.0;

ui.vertical(|ui| {
    if let Ok(mut pattern_guard) = params.pattern.write() {
        let is_running = params.seq_run.value();
        ui.spacing_mut().item_spacing = vec2(4.0, 12.0);

        for row in 0..2 {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                for col in 0..8 {
                    let step_idx = row * 8 + col;
                    let is_current = active_step as usize == step_idx;
                    let step = &mut pattern_guard.steps[step_idx];
                    render_step_strip(ui, step_idx, step, is_current, is_running, cell_w, cell_h);
                }
            });
        }

        render_piano_roll_popup(ui.ctx(), &mut pattern_guard);
    }
});
```

Make the frame fill the remaining window height so no dead band sits below the panel border — in `render_sequencer_and_transport`, before `Frame::new()` (line 27):

```rust
let frame_avail_h = ui.available_height();
```

and inside the frame closure, first line:

```rust
ui.set_min_height((frame_avail_h - 12.0).max(280.0));
```

Leave the transport column (lines 64–432) untouched except: `item_spacing` `vec2(3.0, 1.5)` becomes `vec2(3.0, 3.0)`, and button `min_size` heights 18.0/20.0 become 24.0 (SHIFT, RUN/SYNC, CLEAR rows) so the right column balances the taller grid. Do not change any transport widths, labels, or handlers.

- [ ] **Step 4: Build and capture**

Run: `scripts/capture_standalone_gui.sh`
Expected: builds, captures `latest.png` (FUN stopped), `running.png` (FUN running), `accurate.png` (ACCURATE stopped) plus crops.

- [ ] **Step 5: Vision-verify the captures**

Read `target/gui-qa/latest.png`, `running.png`, `accurate.png` and confirm:
1. All 16 steps visible in a 2x8 grid (01–08 top, 09–16 bottom), none clipped by the window edge.
2. No diagonal cascade; step cells aligned in rows and columns.
3. No floating tall dark column in `running.png` (the old poisoned background rect).
4. Panel border reaches the window bottom margin in all three states — no black band below the panel.
5. Step pills visibly larger than before (ACC/SLD/gate/pitch ≈ 26 px tall).
6. Playhead in `running.png`: active step cell shows elevated background + ruby number badge + cyan LED bar.

If the panel falls short of the window bottom (external band visible), raise `cell_h` in 8 px steps (140 → 148 → 156) and re-capture until the border lands within 8 px of the bottom margin.

- [ ] **Step 6: Commit**

```bash
git add plugin303/src/gui/widgets/step_button.rs plugin303/src/gui/panels/sequencer_panel.rs
git commit -m "fix(gui): sequencer 2x8 grid — remove available_height bg poison, all 16 steps visible, 26px pills"
```

---

### Task 2: Decimal consistency — DECAY formatter

**Files:**
- Modify: `plugin303/src/gui/widgets/knob.rs:211`

**Interfaces:**
- Consumes: nothing.
- Produces: DECAY knob value badge shows `1.1 s` (1 dp) instead of `1.10 s` (2 dp). No other formatter changes.

- [ ] **Step 1: Fix the seconds formatter**

In `knob.rs`, DECAY branch (lines 208–214), change:

```rust
if ms >= 1000.0 {
    format!("{:.2} s", ms / 1000.0)
} else {
    format!("{:.0} ms", ms)
}
```

to:

```rust
if ms >= 1000.0 {
    format!("{:.1} s", ms / 1000.0)
} else {
    format!("{:.0} ms", ms)
}
```

This makes DECAY conform to the formatting rule (1 dp for s; ms stay integer, matching GLIDE `200 ms`). Every other visible badge already conforms: `+0.0 ct`, `2.1 kHz`, `50 %`, `+0.0 dB`, `+0.0 st`, `120.0` BPM (transport DragValue), `3 ms`/`200 ms` envelopes.

- [ ] **Step 2: Build and capture**

Run: `cargo build --release -p plugin303 --bin standalone && scripts/capture_standalone_gui.sh`

- [ ] **Step 3: Vision-verify**

Read `target/gui-qa/latest_crop_classic.png`: DECAY badge reads `1.1 s`. No other badge changed format.

- [ ] **Step 4: Commit**

```bash
git add plugin303/src/gui/widgets/knob.rs
git commit -m "fix(gui): DECAY badge 1dp seconds — unify value formatting rule"
```

---

### Task 3: Full re-audit verification — close #11, confirm no regressions

**Files:**
- Modify: `audit-report.md` (status column refresh only)

**Interfaces:**
- Consumes: captures from Task 2's run (already current).
- Produces: verified close-out of all 16 findings.

- [ ] **Step 1: Verify #11 — playhead vs TIE across all 16 steps**

Read `target/gui-qa/running.png` (now unblocked by Task 1):
1. Active step shows ruby number-badge outline + cyan LED bar + elevated cell background.
2. TIE steps show gold badges — visually distinct from the playhead at a glance and at 100% zoom.
3. With LEN 16 and the generated pattern (steps 01 NOTE, 02 TIE, 03 SLD NOTE, 04 REST, 05 ACC NOTE), confirm each gate type renders its distinct color while the playhead traverses. If the capture instant makes this ambiguous, re-run `scripts/capture_standalone_gui.sh` once (the running-state capture lands on a random step; two samples give coverage).

- [ ] **Step 2: Regression sweep of the ten preserved fixes**

Read the fresh `latest.png`, `running.png`, `accurate.png` + crops and confirm each still holds:
- #2 tweaks panel spans to right margin.
- #3 ACCURATE overlay box centered with modest band margins (`accurate.png`).
- #4 OUTPUT CLIPPER and ANALOG CHARACTER & TWEAKS bottoms aligned.
- #5 classic-row knobs at 50% show identical left-half arcs ending at the pointer.
- #6 QUALITY group uniformly dimmed in `accurate.png`.
- #8 soup PW 50% arc consistent (`latest_crop_souped.png`).
- #9 STOP button orange in `running.png`.
- #10 OUTPUT CLIPPER title white/gray.
- #13 envelope legends `(Mod Envelope)` / `(Voice Envelope)` present.
- #14 no orphan track beside WAVE selector.
- #16 running feedback intact (STOP label, playhead highlight).

- [ ] **Step 3: Refresh audit-report.md statuses**

Update the Status/Defect lines for #1 (Resolved — 2x8 grid, all steps visible), #7 (Closed — intentional 0% dim styling), #11 (Resolved — distinct ruby/gold, verified across grid), #12 (Resolved — 1 dp rule), #15 (Resolved — 26 px pills / 30 px targets). Leave resolved findings' text intact.

- [ ] **Step 4: Commit**

```bash
git add audit-report.md
git commit -m "docs: audit report close-out — phase 4 verification pass"
```

---

## Self-Review

- **Spec coverage:** findings still holding — #1 (Task 1), #15 (Task 1, same geometry change), #12 (Task 2), #11 (Task 3, unblocked by Task 1), #7 closed as intentional with rationale in the table. New defects (bg column artifact, LED 2 vs 4 px) folded into Task 1. Preserved set guarded by Global Constraints + Task 3 Step 2.
- **Placeholder scan:** none — all steps carry exact code, line references, commands, and acceptance numbers.
- **Type consistency:** `render_step_strip` new signature `(ui, step_idx, step, is_current, is_running, cell_width, cell_height)` defined in Task 1 Interfaces, used in Task 1 Step 3 call site; single caller updated in the same task.
