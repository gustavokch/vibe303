# Implementation Plan — Ultra Tier Aliasing Fix

Fix the severe aliasing in the Ultra tier's nodal relaxation VCO (`Vco303`) by applying polyBLEP correction to the reset discontinuity.

## Goal Description
The Ultra tier's nodal JFET relaxation VCO currently runs at the base sample rate ($f_s$), which means its sharp reset edge introduces severe aliasing into the signal chain before it is upsampled and filtered.
Instead of running the entire Newton solver at 4× oversampled rate (which is computationally prohibitive for real-time play, ~588% CPU core), we will detect the JFET reset timing and apply a first-order polyBLEP correction to the VCO's output in the base-rate domain before it enters the oversampler.

To do this accurately, we will:
1. Re-generate the SPICE gold reference waveform at a 4× finer timestep (1.25 µs) to inspect and extract precise physical circuit properties: the `CP` node's high/low voltage thresholds when JFET resets, and the output `o_r` peak-to-peak swing (`OR_SWING`).
2. Implement reset detection inside `Vco303::tick()` based on these thresholds and compute the fractional-sample phase (`blep_frac`) of the reset.
3. Import and apply the `poly_blep` correction in `Voice::render_tier(Tier::Ultra)` before the signal is fed to the 4× oversampler.
4. Verify using a new integration test (`tests/vco_alias.rs`) that asserts the polyBLEP correction reduces the VCO's alias energy by at least 6 dB.
5. Re-generate the committed gold references and calibrate the spectral gate test (`tests/ultra_spectral.rs`) using the new cleaner model.

## User Review Required
> [!IMPORTANT]
> The thresholds for capacitor discharge (`CP_HIGH` and `CP_LOW`) and output swing (`OR_SWING`) will be measured directly from the SPICE simulation at 4× OS in Step 0. The plan expects these to be hardcoded as constants in the Rust source code once determined.

## Proposed Changes

### Circuit / Tools / Evaluation

#### [MODIFY] [voice_ref_spice.py](file:///Users/gus/Git/vibe303/eval/303/voice_ref_spice.py)
- Expose a `step` parameter to `render_vco` (defaulting to `5e-6` / 200 kHz) so we can run the SPICE transient solver at a 4× finer step (1.25 µs / 800 kHz).
- Modify the solver step value to use this parameter.

#### [MODIFY] [gen_gold.py](file:///Users/gus/Git/vibe303/eval/303/gen_gold.py)
- Update to regenerate the gold file at the finer step size, and save the updated gold references `voice_A2_mid.f32` and `voice_A2_mid.json`.

---

### Rust Plugin DSP (`plugin303`)

#### [MODIFY] [osc.rs](file:///Users/gus/Git/vibe303/plugin303/src/dsp/osc.rs)
- Make `poly_blep` public (`pub`) so it can be reused in `voice.rs` for the Ultra tier's VCO.

#### [MODIFY] [vco.rs](file:///Users/gus/Git/vibe303/plugin303/src/dsp/vco.rs)
- Add `pub blep_frac: Option<f32>` to `Vco303`.
- In `tick()`, detect the JFET reset edge: when the capacitor voltage `v[CP]` drops from `CP_HIGH_THRESH` to below `CP_LOW_THRESH` compared to its previous value.
- Compute the linear fractional crossing point:
  `let alpha = (cp_prev - mid) / (cp_prev - cp_now).max(1e-9);`
  and assign `self.blep_frac = Some(alpha as f32)`.
- Reset `blep_frac` to `None` at the start of each `tick()`.

#### [MODIFY] [voice.rs](file:///Users/gus/Git/vibe303/plugin303/src/dsp/voice.rs)
- In `Voice::render_tier(Tier::Ultra)`, check `self.vco.blep_frac`.
- If `Some(frac)`, apply `poly_blep(frac, 1.0) * RESET_AMPLITUDE` to the output of `self.vco.tick()`.
- Define `RESET_AMPLITUDE` as a constant equal to `OR_SWING * VCO_GAIN`.

#### [NEW] [vco_alias.rs](file:///Users/gus/Git/vibe303/plugin303/tests/vco_alias.rs)
- Add a new integration test that renders the Ultra VCO (without the VCF ladder filter) at 880 Hz (note A5) for 8192 samples.
- Verify the spectral alias energy (off-harmonic energy) decreases by at least 6 dB when polyBLEP correction is active versus inactive (we can temporarily bypass it or toggle it to compare).

#### [MODIFY] [ultra_spectral.rs](file:///Users/gus/Git/vibe303/plugin303/tests/ultra_spectral.rs)
- Run the spectral gate test to evaluate the new LSD match vs the finer-timestep SPICE gold.
- Recalibrate `LSD_BUDGET_DB` and/or `ULTRA_BRIGHTNESS` if needed.

## Verification Plan

### Automated Tests
- Build and run the new integration test:
  `cargo test -p plugin303 --test vco_alias`
- Run the full plugin303 test suite:
  `cargo test -p plugin303`
- Run the python drift guard & validation tests:
  `uv run --no-project --with pytest python -m pytest tests/circuitgen/ -q`
  `uv run --no-project --with PySpice --with numpy python -m pytest eval/303/test_voice_ref.py -q`

### Manual Verification
- Re-run gold generation and confirm no compiler warnings or test failures.
- Inspect the output of the spectral gate test.
