# Fix PR 33 Round-2 Review Findings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Address the round-2 code review findings posted on PR #33 (arg-parse panic, vacuous test, dead code, arg-transposition hazard, THD measurement bias) and close out the two decision items (tilt zipper, gain-link scope).

**Architecture:** One CLI hardening in `plugin303/src/bin/eval_clipper.rs`, one test rewrite in `plugin303/tests/warm_clipper.rs`, two one-line deletions (DSP + Python), one signature refactor in `plugin303/src/dsp/voice.rs`, one measurement fix in `eval/303/eval_clipper_aliasing.py`, two documented decisions. Verify with the fast test suite (`cargo nextest run -p plugin303`).

**Tech Stack:** Rust, nih-plug, Python (uv).

**Spec:** Round-2 review comment on PR #33 (https://github.com/gustavokch/vibe303/pull/33#issuecomment-5336633016)

## Global Constraints

- No behavioral regressions in the DSP core. The tilt pair, mix law, and dynamics ducking were verified correct in review — change only what the findings name.
- No transcendentals added to per-tick audio paths.
- Run `cargo check -p plugin303` after each Rust edit; run the fast suite before finishing.
- The round-1 plan (`2026-08-18-fix-pr-33-review-findings.md`) is already landed — do not reopen its items.

---

### Task 1: Fix eval_clipper trailing-flag panic (Finding 1, 🟡)

**Files:**
- Modify: `plugin303/src/bin/eval_clipper.rs:63-145` (the 14 value-parsing arms)

**Interfaces:**
- Every value arm does `i += 1; x = args[i].parse()?;` — a trailing flag with no value panics with index-out-of-bounds instead of a clean error.

- [ ] **Step 1: Add a value-reader helper**

After the `print_usage` fn, add:

```rust
fn take_value(args: &[String], i: &mut usize, flag: &str) -> Result<&str, Box<dyn std::error::Error>> {
    *i += 1;
    args.get(*i)
        .map(|s| s.as_str())
        .ok_or_else(|| format!("Missing value for {}", flag).into())
}
```

- [ ] **Step 2: Convert all 14 value arms to the helper**

Replace each `i += 1; x = args[i].parse()?;` with `x = take_value(&args, &mut i, "--flag")?.parse()?;`. For the two `String` arms (`--signal`, `--in`, `--out`), use `.to_lowercase()` / `.to_string()` on the returned `&str`. Flag arms (`--bypass`, `--asym`, `--help`) stay as-is.

- [ ] **Step 3: Verify manually**

```bash
cargo run --bin eval_clipper -- --sr 2>&1 | grep -q "Missing value for --sr" && echo PASS
cargo run --bin eval_clipper -- --out /tmp/t.f32 --secs 0.01 && echo PASS
```

- [ ] **Step 4: `cargo check -p plugin303`**

---

### Task 2: Make `dynamics_zero_is_transparent` a real guard (Finding 2, 🔵)

**Files:**
- Modify: `plugin303/tests/warm_clipper.rs:400` (`dynamics_zero_is_transparent`)

**Interfaces:**
- Current body compares `ClipperParams::default()` against `dynamics = 0.0` — identical configs, so the assert can never fail. The property worth guarding: with `dynamics = 0.0`, `peak_env` must stay untouched and effective drive must equal `p.drive_ohms` exactly.

- [ ] **Step 1: Rewrite the test to assert the bypass condition directly**

Replace the body with a bit-exact comparison against a reference render, where the reference is the same engine but with the dynamics code path proven unentered. Practical formulation: render 1024 samples of a loud sine (0.8 amplitude — above `DYN_REF`) with `dynamics = 0.0`, then assert the output equals a second render where `dynamics` defaults and `drive_ohms` is explicitly re-set to the same value — still identical configs, so instead assert the structural property: with `dynamics = 0.0` the output must equal the output computed when the duck branch is unreachable. Since that cannot be observed externally, the honest fix is to delete the vacuous equality and instead test the boundary that IS observable: `dynamics = 0.0` vs `dynamics = f32::MIN_POSITIVE` must differ (any nonzero value enters the branch), and `dynamics = 0.0` must equal bypass-branch-free behavior via the existing `dynamics_reduces_sustained_distortion` test's off-arm.

Concretely, replace the test with:

```rust
#[test]
fn dynamics_zero_vs_epsilon_differs() {
    // dynamics = 0.0 skips the duck branch entirely; any nonzero value enters
    // it. A loud sustained tone at max drive must render differently.
    let sr = 48_000.0;
    let run = |dynamics: f32| -> Vec<f32> {
        let mut c = WarmClipper::new(sr);
        let mut p = ClipperParams::default();
        p.mix = 1.0;
        p.drive_ohms = 100_000.0;
        p.dynamics = dynamics;
        c.set_params_immediate(p);
        (0..2048)
            .map(|i| {
                let t = i as f32 / sr;
                c.tick((2.0 * std::f32::consts::PI * 800.0 * t).sin())
            })
            .collect()
    };
    let off = run(0.0);
    let eps = run(f32::MIN_POSITIVE);
    assert_ne!(off, eps, "dynamics = 0 must skip the duck branch; any nonzero value must enter it");
}
```

- [ ] **Step 2: `cargo nextest run -p plugin303 --test warm_clipper`**

---

### Task 3: Drop the redundant drive floor (Finding 3, 🔵)

**Files:**
- Modify: `plugin303/src/dsp/warm_clipper.rs:238`

- [ ] **Step 1: Remove the dead `.max(22_000.0)`**

Replace:
```rust
(p.drive_ohms * (1.0 - duck)).max(22_000.0)
```
With:
```rust
p.drive_ohms * (1.0 - duck)
```
The clamp to `22_000..100_000` on the next line (the `gain` computation) already floors the value; `duck ≤ 1.0` and `drive_ohms ≥ 22_000` guarantee the product never needs it. Verify no other consumer of `drive_ohms` exists between the two lines (there is one: the `gain` line only — confirmed in review).

- [ ] **Step 2: `cargo nextest run -p plugin303 --test warm_clipper`** (dynamics tests pin behavior)

---

### Task 4: Pass `ClipperParams` instead of 12 positional args (Finding 5, 🔵)

**Files:**
- Modify: `plugin303/src/dsp/voice.rs:766-790` (both `update_clipper_params*`)
- Modify: `plugin303/src/lib.rs:280-287` (call site)
- Modify: 6 test call sites (`plugin303/tests/high_spectral.rs`, `reso_gain_comp.rs`, `tier_calibration.rs`, `ultra_spectral.rs`, `voice_esr.rs`, `voicing_features.rs`)

**Interfaces:**
- New signature for both fns:

```rust
pub struct VoiceClipperParams {
    pub bypass: bool,
    pub os: ClipperOversampling,
    pub drive: f32,
    pub threshold: f32,
    pub bass: f32,
    pub tone: f32,
    pub asym: bool,
    pub mix: f32,
    pub in_db: f32,
    pub out_db: f32,
    pub tilt_db: f32,
    pub dynamics: f32,
}

pub fn update_clipper_params(&mut self, p: VoiceClipperParams) { ... }
pub fn update_clipper_params_immediate(&mut self, p: VoiceClipperParams) { ... }
```

Named-field construction at call sites makes transposition a compile error instead of a silent miswire. Do NOT reuse `ClipperParams` directly — its `os` field is the DSP `ClipperOs` enum and the voice layer owns the `ClipperOversampling` → `ClipperOs` mapping.

- [ ] **Step 1: Add `VoiceClipperParams` and change both signatures; move the `ClipperOversampling` → `ClipperOs` match inside.**

- [ ] **Step 2: Update `lib.rs` call site to struct literal with named fields from `self.params`.**

- [ ] **Step 3: Update the 6 test call sites.** Tests that bypass the clipper get a helper:

```rust
fn bypassed_clipper() -> VoiceClipperParams {
    VoiceClipperParams {
        bypass: true, os: ClipperOversampling::X2,
        drive: 47_000.0, threshold: 4_700.0, bass: 8_200.0, tone: 1e-9,
        asym: false, mix: 1.0, in_db: 0.0, out_db: 0.0, tilt_db: 0.0, dynamics: 0.0,
    }
}
```
Put the helper in `plugin303/tests/common/mod.rs` if one exists, otherwise duplicate per-file (match existing test style).

Note: `voice_esr.rs` passes `threshold: 4_700.0` — keep each call site's existing values verbatim, only change the shape.

- [ ] **Step 4: `cargo check -p plugin303 --all-targets` then fast suite.**

---

### Task 5: Python eval cleanups (Findings 6 + 7, 🔵)

**Files:**
- Modify: `eval/303/eval_clipper_aliasing.py:65` (dead line), `eval/303/eval_clipper_aliasing.py:154-164` (THD computation)

- [ ] **Step 1: Delete `fn_render_clipper = None` (line 65).**

- [ ] **Step 2: Subtract the full fundamental band in THD**

In `compute_aliasing_metrics`, replace the single-bin `fund_power` with the fundamental's ±`tolerance_hz` band:

```python
fund_band = np.abs(freqs - f0) <= tolerance_hz
fund_power = np.sum(power[fund_band])
```
And change `thd_db` to use the residual band energy:

```python
harm_minus_fund = np.sum(power[harmonic_mask & ~dc_mask & ~fund_band])
thd_db = 10.0 * np.log10(max(harm_minus_fund, 1e-300) / max(fund_power, 1e-300))
```
Keep `asr_db` unchanged (it uses `harm_power`, which correctly includes the fundamental band).

- [ ] **Step 3: Regenerate eval artifacts**

```bash
cd eval/303 && uv run eval_clipper_aliasing.py
```
Confirm `results/clipper_aliasing/summary.json` and `clipper_control_sweeps.json` refresh without errors. THD absolute values will drop versus the previous run — expected, note it in the PR.

---

### Task 6: Document the tilt-zipper decision (Finding 4, 🔵 → decision)

**Files:**
- Modify: `docs/superpowers/specs/2026-08-18-clipper-control-dimensions-design.md`

**Decision:** Accept block-rate tilt-coefficient jumps for this PR. Rationale: the existing tone control has the identical pattern (`update_tone_alpha` in `set_params`, no smoothing), tilt is ±6 dB over a first-order shelf so the step is small, and smoothing shelf coefficients per-sample adds per-tick cost for a secondary control.

- [ ] **Step 1: Add a "Known limitations" note to the design spec**

Append to the spec:

```markdown
## Known limitations

- Tilt (and tone, pre-existing) coefficients update at block rate without
  smoothing; fast tilt automation can zipper. Accepted: same pattern as tone,
  small step size, per-sample coefficient smoothing not worth the per-tick
  cost. If tone is ever smoothed, smooth tilt the same way.
```

- [ ] **Step 2: No code change.** Close the finding as accepted-with-rationale in a PR reply.

---

### Task 7: Document the gain-link scope decision (Finding 8, ❓ → decision)

**Files:**
- Modify: `docs/superpowers/specs/2026-08-18-clipper-control-dimensions-design.md`
- Modify: `plugin303/src/gui/panels/output_clipper.rs` (tooltip only)

**Decision:** Keep link enforcement edit-time-only (widget drag events). Host automation and preset recall can leave `in ≠ -out` with link engaged; this is intended — process-side enforcement would mean writing parameter values from the audio thread, which nih-plug does not permit, and GUI value-diff polling (the round-1 approach) was rejected for generating host-visible edits without user gestures.

- [ ] **Step 1: Extend the spec's "Known limitations" section**

```markdown
- Gain link is enforced only on GUI drag/double-click gestures. Host
  automation or preset recall can leave IN ≠ −OUT with LINK engaged until
  the user next touches either knob. Accepted: audio-thread parameter writes
  are not permitted, and value-diff polling creates host-visible edits with
  no user gesture.
```

- [ ] **Step 2: Add a hover tooltip to the LINK checkbox**

In `output_clipper.rs`, wrap the checkbox with `.on_hover_text("Link: OUT tracks −IN while dragging (edit-time only)")`.

- [ ] **Step 3: Reply on the PR answering the ❓ with this rationale; mark the conversation resolved.**

---

### Task 8: Final verification

- [ ] **Step 1: `cargo nextest run -p plugin303`** — full fast suite green.
- [ ] **Step 2: `cargo nextest run -p plugin303 --profile slow`** — slow gates still green (voice.rs signature change touches calibration tests).
- [ ] **Step 3: `cargo clippy -p plugin303 --all-targets -- -D warnings`** clean.
- [ ] **Step 4: Confirm the regenerated eval artifacts are untracked/ignored as before (`eval/303/results/` was never committed) — no new files staged accidentally.**
- [ ] **Step 5: Push and confirm the PR checks stay green.**
