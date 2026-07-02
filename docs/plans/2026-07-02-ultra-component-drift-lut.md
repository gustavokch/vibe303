# Plan: Precomputed Pitch LUTs for Integer Component Age and Drift

We will update the `comp_age` and `comp_drift` parameters to be integer controls (0 to 10). To address the real-time CPU performance constraints of the Ultra-tier circuit VCO (`Vco303`), we will precompute 11 distinct Pitch-to-CV LUTs (one for each age level from 0 to 10) during initialization/sample-rate changes. This allows component-level aging to physically shift the VCO frequency characteristics without requiring any real-time secant solvers or matrix re-characterization.

---

## Technical Details

### 1. Integer Parameters (params.rs)
We will change `comp_age` and `comp_drift` in `params.rs` from `FloatParam` to `IntParam` with a range of `0..=10`:
```rust
#[id = "comp_age"]
pub comp_age: IntParam,
#[id = "comp_drift"]
pub comp_drift: IntParam,
```

### 2. Multi-LUT Cache inside Vco303 (vco.rs)
- **Physical Aging:**
  We will introduce an `age` field to the raw `Vco303` model. During physical matrix assembly, the relaxation oscillator timing capacitor (`C_SAW`) is scaled by the age value:
  $$C_{\text{SAW}} = 100\text{ pF} \times (1.0 + \text{age} \times 0.15)$$
- **Cache Keys:**
  We will change `LUT_CACHE` to store mappings keyed by a tuple of sample rate and age index:
  ```rust
  static LUT_CACHE: Mutex<Vec<((u32, u32), [f64; LUT_K])>> = Mutex::new(Vec::new());
  ```
- **Instantiation:**
  When `Vco303` is initialized, it will look up and cache the 11 `cv_lut` arrays (for ages 0 to 10) at once using the process-global cache:
  ```rust
  pub struct Vco303 {
      // ...
      cv_luts: [[f64; LUT_K]; 11],
  }
  ```
- **Interpolation:**
  The `set_freq` method is updated to take the active `age_idx`:
  ```rust
  pub fn set_freq(&mut self, hz: f32, age_idx: usize)
  ```
  It will interpolate using `self.cv_luts[age_idx]`, running in O(1) time on the audio thread.

### 3. Voice Integration (voice.rs)
- The voice coordinator receives `comp_age` and `comp_drift` as integers in `update_params`.
- `comp_age` maps to `age_idx: usize` (clamped 0 to 10).
- Cutoff/resonance aging factors in the filter are scaled dynamically by `age_idx as f32 / 10.0`.
- In `tick()`, `set_freq(osc_f, age_idx)` is called, passing the active age level to the VCOs.

---

## Proposed Changes

### Component: Parameters & Shell

#### [MODIFY] [params.rs](file:///Users/gus/Git/vibe303/worktree-params/plugin303/src/params.rs)
- Replace `FloatParam` with `IntParam` for `comp_age` and `comp_drift`.

#### [MODIFY] [lib.rs](file:///Users/gus/Git/vibe303/worktree-params/plugin303/src/lib.rs)
- Read parameters as `IntParam::value()` and pass them to the voice.

---

### Component: DSP Engines

#### [MODIFY] [vco.rs](file:///Users/gus/Git/vibe303/worktree-params/plugin303/src/dsp/vco.rs)
- Parameterize the LCG, raw VCO, and secant measurement helper by `age: f64`.
- Precalculate and cache 11 Pitch LUTs.
- Update `set_freq` to interpolate using the active age-level's cached table.

#### [MODIFY] [voice.rs](file:///Users/gus/Git/vibe303/worktree-params/plugin303/src/dsp/voice.rs)
- Track `age_idx` and pass it to `vco.set_freq(osc_f, age_idx)`.

---

## Verification Plan

### Automated Tests
1. **Pitch Calibration test (`tests/vco_pitch.rs`)**:
   - Verify that the VCO tracks note frequencies accurately at different `comp_age` settings (0 to 10) without drift anomalies.
2. **Age & Drift test (`tests/age_drift.rs`)**:
   - Update tests to configure `comp_age` and `comp_drift` as integer parameters (0..=10) and verify that aging changes frequency outputs appropriately.
3. **No-Allocation test (`tests/no_alloc.rs`)**:
   - Verify that changing `comp_age` at block rate does not cause any heap allocations.

Run tests:
```bash
cargo test -p plugin303
```
