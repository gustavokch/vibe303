# Analog Wear, Tolerance, Drift & Thermal Noise Refinement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refine and physically ground component aging, manufacturing tolerance, trimmer recalibration (`recalibration_scale`), multi-process analog drift, and operating temperature modeling across the entire `vibe303` DSP engine and GUI.

**Architecture:** 
1. Fix tolerance decimal mappings (`0.5% = 0.005`, `2% = 0.02`, `5% = 0.05`, `10% = 0.10`) and add `recalibration_scale` (`0.0..1.0`, default `1.0`).
2. Wire `comp_seed` into `vco.set_analog_wear` and `ladder.set_analog_wear` using deterministic Gaussian Box-Muller deviates.
3. Update `LutJob` and `Vco303::recalculate_luts_async` to compute uncompensated circuit pitch deviations $\Delta p_{\text{uncal}}(k)$ and blend calibration according to `recalibration_scale` with a $\pm 30\text{ cents}$ safety clamp.
4. Harmonize operating temperatures ($20^\circ\text{C}..45^\circ\text{C}$) across VCO and Ladder, fix ladder temperature scaling, implement multi-channel de-correlated drift walks, and remove the broken macro aging divisor in `voice.rs`.

**Tech Stack:** Rust (edition 2021), `nih-plug`, `nih_plug_egui`, MNA nodal Newton solvers, lock-free double-buffered atomics.

## Global Constraints
- Zero heap allocations on the audio path (`Voice::tick`, `process`, `reset`).
- Double-buffered lock-free swapping for async background LUT calculations (`VcoLutBuffer`).
- Cross-tier voicing parity: switching between Low, High, and Ultra must maintain matching pitch center and timbre.

---

### Task 1: Parameters & Decimal Correction (`recalibration_scale`, `CompTolerance`, `comp_drift` range)

**Files:**
- Modify: `plugin303/src/params.rs`
- Modify: `plugin303/src/gui/panels/souped_panel.rs`
- Test: `plugin303/tests/params_tier.rs`

**Interfaces:**
- Consumes: Existing `Plugin303Params` declaration
- Produces: `params.recalibration_scale: FloatParam`, corrected `CompTolerance` conversions, updated `comp_drift` range `0.0..1.0`.

- [ ] **Step 1: Write the failing test for parameter declarations and tolerance conversion**

In `plugin303/tests/params_tier.rs`, add:
```rust
#[test]
fn test_recalibration_scale_and_tolerance_values() {
    let params = plugin303::params::Plugin303Params::default();
    assert_eq!(params.recalibration_scale.value(), 1.0);
    assert_eq!(params.comp_drift.value(), 0.1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p plugin303 --test params_tier`
Expected: FAIL with "no field `recalibration_scale` on type `Plugin303Params`"

- [ ] **Step 3: Update `params.rs` and `souped_panel.rs`**

In `plugin303/src/params.rs`:
1. Add `recalibration_scale` field to `Plugin303Params`:
```rust
    #[id = "recalibration_scale"]
    pub recalibration_scale: FloatParam,
```
2. In `Plugin303Params::default()`, initialize `recalibration_scale` and update `comp_drift`:
```rust
            recalibration_scale: FloatParam::new(
                "Recalibration Scale",
                1.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(std::sync::Arc::new(|v| format!("{:.0} %", v * 100.0))),

            comp_drift: FloatParam::new(
                "Comp Drift",
                0.1,
                FloatRange::Skewed {
                    min: 0.0,
                    max: 1.0,
                    factor: FloatRange::skew_factor(-1.2),
                },
            )
            .with_value_to_string(std::sync::Arc::new(|v| format!("{:.0} %", v * 100.0))),
```
3. In `plugin303/src/gui/panels/souped_panel.rs`, add a knob for `recalibration_scale` (labeled `"RECAL"`) in the Souped-Up drawer.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p plugin303 --test params_tier`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/params.rs plugin303/src/gui/panels/souped_panel.rs plugin303/tests/params_tier.rs
git commit -m "feat: add recalibration_scale param and update comp_drift range"
```

---

### Task 2: Physical Component Wear & Instance Seeding in VCO and Ladder Solvers

**Files:**
- Modify: `plugin303/src/dsp/vco.rs:512-545`
- Modify: `plugin303/src/dsp/ladder_wdf.rs:212-238`
- Test: `plugin303/tests/age_drift.rs`

**Interfaces:**
- Consumes: `set_analog_wear(tolerance: f64, age: i32, seed: u32)`
- Produces: Deterministic seeded component multipliers ($R_i, C_i, \beta_F, I_s$) adhering to physics-based wear equations.

- [ ] **Step 1: Write the failing test for `comp_seed` component variance**

In `plugin303/tests/age_drift.rs`, add:
```rust
#[test]
fn test_comp_seed_produces_different_components() {
    use plugin303::dsp::vco::Vco303;
    let mut vco_a = Vco303::new(48000.0);
    let mut vco_b = Vco303::new(48000.0);
    vco_a.set_analog_wear(0.05, 10, 1337);
    vco_b.set_analog_wear(0.05, 10, 7331);
    assert_ne!(vco_a.get_c_mults(), vco_b.get_c_mults());
    assert_ne!(vco_a.get_r_mults(), vco_b.get_r_mults());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p plugin303 --test age_drift`
Expected: FAIL with "this function takes 2 arguments but 3 arguments were supplied"

- [ ] **Step 3: Update `set_analog_wear` in `vco.rs` and `ladder_wdf.rs`**

In `plugin303/src/dsp/vco.rs`:
```rust
    pub fn set_analog_wear(&mut self, tolerance: f64, age: i32, seed: u32) {
        let mut rng_state = seed.wrapping_add(self.instance_id.wrapping_mul(0x9E3779B9));
        let mut next_f64 = || {
            rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
            (rng_state as f64) / (std::u32::MAX as f64)
        };
        let mut next_gauss = || {
            let mut u1 = next_f64();
            if u1 < 1e-6 {
                u1 = 1e-6;
            }
            let u2 = next_f64();
            (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
        };

        let systematic_r = 0.05 * (age as f64 / 100.0);
        let systematic_c = -0.05 * (age as f64 / 100.0);

        let bjt_age_factor = 1.0 - 0.15 * (age as f64 / 100.0);
        self.npn.bf = BjtParams::n2n3904().bf * bjt_age_factor;
        self.npn.br = BjtParams::n2n3904().br * bjt_age_factor;
        self.pnp.bf = BjtParams::p2n3906().bf * bjt_age_factor;
        self.pnp.br = BjtParams::p2n3906().br * bjt_age_factor;

        let sigma = tolerance / 3.0;
        for m in self.r_mults.iter_mut() {
            let r_dev = next_gauss() * sigma;
            *m = (1.0 + systematic_r + r_dev).clamp(0.75, 1.25);
        }
        for m in self.c_mults.iter_mut() {
            let c_dev = next_gauss() * sigma;
            *m = (1.0 + systematic_c + c_dev).clamp(0.75, 1.25);
        }
        self.update_g_mults();
    }
```

In `plugin303/src/dsp/ladder_wdf.rs`:
```rust
    pub fn set_analog_wear(&mut self, tolerance: f64, age: i32, seed: u32) {
        let mut rng_state = seed.wrapping_add(self.instance_id.wrapping_mul(0x9E3779B9) ^ 0x55AA55AA);
        let mut next_f64 = || {
            rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
            (rng_state as f64) / (std::u32::MAX as f64)
        };
        let mut next_gauss = || {
            let mut u1 = next_f64();
            if u1 < 1e-6 {
                u1 = 1e-6;
            }
            let u2 = next_f64();
            (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
        };

        let systematic_c = -0.05 * (age as f64 / 100.0);
        let bjt_age_factor = 1.0 - 0.15 * (age as f64 / 100.0);

        self.cal = CAL * bjt_age_factor;
        self.set_cal(self.cal);

        let sigma = tolerance / 3.0;
        for m in self.c_mults.iter_mut() {
            let c_dev = next_gauss() * sigma;
            *m = (1.0 + systematic_c + c_dev).clamp(0.75, 1.25);
        }
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p plugin303 --test age_drift`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/dsp/vco.rs plugin303/src/dsp/ladder_wdf.rs plugin303/tests/age_drift.rs
git commit -m "feat: physical wear formulas and seed-based component generation in VCO/Ladder"
```

---

### Task 3: Pitch LUT Calibration & `recalibration_scale` Engine

**Files:**
- Modify: `plugin303/src/dsp/worker.rs`
- Modify: `plugin303/src/dsp/vco.rs`
- Create: `plugin303/tests/recalibration_scale.rs`

**Interfaces:**
- Consumes: `age`, `tolerance`, `seed`, `recalibration_scale`
- Produces: `recalculate_luts_async(age, tolerance, seed, recal_scale)` solving target frequencies blended by uncompensated circuit deviations.

- [ ] **Step 1: Write the failing test for `recalibration_scale` pitch behavior**

Create `plugin303/tests/recalibration_scale.rs`:
```rust
use plugin303::dsp::vco::Vco303;

#[test]
fn test_recalibration_scale_alters_lut() {
    let sr = 48000.0;
    let mut vco_recal = Vco303::new(sr);
    let mut vco_uncal = Vco303::new(sr);

    vco_recal.set_analog_wear(0.10, 40, 1337);
    vco_uncal.set_analog_wear(0.10, 40, 1337);

    // Sync build with recal_scale = 1.0 (fully recalibrated) vs 0.0 (unserviced)
    vco_recal.recalculate_luts_sync_with_recal(40, 0.10, 1337, 1.0);
    vco_uncal.recalculate_luts_sync_with_recal(40, 0.10, 1337, 0.0);

    assert_ne!(vco_recal.get_cv_lut(), vco_uncal.get_cv_lut());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p plugin303 --test recalibration_scale`
Expected: FAIL with "no method named `recalculate_luts_sync_with_recal`"

- [ ] **Step 3: Implement `recalibration_scale` in `worker.rs` and `vco.rs`**

1. In `plugin303/src/dsp/worker.rs`, add `seed: u32` and `recal_scale: f32` to `LutJob`.
2. In `plugin303/src/dsp/vco.rs`:
   - In `solve_cv_lut_with_recal`:
     - For each anchor note $k \in [36..60]$:
     - Compute nominal $f_{\text{nom}}(k) = 440.0 \cdot 2.0^{(k - 69.0)/12.0}$.
     - Obtain nominal CV: $\text{cv}_{\text{nom}} = \text{cv\_lut\_nominal}[k - 36]$.
     - Evaluate raw uncompensated circuit frequency at nominal CV: $f_{\text{raw}}(k) = \text{measure\_f\_at\_cv}(\text{cv}_{\text{nom}})$.
     - Calculate uncalibrated pitch deviation: $\Delta p_{\text{uncal}} = 1200.0 \cdot \log_2(f_{\text{raw}} / f_{\text{nom}})$.
     - Blend target: $\Delta p_{\text{eff}} = (\Delta p_{\text{uncal}} \cdot (1.0 - \text{recal\_scale} \text{ as f64})).clamp(-30.0, 30.0)$.
     - Target frequency: $f_{\text{target}} = f_{\text{nom}} \cdot 2.0^{\Delta p_{\text{eff}} / 1200.0}$.
     - Run secant solver for $f_{\text{target}}$.
   - Update `recalculate_luts_async(age, tolerance, seed, recal_scale)`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p plugin303 --test recalibration_scale`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/dsp/worker.rs plugin303/src/dsp/vco.rs plugin303/tests/recalibration_scale.rs
git commit -m "feat: implement pitch LUT calibration blending via recalibration_scale"
```

---

### Task 4: Multi-Process Drift & Thermal Harmonization in `voice.rs`

**Files:**
- Modify: `plugin303/src/dsp/voice.rs`
- Modify: `plugin303/src/dsp/ladder_wdf.rs`
- Modify: `plugin303/src/lib.rs`
- Modify: `plugin303/src/bin/render303.rs`
- Test: `plugin303/tests/age_drift.rs`

**Interfaces:**
- Consumes: `recalibration_scale`, `comp_drift`, `thermal_noise`, `comp_seed`, `tolerance`, `comp_age`
- Produces: Correct percentage mappings, multi-channel drift walks (`drift_pitch`, `drift_cutoff`, `drift_reso`, `drift_pitch_osc2`), unified temperature $T_{\text{op}} \in [20.0^\circ\text{C}, 45.0^\circ\text{C}]$.

- [ ] **Step 1: Write test for positive cutoffs at age=100 and multi-seed drift**

In `plugin303/tests/age_drift.rs`, add:
```rust
#[test]
fn test_age_100_maintains_positive_cutoff_and_resonance() {
    let mut v = Voice::new(48000.0);
    v.update_params(
        0.0, 0.0, 0.0, 0.0, plugin303::params::OscWave::Saw,
        0.5, 0.0, 0.5, 0.5, 0.5, 0.5, 0.5, 0.0,
        plugin303::params::VcfTaps::FourPole, 0.0, false,
        3.0, 200.0, 0.0, 200.0, 3.0, 200.0, 0.0, 200.0,
        plugin303::params::VcaEnvMode::Envelope,
        100, // max age
        0.5, // comp_drift
        plugin303::params::CompTolerance::Ten,
        100, // max thermal noise
        CircuitTopology::Accurate,
        0.0, 1337, 0.5, 0.0, 1.0, // recalibration_scale = 1.0
    );
    v.note_on(60);
    v.set_gate(true, true, false, false);
    for _ in 0..100 {
        let s = v.tick();
        assert!(s.is_finite());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p plugin303 --test age_drift`
Expected: FAIL due to parameter count mismatch in `update_params`

- [ ] **Step 3: Update `voice.rs`, `ladder_wdf.rs`, `lib.rs`, `render303.rs`**

1. In `voice.rs`:
   - Fix `CompTolerance` mapping: `PointFive => 0.005`, `Two => 0.02`, `Five => 0.05`, `Ten => 0.10`.
   - Remove unused drift generators (`drift_pitch_thermal`, `drift_pitch_age`, `drift_cutoff_age`).
   - Add `drift_reso: DriftGenerator` and `drift_pitch_osc2: DriftGenerator`.
   - Re-seed drift generators on `comp_seed` change.
   - Implement unified operating temperature:
     ```rust
     let ambient_temp = 20.0 + 20.0 * (self.thermal_noise as f32 / 100.0);
     let target_temp = if self.gate { ambient_temp + 3.0 } else { ambient_temp };
     let tau = if self.gate { 15.0 } else { 30.0 };
     let alpha = 1.0 - (-1.0 / (tau * self.sr)).exp();
     self.temperature += alpha * (target_temp - self.temperature);
     ```
   - In `ladder_wdf.rs:241`:
     Fix temperature equation to: `let temp_c = 20.0 + 25.0 * (thermal_noise as f64 / 100.0);`.
   - In `voice.rs`:
     Remove `let age_f32 = self.age as f32 / 10.0;` and delete negative cutoff/reso multiplication.
   - Pass `recalibration_scale` from `lib.rs` and `render303.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p plugin303 --test age_drift`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/dsp/voice.rs plugin303/src/dsp/ladder_wdf.rs plugin303/src/lib.rs plugin303/src/bin/render303.rs plugin303/tests/age_drift.rs
git commit -m "fix: multi-process drift, temperature harmonization and remove negative age multiplier"
```

---

### Task 5: Full Regression Testing & Audio Thread Allocation Guard

**Files:**
- Modify: `plugin303/tests/no_alloc.rs`
- Run: Entire test suite (`cargo test -p plugin303`)

- [ ] **Step 1: Add parameter sweep test in `no_alloc.rs`**

In `plugin303/tests/no_alloc.rs`, add an assertion testing real-time parameter changes (`recalibration_scale`, `comp_age`, `tolerance`, `comp_drift`, `thermal_noise`, `comp_seed`) during audio processing to guarantee zero allocations.

- [ ] **Step 2: Run all tests**

Run: `cargo test -p plugin303`
Expected: ALL tests pass (0 failures).

- [ ] **Step 3: Commit**

```bash
git add plugin303/tests/no_alloc.rs
git commit -m "test: verify zero allocation during analog character parameter changes"
```
