# Clipper Control Dimensions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Mix, Input/Output Gain (with GUI link), Tilt pre-emphasis, and Dynamics transient ducking to the WarmClipper output stage.

**Architecture:** All DSP integrated into `warm_clipper.rs` (Approach A). Tilt + dynamics run inside the oversampled `StagedEngine` (each engine already knows its OS rate: `sr*2` / `sr*4`). Gain + equal-power mix run at base rate in `WarmClipper`, reusing the existing `Delay48` dry path.

**Tech Stack:** Rust, nih_plug, nih_plug_egui.

**Spec:** `docs/superpowers/specs/2026-08-18-clipper-control-dimensions-design.md`

## Global Constraints

- No per-sample `exp()` / `powf()` / `tan()` — precalculate in `new()` / `set_params()` (v4 perf convention).
- `CLIPPER_LATENCY` stays 48; total plugin latency stays 96 samples.
- DSP `ClipperParams::default()` keeps `mix = 1.0` (wet) so all existing DSP tests pass unchanged. The **plugin parameter** default is `mix = 0.0` (dry) per spec.
- All new DSP lives in `plugin303/src/dsp/warm_clipper.rs`; no new DSP files.
- Spec deviation (approved at plan time): `clipper_in_gain`, `clipper_out_gain`, `clipper_tilt` use `FloatRange::Linear`, not Skewed — symmetric dB ranges must center 0 dB; skew would misplace unity gain.
- `cargo test -p plugin303` green after every task.
- Commit style: conventional commits, e.g. `feat(dsp): ...`.

## File Structure

| File | Responsibility | Tasks |
|---|---|---|
| `plugin303/src/dsp/warm_clipper.rs` | ClipperParams fields, gain/mix base-rate DSP, tilt + dynamics OS-rate DSP, unit tests | 1, 2, 3 |
| `plugin303/tests/warm_clipper.rs` | Integration tests for new dimensions | 1, 2, 3 |
| `plugin303/src/dsp/voice.rs` | `update_clipper_params[_immediate]` signature extension | 1 (compile fix), 4 |
| `plugin303/src/bin/eval_clipper.rs` | CLI flags for new params | 1 (compile fix), 6 |
| `plugin303/src/params.rs` | 6 new plugin params | 4 |
| `plugin303/src/lib.rs` | Param wiring at block rate | 4 |
| `plugin303/tests/{ultra_spectral,reso_gain_comp,tier_calibration,high_spectral}.rs` | Call-site signature updates | 4 |
| `plugin303/src/gui/panels/output_clipper.rs` | Row 2 UI + link logic | 5 |
| `eval/303/eval_clipper_aliasing.py` | Control sweep evaluation | 6 |
| `eval/303/README_CLIPPER_ALIASING.md` | New flag docs | 6 |

---

### Task 1: ClipperParams fields + base-rate gain/mix DSP

**Files:**
- Modify: `plugin303/src/dsp/warm_clipper.rs`
- Modify: `plugin303/src/dsp/voice.rs:766-788` (compile fix only — `..Default::default()`)
- Modify: `plugin303/src/bin/eval_clipper.rs:164-172` (compile fix only)
- Test: `plugin303/tests/warm_clipper.rs` (new tests appended)
- Test: `plugin303/src/dsp/warm_clipper.rs` `mod tests` (white-box gain test)

**Interfaces:**
- Produces: `ClipperParams` new fields `mix: f32, in_db: f32, out_db: f32, tilt_db: f32, dynamics: f32` (all tasks rely on these names).
- Produces: `WarmClipper::branch(&mut self, bypass: bool, os: ClipperOs, x: f32, d: f32) -> f32` (private).

- [ ] **Step 1: Write the failing tests**

Append to `plugin303/tests/warm_clipper.rs`:

```rust
#[test]
fn mix_zero_matches_bypass() {
    let sr = 48_000.0;
    let mut c_byp = WarmClipper::new(sr);
    let mut p = ClipperParams::default();
    p.bypass = true;
    c_byp.set_params_immediate(p);

    let mut c_dry = WarmClipper::new(sr);
    let mut p2 = ClipperParams::default();
    p2.mix = 0.0;
    c_dry.set_params_immediate(p2);

    for i in 0..256 {
        let x = if i == 0 { 0.75 } else { 0.0 };
        assert_eq!(c_byp.tick(x), c_dry.tick(x), "mix=0 must equal bypass output");
    }
}

#[test]
fn mix_one_matches_default_wet() {
    let sr = 48_000.0;
    let mut a = WarmClipper::new(sr);
    a.set_params_immediate(ClipperParams::default());
    let mut b = WarmClipper::new(sr);
    let mut p = ClipperParams::default();
    p.mix = 1.0;
    b.set_params_immediate(p);
    for i in 0..512 {
        let t = i as f32 / sr;
        let x = (2.0 * std::f32::consts::PI * 440.0 * t).sin();
        assert_eq!(a.tick(x), b.tick(x));
    }
}

#[test]
fn gain_round_trip_unity_small_signal() {
    let sr = 48_000.0;
    let run = |in_db: f32, out_db: f32| -> Vec<f32> {
        let mut c = WarmClipper::new(sr);
        let mut p = ClipperParams::default();
        p.mix = 1.0;
        p.in_db = in_db;
        p.out_db = out_db;
        p.drive_ohms = 22_000.0; // minimum drive: near-linear small-signal region
        c.set_params_immediate(p);
        (0..9600)
            .map(|i| {
                let t = i as f32 / sr;
                c.tick(1e-3 * (2.0 * std::f32::consts::PI * 1000.0 * t).sin())
            })
            .collect()
    };
    let a = run(0.0, 0.0);
    let b = run(6.0, -6.0);
    let tail_a = &a[4800..];
    let tail_b = &b[4800..];
    let rms = |v: &[f32]| (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt();
    let diff: Vec<f32> = tail_a.iter().zip(tail_b).map(|(x, y)| x - y).collect();
    assert!(
        rms(&diff) / rms(tail_a).max(1e-12) < 0.01,
        "+6 dB in / -6 dB out must be unity within 1% RMS"
    );
}
```

Add white-box test inside `mod tests` in `plugin303/src/dsp/warm_clipper.rs`:

```rust
#[test]
fn mix_half_equal_power_gains() {
    let mut c = WarmClipper::new(48_000.0);
    let mut p = ClipperParams::default();
    p.mix = 0.5;
    c.set_params_immediate(p);
    let expect = (0.5f32 * core::f32::consts::FRAC_PI_2).sin(); // 0.7071
    assert!((c.dry_s - expect).abs() < 1e-6);
    assert!((c.wet_s - expect).abs() < 1e-6);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p plugin303 --test warm_clipper mix_zero_matches_bypass`
Expected: FAIL to compile — `ClipperParams` has no field `mix`.

- [ ] **Step 3: Implement**

In `warm_clipper.rs`, extend `ClipperParams` and its `Default`:

```rust
#[derive(Clone, Copy, Debug)]
pub struct ClipperParams {
    pub bypass: bool,
    pub os: ClipperOs,
    pub drive_ohms: f32,
    pub threshold_ohms: f32,
    pub bass_ohms: f32,
    pub tone_f: f32,
    pub asym: bool,
    pub mix: f32,      // 0.0 = dry, 1.0 = wet; equal-power law
    pub in_db: f32,    // pre-clipper input gain
    pub out_db: f32,   // post-clipper output gain
    pub tilt_db: f32,  // pre/post shelf pair (Task 2)
    pub dynamics: f32, // transient drive ducking (Task 3)
}

impl Default for ClipperParams {
    fn default() -> Self {
        Self {
            bypass: false,
            os: ClipperOs::X2,
            drive_ohms: 47_000.0,
            threshold_ohms: 4_700.0,
            bass_ohms: 8_200.0,
            tone_f: 1e-9,
            asym: false,
            mix: 1.0, // DSP default stays wet; the plugin param defaults to dry
            in_db: 0.0,
            out_db: 0.0,
            tilt_db: 0.0,
            dynamics: 0.0,
        }
    }
}
```

Add smoother fields to `WarmClipper`:

```rust
pub struct WarmClipper {
    os2: Os2x,
    os4: Os4x,
    e2: StagedEngine,
    e4: StagedEngine,
    pad2: Delay16,
    dry: Delay48,
    params: ClipperParams,
    active_bypass: bool,
    active_os: ClipperOs,
    fade_from_bypass: bool,
    fade_from_os: ClipperOs,
    fade: f32,
    fade_step: f32,
    // Base-rate smoothed scalars for the new control dimensions.
    smooth_k: f32,
    in_t: f32, in_s: f32,
    out_t: f32, out_s: f32,
    dry_t: f32, dry_s: f32,
    wet_t: f32, wet_s: f32,
}
```

In `WarmClipper::new`, initialize (default mix=1 → dry=0, wet=1):

```rust
smooth_k: 1.0 - (-1.0 / (0.010 * sr)).exp(),
in_t: 1.0, in_s: 1.0,
out_t: 1.0, out_s: 1.0,
dry_t: 0.0, dry_s: 0.0,
wet_t: 1.0, wet_s: 1.0,
```

Add target computation and branch mixing:

```rust
fn set_targets(&mut self, p: &ClipperParams) {
    self.in_t = 10f32.powf(p.in_db.clamp(-18.0, 18.0) / 20.0);
    self.out_t = 10f32.powf(p.out_db.clamp(-18.0, 18.0) / 20.0);
    let angle = p.mix.clamp(0.0, 1.0) * core::f32::consts::FRAC_PI_2;
    self.dry_t = angle.cos();
    self.wet_t = angle.sin();
}

fn snap_smoothers(&mut self) {
    self.in_s = self.in_t;
    self.out_s = self.out_t;
    self.dry_s = self.dry_t;
    self.wet_s = self.wet_t;
}

fn branch(&mut self, bypass: bool, os: ClipperOs, x: f32, d: f32) -> f32 {
    if bypass {
        d
    } else {
        let xg = x * self.in_s;
        let wet = match os {
            ClipperOs::X2 => self.wet2(xg),
            ClipperOs::X4 => self.wet4(xg),
        } * self.out_s;
        self.dry_s * d + self.wet_s * wet
    }
}
```

Rewrite `set_params`, `set_params_immediate`, `tick`, `reset`; delete `render`:

```rust
pub fn set_params(&mut self, params: ClipperParams) {
    self.e2.set_params(params);
    self.e4.set_params(params);
    if params.bypass != self.active_bypass || params.os != self.active_os {
        self.fade_from_bypass = self.active_bypass;
        self.fade_from_os = self.active_os;
        self.active_bypass = params.bypass;
        self.active_os = params.os;
        self.fade = 0.0;
    }
    self.set_targets(&params);
    self.params = params;
}

pub fn set_params_immediate(&mut self, params: ClipperParams) {
    self.e2.set_params(params);
    self.e4.set_params(params);
    self.active_bypass = params.bypass;
    self.active_os = params.os;
    self.fade_from_bypass = params.bypass;
    self.fade_from_os = params.os;
    self.fade = 1.0;
    self.set_targets(&params);
    self.snap_smoothers();
    self.params = params;
}

pub fn tick(&mut self, x: f32) -> f32 {
    // Dry delay runs every sample so the mix path always has a warm buffer.
    let d = self.dry.tick(x);
    self.in_s += self.smooth_k * (self.in_t - self.in_s);
    self.out_s += self.smooth_k * (self.out_t - self.out_s);
    self.dry_s += self.smooth_k * (self.dry_t - self.dry_s);
    self.wet_s += self.smooth_k * (self.wet_t - self.wet_s);
    // Both paths are only rendered during a transition. A bypassed steady
    // engine therefore incurs no nonlinear solve.
    let to = self.branch(self.active_bypass, self.active_os, x, d);
    if self.fade >= 1.0 {
        return to;
    }
    let from = self.branch(self.fade_from_bypass, self.fade_from_os, x, d);
    let theta = self.fade * core::f32::consts::FRAC_PI_2;
    let y = from * theta.cos() + to * theta.sin();
    self.fade = (self.fade + self.fade_step).min(1.0);
    y
}

pub fn reset(&mut self) {
    self.os2 = Os2x::new();
    self.os4 = Os4x::new();
    self.e2.reset();
    self.e4.reset();
    self.pad2.reset();
    self.dry.reset();
    self.fade = 1.0;
    self.snap_smoothers();
    self.active_bypass = self.params.bypass;
    self.active_os = self.params.os;
    self.fade_from_bypass = self.params.bypass;
    self.fade_from_os = self.params.os;
}
```

Compile fix in `voice.rs` (both `update_clipper_params` and `update_clipper_params_immediate`):

```rust
self.clipper.set_params(ClipperParams {
    bypass,
    os: match os { ClipperOversampling::X2 => ClipperOs::X2, ClipperOversampling::X4 => ClipperOs::X4 },
    drive_ohms: drive, threshold_ohms: threshold, bass_ohms: bass,
    tone_f: tone, asym,
    ..ClipperParams::default()
});
```

Compile fix in `eval_clipper.rs` struct literal: append `..ClipperParams::default()` after `asym,`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p plugin303 --test warm_clipper`
Expected: PASS, including the 3 new tests + white-box test.
Run: `cargo test -p plugin303`
Expected: all suites PASS (default `mix=1.0` preserves behavior).

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/dsp/warm_clipper.rs plugin303/src/dsp/voice.rs plugin303/src/bin/eval_clipper.rs plugin303/tests/warm_clipper.rs
git commit -m "feat(dsp): clipper input/output gain and equal-power dry/wet mix"
```

---

### Task 2: Tilt pre/post shelf pair in StagedEngine

**Files:**
- Modify: `plugin303/src/dsp/warm_clipper.rs` (`StagedEngine`)
- Test: `plugin303/tests/warm_clipper.rs`

**Interfaces:**
- Consumes: `ClipperParams.tilt_db` (Task 1).
- Produces: `StagedEngine::update_tilt_coeffs(&mut self)` (private).

Design: first-order high shelf, `+tilt_db` at Nyquist, 0 dB at DC, 2 kHz transition, via bilinear one-pole. With `k = tan(π·2000/sr_engine)` and `g = 10^(tilt_db/20)`:

```
a1 = (k - 1) / (k + 1)
b0 = (k + g) / (k + 1)
b1 = (k - g) / (k + 1)
pre:  y = b0·x + b1·x1 - a1·y1
post (exact inverse): y = (1/b0)·(x + a1·x1) - (b1/b0)·y1
```

Engine `sr` is already the oversampled rate (`sr*2` / `sr*4`), so coefficients are per-OS automatically.

- [ ] **Step 1: Write the failing tests**

Append to `plugin303/tests/warm_clipper.rs`:

```rust
#[test]
fn tilt_inversion_flat_small_signal() {
    // Below the clip threshold the core is linear, so pre/post tilt cancel:
    // tilt=+6 dB output must match tilt=0 output within 1% RMS.
    let sr = 48_000.0;
    let run = |tilt_db: f32| -> Vec<f32> {
        let mut c = WarmClipper::new(sr);
        let mut p = ClipperParams::default();
        p.mix = 1.0;
        p.drive_ohms = 22_000.0;
        p.tilt_db = tilt_db;
        c.set_params_immediate(p);
        (0..9600)
            .map(|i| {
                let t = i as f32 / sr;
                c.tick(1e-3 * (2.0 * std::f32::consts::PI * 5000.0 * t).sin())
            })
            .collect()
    };
    let flat = run(0.0);
    let tilted = run(6.0);
    let tail_f = &flat[4800..];
    let tail_t = &tilted[4800..];
    let rms = |v: &[f32]| (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt();
    let diff: Vec<f32> = tail_f.iter().zip(tail_t).map(|(x, y)| x - y).collect();
    assert!(
        rms(&diff) / rms(tail_f).max(1e-12) < 0.01,
        "pre/post tilt must cancel in the linear region"
    );
}

#[test]
fn tilt_pre_emphasis_changes_distortion() {
    // Boosting highs into the clipper distorts a 4 kHz tone more than cutting.
    let sr = 48_000.0;
    let run_thd = |tilt_db: f32| -> f32 {
        let mut c = WarmClipper::new(sr);
        let mut p = ClipperParams::default();
        p.mix = 1.0;
        p.drive_ohms = 47_000.0;
        p.tilt_db = tilt_db;
        c.set_params_immediate(p);
        let n = (sr * 0.2) as usize;
        let mut buf = Vec::new();
        for i in 0..n {
            let t = i as f32 / sr;
            let y = c.tick(0.5 * (2.0 * std::f32::consts::PI * 4000.0 * t).sin());
            if i >= n / 2 {
                buf.push(y);
            }
        }
        compute_thd(&buf, 4000.0, sr)
    };
    let thd_boost = run_thd(6.0);
    let thd_cut = run_thd(-6.0);
    println!("THD +6 dB tilt: {:.2}%, -6 dB tilt: {:.2}%", thd_boost * 100.0, thd_cut * 100.0);
    assert!(thd_boost > thd_cut, "boosting highs pre-clip must increase THD");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p plugin303 --test warm_clipper tilt_inversion_flat_small_signal`
Expected: FAIL — outputs identical because `tilt_db` is unused (diff = 0 passes inversion, so expect `tilt_pre_emphasis_changes_distortion` to FAIL: thd_boost == thd_cut).

- [ ] **Step 3: Implement**

In `StagedEngine`, add fields:

```rust
// Pre/post tilt shelf state (exact-inverse first-order pair at 2 kHz).
tilt_pre_x1: f32,
tilt_pre_y1: f32,
tilt_post_x1: f32,
tilt_post_y1: f32,
tilt_a1: f32,
tilt_b0: f32,
tilt_b1: f32,
tilt_post_k: f32, // 1/b0
tilt_post_b: f32, // b1/b0
tilt_active: bool,
```

Add coefficient update (called from `new()` and `set_params()`):

```rust
fn update_tilt_coeffs(&mut self) {
    let tilt = self.params.tilt_db.clamp(-6.0, 6.0);
    self.tilt_active = tilt != 0.0;
    let k = (core::f32::consts::PI * 2_000.0 / self.sr).tan();
    let g = 10f32.powf(tilt / 20.0);
    self.tilt_a1 = (k - 1.0) / (k + 1.0);
    self.tilt_b0 = (k + g) / (k + 1.0);
    self.tilt_b1 = (k - g) / (k + 1.0);
    self.tilt_post_k = 1.0 / self.tilt_b0;
    self.tilt_post_b = self.tilt_b1 / self.tilt_b0;
}
```

In `StagedEngine::new`, zero-init the four state cells and call `engine.update_tilt_coeffs()` next to `engine.update_tone_alpha()`. In `set_params`, call `self.update_tilt_coeffs()` after `self.update_tone_alpha()`.

In `StagedEngine::tick`, after the input clamp, apply pre-tilt:

```rust
let x = if self.tilt_active {
    let y = self.tilt_b0 * x + self.tilt_b1 * self.tilt_pre_x1 - self.tilt_a1 * self.tilt_pre_y1;
    self.tilt_pre_x1 = x;
    self.tilt_pre_y1 = y;
    y
} else {
    x
};
```

At the end of `tick`, wrap the final `out` (before the `is_finite` guard):

```rust
let out = (clp * 1.35 + bass * 0.20 * bass_mix) * GAIN_MINUS_12DB;
let out = if self.tilt_active {
    let y = self.tilt_post_k * (out + self.tilt_a1 * self.tilt_post_x1)
        - self.tilt_post_b * self.tilt_post_y1;
    self.tilt_post_x1 = out;
    self.tilt_post_y1 = y;
    y
} else {
    out
};
if out.is_finite() { out } else { 0.0 }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p plugin303 --test warm_clipper`
Expected: PASS all, including both tilt tests.
Run: `cargo test -p plugin303`
Expected: all suites PASS (`tilt_db=0` default → `tilt_active=false` → untouched behavior).

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/dsp/warm_clipper.rs plugin303/tests/warm_clipper.rs
git commit -m "feat(dsp): clipper pre/post tilt shelf pair at 2 kHz"
```

---

### Task 3: Dynamics transient ducking in StagedEngine

**Files:**
- Modify: `plugin303/src/dsp/warm_clipper.rs` (`StagedEngine`)
- Test: `plugin303/tests/warm_clipper.rs`

**Interfaces:**
- Consumes: `ClipperParams.dynamics` (Task 1), tilted input `x` (Task 2).
- Produces: no new public API.

Mechanism: fast peak follower (attack 1 ms, release 50 ms, coefficients from engine sr) on the tilted input. Duck factor `d = dynamics · min(1, peak_env / DYN_REF)`; effective drive `drive_ohms · (1 - d)`, floored at 22 kΩ.

- [ ] **Step 1: Write the failing tests**

Append to `plugin303/tests/warm_clipper.rs`:

```rust
#[test]
fn dynamics_zero_is_transparent() {
    let sr = 48_000.0;
    let mut a = WarmClipper::new(sr);
    a.set_params_immediate(ClipperParams::default());
    let mut b = WarmClipper::new(sr);
    let mut p = ClipperParams::default();
    p.dynamics = 0.0;
    b.set_params_immediate(p);
    for i in 0..1024 {
        let t = i as f32 / sr;
        let x = 0.8 * (2.0 * std::f32::consts::PI * 700.0 * t).sin();
        assert_eq!(a.tick(x), b.tick(x), "dynamics=0 must not alter the signal");
    }
}

#[test]
fn dynamics_reduces_sustained_distortion() {
    // Loud sustained tone at max drive: ducking lowers effective drive → less THD.
    let sr = 48_000.0;
    let run_thd = |dynamics: f32| -> f32 {
        let mut c = WarmClipper::new(sr);
        let mut p = ClipperParams::default();
        p.mix = 1.0;
        p.drive_ohms = 100_000.0;
        p.dynamics = dynamics;
        c.set_params_immediate(p);
        let n = (sr * 0.4) as usize;
        let mut buf = Vec::new();
        for i in 0..n {
            let t = i as f32 / sr;
            let y = c.tick((2.0 * std::f32::consts::PI * 800.0 * t).sin());
            if i >= n / 2 {
                buf.push(y);
            }
        }
        compute_thd(&buf, 800.0, sr)
    };
    let thd_off = run_thd(0.0);
    let thd_on = run_thd(1.0);
    println!("THD dynamics off: {:.2}%, on: {:.2}%", thd_off * 100.0, thd_on * 100.0);
    assert!(thd_on < thd_off, "dynamics must duck drive on sustained loud signal");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p plugin303 --test warm_clipper dynamics_reduces_sustained_distortion`
Expected: FAIL — `dynamics` unused, thd_on == thd_off. (`dynamics_zero_is_transparent` passes trivially — that is the regression anchor.)

- [ ] **Step 3: Implement**

Add constant near the top of `warm_clipper.rs`:

```rust
/// Peak level (normalised audio volts) at which dynamics ducking saturates.
const DYN_REF: f32 = 0.5;
```

Add `StagedEngine` fields:

```rust
// Dynamics: fast peak follower ducking effective drive (1 ms / 50 ms).
peak_env: f32,
dyn_attack: f32,
dyn_release: f32,
```

In `StagedEngine::new`, alongside the other coefficient precalculations:

```rust
let dyn_attack = 1.0 - (-1.0 / (0.001 * sr)).exp();
let dyn_release = 1.0 - (-1.0 / (0.050 * sr)).exp();
```

(add to struct init: `peak_env: 0.0, dyn_attack, dyn_release,`)

In `StagedEngine::tick`, replace the fixed `gain` line in stage A:

```rust
// Dynamics: fast peak detector ducks drive on loud passages, letting
// transient attacks through before the Ge/JFET stage saturates.
let drive_ohms = if p.dynamics > 0.0 {
    let ax = x.abs();
    let a = if ax > self.peak_env { self.dyn_attack } else { self.dyn_release };
    self.peak_env += a * (ax - self.peak_env);
    if !self.peak_env.is_finite() {
        self.peak_env = 0.0;
    }
    let duck = p.dynamics.clamp(0.0, 1.0) * (self.peak_env / DYN_REF).min(1.0);
    (p.drive_ohms * (1.0 - duck)).max(22_000.0)
} else {
    p.drive_ohms
};

let buf = x * 2.0;
let gain = 1.0 + drive_ohms.clamp(22_000.0, 100_000.0) / 4_700.0;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p plugin303 --test warm_clipper`
Expected: PASS all.
Run: `cargo test -p plugin303`
Expected: all suites PASS.

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/dsp/warm_clipper.rs plugin303/tests/warm_clipper.rs
git commit -m "feat(dsp): clipper dynamics transient drive ducking"
```

---

### Task 4: Plugin params + voice/lib wiring

**Files:**
- Modify: `plugin303/src/params.rs` (fields after `clipper_asym` ~line 355; defaults after line 624)
- Modify: `plugin303/src/dsp/voice.rs:766-788`
- Modify: `plugin303/src/lib.rs:278-283`
- Modify: `plugin303/tests/ultra_spectral.rs:25`, `plugin303/tests/reso_gain_comp.rs:24`, `plugin303/tests/tier_calibration.rs:158`, `plugin303/tests/high_spectral.rs:36,50`

**Interfaces:**
- Consumes: `ClipperParams` fields (Task 1).
- Produces: plugin params `clipper_mix`, `clipper_in_gain`, `clipper_out_gain`, `clipper_gain_link`, `clipper_tilt`, `clipper_dynamics` (Task 5 GUI relies on these exact names).
- Produces: `Voice::update_clipper_params(&mut self, bypass: bool, os: ClipperOversampling, drive: f32, threshold: f32, bass: f32, tone: f32, asym: bool, mix: f32, in_db: f32, out_db: f32, tilt_db: f32, dynamics: f32)` and same signature for `update_clipper_params_immediate`.

- [ ] **Step 1: Add plugin params**

In `params.rs` struct, after `clipper_asym`:

```rust
#[id = "clipper_mix"]
pub clipper_mix: FloatParam,
#[id = "clipper_in_gain"]
pub clipper_in_gain: FloatParam,
#[id = "clipper_out_gain"]
pub clipper_out_gain: FloatParam,
#[id = "clipper_gain_link"]
pub clipper_gain_link: BoolParam,
#[id = "clipper_tilt"]
pub clipper_tilt: FloatParam,
#[id = "clipper_dynamics"]
pub clipper_dynamics: FloatParam,
```

In `Default`, after `clipper_asym` init:

```rust
clipper_mix: FloatParam::new("Clipper Mix", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
    .with_value_to_string(std::sync::Arc::new(|v| format!("{:.0} %", v * 100.0))),
clipper_in_gain: FloatParam::new("Clipper In", 0.0, FloatRange::Linear { min: -18.0, max: 18.0 })
    .with_unit(" dB"),
clipper_out_gain: FloatParam::new("Clipper Out", 0.0, FloatRange::Linear { min: -18.0, max: 18.0 })
    .with_unit(" dB"),
clipper_gain_link: BoolParam::new("Clipper Gain Link", true),
clipper_tilt: FloatParam::new("Clipper Tilt", 0.0, FloatRange::Linear { min: -6.0, max: 6.0 })
    .with_unit(" dB"),
clipper_dynamics: FloatParam::new("Clipper Dynamics", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
    .with_value_to_string(std::sync::Arc::new(|v| format!("{:.0} %", v * 100.0))),
```

- [ ] **Step 2: Extend voice API and lib wiring**

In `voice.rs`, both functions become (replace the Task 1 `..Default::default()`):

```rust
pub fn update_clipper_params(
    &mut self, bypass: bool, os: ClipperOversampling, drive: f32,
    threshold: f32, bass: f32, tone: f32, asym: bool,
    mix: f32, in_db: f32, out_db: f32, tilt_db: f32, dynamics: f32,
) {
    self.clipper.set_params(ClipperParams {
        bypass,
        os: match os { ClipperOversampling::X2 => ClipperOs::X2, ClipperOversampling::X4 => ClipperOs::X4 },
        drive_ohms: drive, threshold_ohms: threshold, bass_ohms: bass,
        tone_f: tone, asym, mix, in_db, out_db, tilt_db, dynamics,
    });
}

pub fn update_clipper_params_immediate(
    &mut self, bypass: bool, os: ClipperOversampling, drive: f32,
    threshold: f32, bass: f32, tone: f32, asym: bool,
    mix: f32, in_db: f32, out_db: f32, tilt_db: f32, dynamics: f32,
) {
    self.clipper.set_params_immediate(ClipperParams {
        bypass,
        os: match os { ClipperOversampling::X2 => ClipperOs::X2, ClipperOversampling::X4 => ClipperOs::X4 },
        drive_ohms: drive, threshold_ohms: threshold, bass_ohms: bass,
        tone_f: tone, asym, mix, in_db, out_db, tilt_db, dynamics,
    });
}
```

In `lib.rs`, extend the block-rate call:

```rust
self.voice.update_clipper_params(
    self.params.clipper_bypass.value(), self.params.clipper_os.value(),
    self.params.clipper_drive.value(), self.params.clipper_threshold.value(),
    self.params.clipper_bass.value(), self.params.clipper_tone.value(),
    self.params.clipper_asym.value(),
    self.params.clipper_mix.value(), self.params.clipper_in_gain.value(),
    self.params.clipper_out_gain.value(), self.params.clipper_tilt.value(),
    self.params.clipper_dynamics.value(),
);
```

Update the four test call sites — append `, 1.0, 0.0, 0.0, 0.0, 0.0` to each
`update_clipper_params_immediate(...)` call (mix=1.0 wet preserves their
current behavior; they bypass the clipper anyway):
- `plugin303/tests/ultra_spectral.rs:25`
- `plugin303/tests/reso_gain_comp.rs:24`
- `plugin303/tests/tier_calibration.rs:158`
- `plugin303/tests/high_spectral.rs:36` and `:50`

Then grep for any missed callers: `rg "update_clipper_params" plugin303/` — every call must pass 12 args.

- [ ] **Step 3: Build and run tests**

Run: `cargo test -p plugin303`
Expected: all suites PASS. (No new tests in this task — it is wiring; the
param-smoke evidence is the full-suite green plus Task 5 GUI build.)

- [ ] **Step 4: Commit**

```bash
git add plugin303/src/params.rs plugin303/src/dsp/voice.rs plugin303/src/lib.rs plugin303/tests/
git commit -m "feat(params): clipper mix, gain link, tilt, dynamics parameters"
```

---

### Task 5: GUI row 2 + gain link

**Files:**
- Modify: `plugin303/src/gui/panels/output_clipper.rs`

**Interfaces:**
- Consumes: plugin params from Task 4, existing widgets `Knob303::new(&param, setter, "LABEL", 13.0)` and the checkbox pattern used for BYPASS/ASYM.

Link semantics (edit-time only, GUI-side; DAW automation is never rewritten):
- Link ON, IN moves → OUT := −IN.
- Link ON, OUT moves → IN := −OUT.
- Link toggled ON → OUT snaps to −IN immediately.

- [ ] **Step 1: Implement row 2**

Replace the body of `render_output_clipper` with:

```rust
pub fn render_output_clipper(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    Frame::new()
        .fill(theme::PANEL_BG)
        .stroke(theme::border_stroke())
        .inner_margin(Margin::same(7))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(nih_plug_egui::egui::RichText::new("OUTPUT CLIPPER").strong().color(theme::SOUPED_ORANGE));
                let mut bypass = params.clipper_bypass.value();
                if ui.checkbox(&mut bypass, "BYPASS").changed() {
                    setter.begin_set_parameter(&params.clipper_bypass);
                    setter.set_parameter(&params.clipper_bypass, bypass);
                    setter.end_set_parameter(&params.clipper_bypass);
                }
                ui.add(SlideSwitch::new(&params.clipper_os, setter, "OS", &["2X", "4X"]));
                ui.add(Knob303::new(&params.clipper_drive, setter, "DRIVE", 13.0));
                ui.add(Knob303::new(&params.clipper_threshold, setter, "THRESH", 13.0));
                ui.add(Knob303::new(&params.clipper_bass, setter, "BASS", 13.0));
                ui.add(Knob303::new(&params.clipper_tone, setter, "TONE", 13.0));
                let mut asym = params.clipper_asym.value();
                if ui.checkbox(&mut asym, "ASYM").changed() {
                    setter.begin_set_parameter(&params.clipper_asym);
                    setter.set_parameter(&params.clipper_asym, asym);
                    setter.end_set_parameter(&params.clipper_asym);
                }
                ui.colored_label(Color32::from_gray(120), "GE / JFET");
            });

            let bypassed = params.clipper_bypass.value();
            ui.add_enabled_ui(!bypassed, |ui| {
                ui.horizontal(|ui| {
                    ui.add(Knob303::new(&params.clipper_in_gain, setter, "IN", 13.0));
                    let mut link = params.clipper_gain_link.value();
                    if ui.checkbox(&mut link, "LINK").changed() {
                        setter.begin_set_parameter(&params.clipper_gain_link);
                        setter.set_parameter(&params.clipper_gain_link, link);
                        setter.end_set_parameter(&params.clipper_gain_link);
                        if link {
                            // Snap OUT to the inverse of IN when link engages.
                            setter.begin_set_parameter(&params.clipper_out_gain);
                            setter.set_parameter(&params.clipper_out_gain, -params.clipper_in_gain.value());
                            setter.end_set_parameter(&params.clipper_out_gain);
                        }
                    }
                    ui.add(Knob303::new(&params.clipper_out_gain, setter, "OUT", 13.0));
                    ui.add(Knob303::new(&params.clipper_tilt, setter, "TILT", 13.0));
                    ui.add(Knob303::new(&params.clipper_dynamics, setter, "DYN", 13.0));
                    ui.add(Knob303::new(&params.clipper_mix, setter, "MIX", 13.0));
                });
            });

            // Gain link enforcement: edit-time only. Compare against the values
            // seen last frame; whichever knob moved drives its counterpart.
            if params.clipper_gain_link.value() {
                let id = ui.id().with("clipper_gain_link_last");
                let last: Option<(f32, f32)> = ui.ctx().data_mut(|d| d.get_temp(id));
                let cur_in = params.clipper_in_gain.value();
                let cur_out = params.clipper_out_gain.value();
                if let Some((li, lo)) = last {
                    if cur_in != li {
                        setter.begin_set_parameter(&params.clipper_out_gain);
                        setter.set_parameter(&params.clipper_out_gain, -cur_in);
                        setter.end_set_parameter(&params.clipper_out_gain);
                    } else if cur_out != lo {
                        setter.begin_set_parameter(&params.clipper_in_gain);
                        setter.set_parameter(&params.clipper_in_gain, -cur_out);
                        setter.end_set_parameter(&params.clipper_in_gain);
                    }
                }
                ui.ctx().data_mut(|d| {
                    d.insert_temp(id, (params.clipper_in_gain.value(), params.clipper_out_gain.value()));
                });
            }
        });
}
```

If the installed egui version lacks `ctx.data_mut` closure form, use
`ui.memory_mut(|m| m.data.get_temp(id))` / `insert_temp` equivalently.

- [ ] **Step 2: Build**

Run: `cargo build -p plugin303`
Expected: PASS (GUI compiles; no new warnings).

- [ ] **Step 3: Run full tests**

Run: `cargo test -p plugin303`
Expected: all suites PASS.

- [ ] **Step 4: Manual GUI check (executor reports, user verifies)**

Launch standalone (`cargo run -p plugin303 --bin standalone` if available,
else note for user): LINK on, drag IN → OUT mirrors inverted; drag OUT → IN
mirrors; toggle LINK off → knobs independent; BYPASS greys row 2.

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/gui/panels/output_clipper.rs
git commit -m "feat(gui): clipper control row with gain link, tilt, dynamics, mix"
```

---

### Task 6: Eval tooling + regression verification

**Files:**
- Modify: `plugin303/src/bin/eval_clipper.rs`
- Modify: `eval/303/eval_clipper_aliasing.py`
- Modify: `eval/303/README_CLIPPER_ALIASING.md`

**Interfaces:**
- Consumes: `ClipperParams` fields (Task 1).

- [ ] **Step 1: Add CLI flags to eval_clipper.rs**

Replace the Task 1 `..ClipperParams::default()` literal with explicit fields.
Add locals near the other defaults (mix defaults to 1.0 so existing evals are
unchanged):

```rust
let mut mix: f32 = 1.0;
let mut in_gain: f32 = 0.0;
let mut out_gain: f32 = 0.0;
let mut tilt: f32 = 0.0;
let mut dynamics: f32 = 0.0;
```

Add match arms following the existing pattern:

```rust
"--mix" => {
    i += 1;
    mix = args[i].parse()?;
}
"--in-gain" => {
    i += 1;
    in_gain = args[i].parse()?;
}
"--out-gain" => {
    i += 1;
    out_gain = args[i].parse()?;
}
"--tilt" => {
    i += 1;
    tilt = args[i].parse()?;
}
"--dynamics" => {
    i += 1;
    dynamics = args[i].parse()?;
}
```

Update the params construction:

```rust
let params = ClipperParams {
    bypass,
    os: os_mode,
    drive_ohms: drive,
    threshold_ohms: thresh,
    bass_ohms: bass,
    tone_f: tone,
    asym,
    mix,
    in_db: in_gain,
    out_db: out_gain,
    tilt_db: tilt,
    dynamics,
};
```

Add usage lines after `--asym`:

```
--mix <float>       Dry/wet mix 0.0..1.0 (default: 1.0)
--in-gain <float>   Input gain dB -18..18 (default: 0.0)
--out-gain <float>  Output gain dB -18..18 (default: 0.0)
--tilt <float>      Pre/post tilt dB -6..6 (default: 0.0)
--dynamics <float>  Transient ducking 0.0..1.0 (default: 0.0)
```

- [ ] **Step 2: Extend the Python eval**

In `eval/303/eval_clipper_aliasing.py`, extend `render_clipper` signature and
command builder:

```python
def render_clipper(
    sr: float = 48000.0,
    os: str = "2x",
    bypass: bool = False,
    drive: float = 47000.0,
    thresh: float = 47000.0,
    bass: float = 8200.0,
    tone: float = 1e-9,
    asym: bool = False,
    signal: str = "sine",
    freq: float = 1700.0,
    amp: float = 1.0,
    secs: float = 1.0,
    mix: float = 1.0,
    in_gain: float = 0.0,
    out_gain: float = 0.0,
    tilt: float = 0.0,
    dynamics: float = 0.0,
) -> np.ndarray:
```

Append to `cmd` after `"--secs", str(secs),`:

```python
            "--mix", str(mix),
            "--in-gain", str(in_gain),
            "--out-gain", str(out_gain),
            "--tilt", str(tilt),
            "--dynamics", str(dynamics),
```

Add a new sweep function after `plot_drive_aliasing_sweep`:

```python
def run_control_sweeps(sr: float = 48000.0, f0: float = 2500.0):
    """Sweep 4: THD & ASR across the new control dimensions (mix, tilt, dynamics)."""
    print("[4/4] Running control-dimension sweeps (mix, tilt, dynamics)...")

    rows = []
    for m in [0.0, 0.25, 0.5, 0.75, 1.0]:
        s = render_clipper(sr=sr, os="2x", signal="sine", freq=f0, mix=m)
        met = compute_aliasing_metrics(s, sr, f0)
        rows.append({"sweep": "mix", "value": m, "thd_db": met["thd_db"], "asr_db": met["asr_db"]})
    for d in [0.0, 0.5, 1.0]:
        s = render_clipper(sr=sr, os="2x", signal="sine", freq=f0, drive=100000.0, dynamics=d)
        met = compute_aliasing_metrics(s, sr, f0)
        rows.append({"sweep": "dynamics", "value": d, "thd_db": met["thd_db"], "asr_db": met["asr_db"]})
    for t in [-6.0, 0.0, 6.0]:
        s = render_clipper(sr=sr, os="2x", signal="sine", freq=f0, tilt=t)
        met = compute_aliasing_metrics(s, sr, f0)
        rows.append({"sweep": "tilt", "value": t, "thd_db": met["thd_db"], "asr_db": met["asr_db"]})

    out_json = RESULTS_DIR / "clipper_control_sweeps.json"
    out_json.write_text(json.dumps(rows, indent=2))
    print(f"  -> saved {out_json}")
    for r in rows:
        print(f"  {r['sweep']:>8} = {r['value']:>5}: THD {r['thd_db']:.1f} dB, ASR {r['asr_db']:.1f} dB")
```

Call `run_control_sweeps(sr)` in `main()` after the existing plot calls.

Document the five new flags in `eval/303/README_CLIPPER_ALIASING.md` (CLI
options section) and the new `clipper_control_sweeps.json` output.

- [ ] **Step 3: Build binary and run eval**

Run: `cargo build --release --bin eval_clipper`
Expected: PASS.
Run: `uv run python eval/303/eval_clipper_aliasing.py`
Expected: existing plots regenerate; `clipper_control_sweeps.json` written;
default-config ASR values match the pre-change run within ±0.5 dB (defaults
are behavior-preserving: mix=1, gains 0 dB, tilt 0, dynamics 0).

- [ ] **Step 4: Final full verification**

Run: `cargo test -p plugin303`
Expected: all 21+ suites PASS.
Run: `rg "update_clipper_params" plugin303/` — confirm 8 matches (lib.rs
call, voice.rs definitions ×2, test call sites ×5), every call passing the
12-arg signature.

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/bin/eval_clipper.rs eval/303/eval_clipper_aliasing.py eval/303/README_CLIPPER_ALIASING.md
git commit -m "feat(eval): clipper control-dimension sweeps and CLI flags"
```
