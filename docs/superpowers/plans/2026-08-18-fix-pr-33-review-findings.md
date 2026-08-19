# Fix PR 33 Review Findings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Address the code review findings posted on PR #33 (eval threshold default, GUI gain-link feedback, reset param preservation, mix default decision) and regenerate the affected eval data.

**Architecture:** Two one-line eval tooling fixes, one GUI rework in `plugin303/src/gui/panels/output_clipper.rs` (event-driven gain link instead of value-diff), one DSP hardening in `plugin303/src/dsp/warm_clipper.rs`, one documented product decision. Verify with the fast test suite (`cargo nextest run -p plugin303`).

**Tech Stack:** Rust, nih-plug, nih_plug_egui, Python (uv).

**Spec:** Review comment on PR #33 (https://github.com/gustavokch/vibe303/pull/33#issuecomment-5336394454)

## Global Constraints

- No behavioral regressions in DSP or parameter handling; DSP core (tilt pair, mix law, dynamics) was verified clean — do not touch it.
- No transcendentals in per-tick audio paths (PR #32 finding pattern).
- GUI link fix must not introduce host-visible parameter edits except on genuine user gestures.
- Run `cargo check -p plugin303` after each Rust edit; run the fast suite before finishing.

---

### Task 1: Fix eval_clipper `--thresh` default and help text

**Files:**
- Modify: `plugin303/src/bin/eval_clipper.rs:16` (help text), `plugin303/src/bin/eval_clipper.rs:45` (default value)

**Interfaces:**
- The DSP clamps `threshold_ohms` to `2_200..22_000` (`plugin303/src/dsp/warm_clipper.rs:248`); the plugin param default is `4_700.0` (`plugin303/src/params.rs:633`). Eval defaults must match the plugin default.

- [ ] **Step 1: Fix the help text range**

In `plugin303/src/bin/eval_clipper.rs` line 16, replace:
```rust
--thresh <float>    Threshold resistance in Ohms (22000.0..220000.0, default: 47000.0)
```
With:
```rust
--thresh <float>    Threshold resistance in Ohms (2200.0..22000.0, default: 4700.0)
```

- [ ] **Step 2: Fix the default value**

In `plugin303/src/bin/eval_clipper.rs` line 45, replace:
```rust
let mut thresh: f32 = 47000.0;
```
With:
```rust
let mut thresh: f32 = 4700.0;
```

- [ ] **Step 3: Verify build**

Run: `cargo check -p plugin303 --bin eval_clipper`
Expected: PASS

---

### Task 2: Fix Python eval wrapper threshold default

**Files:**
- Modify: `eval/303/eval_clipper_aliasing.py:72`

- [ ] **Step 1: Fix the default**

Replace:
```python
thresh: float = 47000.0,
```
With:
```python
thresh: float = 4700.0,
```

- [ ] **Step 2: Check README for stale threshold mentions**

Read `eval/303/README_CLIPPER_ALIASING.md`. If it documents the `47000` default or the wrong range, update to `4700` / `2200..22000`.

---

### Task 3: Regenerate eval results at the correct operating point

**Files:**
- Regenerate: `eval/303/results/clipper_aliasing/` (existing results were measured with threshold clamped to 22000, i.e. minimum detector sensitivity, not the plugin default 4700)

- [ ] **Step 1: Re-run the aliasing sweep**

Run: `cd eval/303 && uv run --no-project python eval_clipper_aliasing.py`
Expected: completes, writes fresh `clipper_control_sweeps.json` and result artifacts measured at `thresh = 4700` (`sens_gain = 1.0`).

- [ ] **Step 2: Sanity-check the delta**

Compare a before/after THD or aliasing figure for one identical sweep point. Expect materially different numbers (threshold dropped 4.7x, so the JFET sidechain is more sensitive). If numbers are bit-identical, the flag is not being threaded — investigate before proceeding.

---

### Task 4: Rework GUI gain link to be event-driven

**Files:**
- Modify: `plugin303/src/gui/panels/output_clipper.rs:40-58` (capture knob responses), delete the value-diff block at lines 61-82

**Interfaces:**
- `Knob303::ui` returns `egui::Response` with `drag_started()`, `dragged()`, `drag_stopped()`, `double_clicked()` available (`plugin303/src/gui/widgets/knob.rs:34-57`). These are genuine user gestures only — host automation and preset recall never produce them.
- Current defect: the per-frame value diff (lines 61-82) fires `begin/end_set_parameter` on the counterpart for ANY value change, including host-driven ones (preset recall with `in != -out` gets OUT overwritten; automation on IN spawns host-visible OUT edits).

**Design:** Mirror the counterpart inside the user's gesture. When the IN knob reports drag events and link is on, wrap the OUT param in matching begin/set/end calls (OUT := -IN), and vice versa. Include double-click reset (`double_clicked()` or `response.changed()` without drag) so reset-to-default also mirrors. Delete the value-diff block and its egui temp-data storage entirely.

- [ ] **Step 1: Capture responses from the IN and OUT knobs**

Replace lines 41 and 54 so responses are bound:
```rust
let in_resp = ui.add(Knob303::new(&params.clipper_in_gain, setter, "IN", 13.0));
```
```rust
let out_resp = ui.add(Knob303::new(&params.clipper_out_gain, setter, "OUT", 13.0));
```
Note: `in_resp` is used after `out_resp` is created; declare `in_resp` before the LINK checkbox block and `out_resp` at line 54, then apply link logic after both exist (still inside the `ui.horizontal` closure, after line 57).

- [ ] **Step 2: Implement event-driven mirroring**

After the knobs, inside the same horizontal closure:
```rust
let linked = params.clipper_gain_link.value();
if linked {
    if in_resp.drag_started() || in_resp.double_clicked() {
        setter.begin_set_parameter(&params.clipper_out_gain);
    }
    if in_resp.dragged() || in_resp.double_clicked() {
        setter.set_parameter(&params.clipper_out_gain, -params.clipper_in_gain.value());
    }
    if in_resp.drag_stopped() || in_resp.double_clicked() {
        setter.end_set_parameter(&params.clipper_out_gain);
    }
    if out_resp.drag_started() || out_resp.double_clicked() {
        setter.begin_set_parameter(&params.clipper_in_gain);
    }
    if out_resp.dragged() || out_resp.double_clicked() {
        setter.set_parameter(&params.clipper_in_gain, -params.clipper_out_gain.value());
    }
    if out_resp.drag_stopped() || out_resp.double_clicked() {
        setter.end_set_parameter(&params.clipper_in_gain);
    }
}
```
(Exact event combination may be simplified once written — e.g. `changed()` — but the rule stands: only widget gesture events may trigger counterpart writes. Keep the existing LINK-engage snap at lines 47-52; that is a genuine user gesture on the checkbox.)

- [ ] **Step 3: Delete the value-diff block**

Remove lines 61-82 (the `// Gain link enforcement: edit-time only` block) entirely, including the `ui.ctx().data_mut` temp storage.

- [ ] **Step 4: Verify build**

Run: `cargo check -p plugin303`
Expected: PASS

- [ ] **Step 5: Manual GUI verification**

Run the plugin standalone (`cargo run -p plugin303 --bin plugin303` or the project's standalone target). Verify: (a) dragging IN with LINK on moves OUT to -IN live; (b) dragging OUT moves IN; (c) double-click reset on either knob mirrors; (d) with LINK on, changing IN via the host/automation lane does NOT produce OUT edits (check the host's automation/touch indicators stay clean); (e) recalling a preset with `in != -out` and LINK on leaves both values as stored. Items (d)-(e) are the regression this task fixes.

---

### Task 5: Preserve params across `StagedEngine::reset`

**Files:**
- Modify: `plugin303/src/dsp/warm_clipper.rs:192-194` (`StagedEngine::reset`)

**Context:** Pre-existing on main, widened by this PR (five new params also wiped). Unreachable in the plugin today because `Plugin303::process` re-pushes all clipper params every block before rendering — this is hardening for direct DSP callers (tests, eval binaries) that `reset()` then `tick()` without a fresh `set_params`.

- [ ] **Step 1: Preserve params across reset**

Replace:
```rust
fn reset(&mut self) {
    *self = Self::new(self.sr);
}
```
With:
```rust
fn reset(&mut self) {
    let params = self.params;
    *self = Self::new(self.sr);
    self.set_params(params);
}
```
Check `ClipperParams` is `Copy` (it is a plain f32/bool struct — if not `Copy`, clone). `set_params` re-derives tilt coefficients and smoother targets, so engine state is fully consistent.

- [ ] **Step 2: Add a regression test**

In `plugin303/tests/warm_clipper.rs`, add a test: configure non-default tilt/dynamics/drive, call `warm_clipper.reset()` (or engine reset at the exposed level), render a buffer, and assert output matches a fresh instance that received the same params (or simply assert params survive: render before reset, reset, render again without set_params, outputs identical for the same input). Follow existing test patterns in that file.

- [ ] **Step 3: Run clipper tests**

Run: `cargo nextest run -p plugin303 --test warm_clipper`
Expected: PASS (all existing + new test)

---

### Task 6: Decide and document the `clipper_mix` default

**Files:**
- Possibly modify: `plugin303/src/params.rs:637`
- Modify: `docs/superpowers/specs/2026-08-18-clipper-control-dimensions-design.md` (decision note)

**Context:** Review confirmed the `0.0 (dry)` default is deliberate per spec. Open question: sessions saved with tag 0.4.0 (clipper engaged, no mix param) load with the clipper engaged but 100% dry — a previously audible stage goes silent.

- [ ] **Step 1: Make the call with the project owner**

Options: (a) keep 0.0 — new instances start transparent, dry is the safe factory state, accepted cost for old sessions; (b) switch to 1.0 — preserves backward compatibility, new instances get the previously default wet behavior.

- [ ] **Step 2a: If keeping 0.0**

Add a short "Backward compatibility" note to the design spec stating the tradeoff was considered and accepted. No code change.

- [ ] **Step 2b: If switching to 1.0**

Change `params.rs:637` default to `1.0`, update the spec table and the comment at `warm_clipper.rs:55`, and re-check any test that depends on the plugin param default.

---

### Task 7: Full verification

- [ ] **Step 1: Fast test suite**

Run: `cargo nextest run -p plugin303`
Expected: PASS, except the known pre-existing `voice_esr` failure (3.55 dB vs 3.5 dB budget, fails identically on clean main — unrelated drift, do not chase in this plan).

- [ ] **Step 2: Clippy**

Run: `cargo clippy -p plugin303 --all-targets`
Expected: no new warnings.

- [ ] **Step 3: Reply on the PR**

Comment on PR #33 summarizing which findings were fixed, which were deferred (with reasons), and link the regenerated eval results.
