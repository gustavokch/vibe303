# Phase 4: Voicing Calibration and CV/Pitch Tracking Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Calibrate and align the VCO pitch tracking and VCF voicing of the Low and High tiers to match the reference behavior of the Ultra tier.

**Architecture:** Use offline Python calibration scripts to sweep parameters and optimize fit (Nelder-Mead grid search for VCF LSD minimization, sub-sample zero-crossing search for VCO pitch). Fit polynomial correction curves (1D for VCO, 2D for VCF) and apply them to Low/High runtime DSP loops in `Voice::tick`.

**Tech Stack:** Rust (plugin303, nih-plug), Python 3.12 (scipy, numpy)

## Global Constraints
*   All code in the DSP audio path (`Voice::tick`, filters, etc.) must remain completely allocation-free.
*   Polynomial evaluation must be fast, inlineable, and computationally cheap.
*   The calibration scripts must run via `uv run --no-project --with numpy --with scipy`.

---

## File Structure & Responsibilities

*   Create: `plugin303/src/bin/vco_pitch_sweep.rs` — Rust helper binary to measure raw VCO pitch at nominal state using sub-sample zero-crossing logic.
*   Create: `eval/303/calibrate_vco_pitch.py` — Python script to fit a 3rd-order 1D polynomial to the VCO pitch deviations.
*   Create: `eval/303/calibrate_vcf_voicing.py` — Python script running a 2D grid sweep and Nelder-Mead optimization to minimize Welch LSD, fitting 2D polynomial surfaces.
*   Modify: `plugin303/src/bin/render303.rs` — Add `--gate-vca` and `--env-mod` CLI flags to allow steady-state rendering for the optimization sweep.
*   Modify: `plugin303/src/dsp/voice.rs` — Implement the public setter for VCA env mode, and implement the VCO/VCF polynomial corrections.
*   Create: `plugin303/tests/tier_calibration.rs` — Integration test suite verifying pitch parity within $\pm 0.1\%$ and VCF spectral match budgets.

---

### Task 1: Scaffolding and `render303` Enhancements

**Files:**
*   Modify: `plugin303/src/dsp/voice.rs:310-340` (Add public setter `set_vca_env_mode`)
*   Modify: `plugin303/src/bin/render303.rs:25-60` (Parse `--gate-vca` and `--env-mod` flags)

**Interfaces:**
*   Produces: `Voice::set_vca_env_mode(&mut self, mode: VcaEnvMode)`

- [ ] **Step 1: Implement public setter in `voice.rs`**
  Add the following method inside the `impl Voice` block in `plugin303/src/dsp/voice.rs` (e.g. near `set_wave`):
  ```rust
  pub fn set_vca_env_mode(&mut self, mode: VcaEnvMode) {
      self.vca_env_mode = mode;
  }
  ```

- [ ] **Step 2: Update `render303.rs` parsing**
  Modify `plugin303/src/bin/render303.rs` to parse the new flags and apply them:
  ```rust
  // Insert at parse area:
  let gate_vca = arg("--gate-vca", 0.0) > 0.5;
  let env_mod = arg("--env-mod", 0.0); // Default to 0.0 if not specified to allow static cutoff

  // Insert before rendering loop:
  if gate_vca {
      v.set_vca_env_mode(plugin303::dsp::voice::VcaEnvMode::Gate);
  }
  v.update_params(
      0.0, // tuning_cents
      0.0, // osc1_pitch_st
      0.0, // osc2_pitch_st
      0.0, // osc2_vol
      v.osc.wave, // osc1_wave (keep wave set by set_wave)
      0.5, // osc1_pw
      0.0, // sub_vol
      cutoff,
      reso / 100.0,
      env_mod,
      0.5, // decay_knob
      0.0, // accent_knob
      0.0, // volume_db
      plugin303::dsp::voice::VcfTaps::Four,
      0.0, // vcf_drive_db
      false, // advanced_env
      0.0, 0.0, 0.0, 0.0, // meg ADSR
      0.0, 0.0, 0.0, 0.0, // veg ADSR
      if gate_vca { plugin303::dsp::voice::VcaEnvMode::Gate } else { plugin303::dsp::voice::VcaEnvMode::Envelope },
      0, // comp_age
      0.0, // comp_drift
      plugin303::dsp::voice::CompTolerance::Zero,
      0, // thermal_noise
      plugin303::dsp::voice::CircuitTopology::Accurate,
      0.0, // sweep_pos
      0, // comp_seed
      0.5, // glide_knob
  );
  ```

- [ ] **Step 3: Run build to verify compilation**
  Run: `cargo build --bin render303 -p plugin303`
  Expected: Success

- [ ] **Step 4: Commit**
  ```bash
  git add plugin303/src/dsp/voice.rs plugin303/src/bin/render303.rs
  git commit -m "feat: add render303 flags for calibration gating"
  ```

---

### Task 2: VCO Pitch Calibration Implementation

**Files:**
*   Create: `plugin303/src/bin/vco_pitch_sweep.rs`
*   Create: `eval/303/calibrate_vco_pitch.py`

**Interfaces:**
*   Produces: Command-line output containing fitted 3rd-order polynomial coefficients for VCO pitch correction.

- [ ] **Step 1: Create VCO sweep binary in Rust**
  Create `plugin303/src/bin/vco_pitch_sweep.rs` with the following content:
  ```rust
  use plugin303::dsp::vco::Vco303;

  fn realized_hz(v: &mut Vco303, sr: f32) -> f32 {
      let n = (sr * 0.3) as usize;
      let mut prev = v.tick();
      let (mut first_t, mut last_t, mut count) = (-1.0f64, 0.0f64, 0i64);
      for i in 0..n {
          let y = v.tick();
          if prev < 0.0 && y >= 0.0 {
              let frac = (-prev as f64) / ((y - prev) as f64);
              let t = i as f64 + frac;
              if first_t < 0.0 {
                  first_t = t;
              } else {
                  last_t = t;
                  count += 1;
              }
          }
          prev = y;
      }
      if count == 0 || last_t <= first_t {
          return 0.0;
      }
      (count as f64 * sr as f64 / (last_t - first_t)) as f32
  }

  fn main() {
      let sr = 48000.0;
      println!("note,nominal_hz,realized_hz");
      for note in 24..=72 {
          let nominal = 440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0);
          let mut v = Vco303::new(sr);
          v.set_freq(nominal);
          for _ in 0..(sr * 0.1) as usize {
              v.tick();
          }
          let realized = realized_hz(&mut v, sr);
          println!("{},{:.6},{:.6}", note, nominal, realized);
      }
  }
  ```

- [ ] **Step 2: Create VCO calibration Python script**
  Create `eval/303/calibrate_vco_pitch.py` with the following content:
  ```python
  import subprocess
  import numpy as np
  import sys
  from pathlib import Path

  def main():
      root = Path(__file__).resolve().parent.parent.parent
      bin_path = root / "target" / "release" / "vco_pitch_sweep"
      if not bin_path.exists():
          bin_path = root / "target" / "debug" / "vco_pitch_sweep"
          
      print(f"Running VCO sweep from {bin_path}...")
      res = subprocess.run([str(bin_path)], capture_output=True, text=True, check=True)
      
      lines = res.stdout.strip().split("\n")
      data = []
      for line in lines[1:]:
          parts = line.split(",")
          if len(parts) == 3:
              note = float(parts[0])
              nominal = float(parts[1])
              realized = float(parts[2])
              data.append((note, nominal, realized))
              
      notes = np.array([d[0] for d in data])
      nominals = np.array([d[1] for d in data])
      realizeds = np.array([d[2] for d in data])
      
      dp = 12.0 * np.log2(realizeds / nominals)
      coeffs = np.polyfit(notes, dp, 3)
      
      print("\nFitted 3rd-order polynomial coefficients [a3, a2, a1, a0]:")
      print(f"a3 = {coeffs[0]:.12e}")
      print(f"a2 = {coeffs[1]:.12e}")
      print(f"a1 = {coeffs[2]:.12e}")
      print(f"a0 = {coeffs[3]:.12e}")
      
      fit_vals = np.polyval(coeffs, notes)
      residuals = dp - fit_vals
      print(f"\nMax fit residual error: {np.max(np.abs(residuals)) * 100.0:.4f} cents")

  if __name__ == "__main__":
      main()
  ```

- [ ] **Step 3: Build Rust sweep binary and run the calibration**
  Run: `cargo build --bin vco_pitch_sweep --release`
  Run: `uv run --no-project --with numpy python eval/303/calibrate_vco_pitch.py`
  Expected: Successful run, outputs coefficients with max fit residual error < 0.1 cents. Take note of these coefficients for Task 4.

- [ ] **Step 4: Commit**
  ```bash
  git add plugin303/src/bin/vco_pitch_sweep.rs eval/303/calibrate_vco_pitch.py
  git commit -m "feat: implement VCO pitch tracking calibration sweep"
  ```

---

### Task 3: VCF Voicing Calibration Implementation

**Files:**
*   Create: `eval/303/calibrate_vcf_voicing.py`

**Interfaces:**
*   Produces: Command-line output containing 2D polynomial surface coefficients for Low and High cutoff/resonance mapping.

- [ ] **Step 1: Create VCF calibration Python script**
  Create `eval/303/calibrate_vcf_voicing.py` with the following content:
  ```python
  import subprocess
  import numpy as np
  import scipy.optimize
  import sys
  from pathlib import Path
  import tempfile

  HERE = Path(__file__).resolve().parent
  sys.path.insert(0, str(HERE))
  import metrics as m

  RENDER303 = HERE.parent.parent / "target" / "release" / "render303"
  if not RENDER303.exists():
      RENDER303 = HERE.parent.parent / "target" / "debug" / "render303"

  SR = 48000.0
  SECS = 0.2

  def render_rust(note: int, cutoff_hz: float, reso_knob: float, tier_idx: int) -> np.ndarray:
      with tempfile.TemporaryDirectory() as d:
          out = Path(d) / "r.f32"
          subprocess.run(
              [
                  str(RENDER303),
                  "--note", str(note),
                  "--sr", str(SR),
                  "--secs", str(SECS),
                  "--cutoff", str(cutoff_hz),
                  "--reso", str(reso_knob * 100.0),
                  "--tier", str(tier_idx),
                  "--gate-vca", "1",
                  "--env-mod", "0.0",
                  "--out", str(out)
              ],
              check=True, capture_output=True
          )
          return np.fromfile(out, dtype="<f4").astype(np.float64)

  c_grid = np.array([0.0, 0.15, 0.3, 0.45, 0.6, 0.75, 0.9, 1.0])
  r_grid = np.array([0.0, 0.2, 0.4, 0.6, 0.8, 0.85])

  def fit_2d_poly(X, Y, Z, deg=3):
      terms = []
      for i in range(deg + 1):
          for j in range(deg + 1 - i):
              terms.append((i, j))
      
      A = []
      for x, y in zip(X, Y):
          row = [ (x**i) * (y**j) for i, j in terms ]
          A.append(row)
      A = np.array(A)
      coeffs, _, _, _ = np.linalg.lstsq(A, Z, rcond=None)
      return terms, coeffs

  def optimize_tier(tier_idx):
      print(f"\n--- Optimizing Tier {tier_idx} (0=Low, 1=High) ---")
      X_data, Y_data, Z_c_opt, Z_r_opt = [], [], [], []
      
      for c_nom in c_grid:
          for r_nom in r_grid:
              fc_nom = 200.0 * (20.0 ** c_nom)
              reso_nom = r_nom
              
              ultra_ref = render_rust(note=57, cutoff_hz=fc_nom, reso_knob=reso_nom, tier_idx=2)
              
              def objective(params):
                  c_cand, r_cand = params
                  c_cand = np.clip(c_cand, 0.0, 1.0)
                  r_cand = np.clip(r_cand, 0.0, 1.0)
                  
                  fc_cand = 200.0 * (20.0 ** c_cand)
                  reso_cand = r_cand
                  
                  cand = render_rust(note=57, cutoff_hz=fc_cand, reso_knob=reso_cand, tier_idx=tier_idx)
                  lsd, _ = m.spectral_match(ultra_ref, cand, sr=SR)
                  return lsd
              
              res = scipy.optimize.minimize(objective, [c_nom, r_nom], method='Nelder-Mead', options={'xatol': 1e-4, 'fatol': 1e-4})
              c_opt, r_opt = res.x
              c_opt = np.clip(c_opt, 0.0, 1.0)
              r_opt = np.clip(r_opt, 0.0, 1.0)
              
              print(f"Nominal (c={c_nom:.2f}, r={r_nom:.2f}) -> Opt (c={c_opt:.4f}, r={r_opt:.4f}), LSD = {res.fun:.2f} dB")
              
              X_data.append(c_nom)
              Y_data.append(r_nom)
              Z_c_opt.append(c_opt)
              Z_r_opt.append(r_opt)
              
      terms_c, coeffs_c = fit_2d_poly(X_data, Y_data, Z_c_opt, deg=3)
      terms_r, coeffs_r = fit_2d_poly(X_data, Y_data, Z_r_opt, deg=3)
      
      print(f"\n--- Polynomial Coefficients for Tier {tier_idx} ---")
      print("Cutoff (c_opt = sum k_ij * c^i * r^j):")
      for (i, j), k in zip(terms_c, coeffs_c):
          print(f"  ({i}, {j}) = {k:.12e}")
      print("Resonance (r_opt = sum k_ij * c^i * r^j):")
      for (i, j), k in zip(terms_r, coeffs_r):
          print(f"  ({i}, {j}) = {k:.12e}")

  def main():
      if not RENDER303.exists():
          print(f"Binary {RENDER303} missing. Run 'cargo build --release' first.")
          sys.exit(1)
      optimize_tier(0) # Low
      optimize_tier(1) # High

  if __name__ == "__main__":
      main()
  ```

- [ ] **Step 2: Build release tools and run VCF voicing calibration**
  Run: `cargo build --bin render303 --release`
  Run: `uv run --no-project --with numpy --with scipy python eval/303/calibrate_vcf_voicing.py`
  Expected: Successful run, outputs coefficients list for Low and High. Keep these matrices for Task 4.

- [ ] **Step 3: Commit**
  ```bash
  git add eval/303/calibrate_vcf_voicing.py
  git commit -m "feat: implement VCF voicing calibration script via Nelder-Mead optimization"
  ```

---

### Task 4: Runtime DSP Integration (VCO & VCF)

**Files:**
*   Modify: `plugin303/src/dsp/voice.rs` (around lines 770-830)

**Interfaces:**
*   Consumes: Polynomial coefficients calculated in Tasks 2 and 3.

- [ ] **Step 1: Add polynomial evaluation helper and coefficients to `voice.rs`**
  Add the helper function and static coefficients generated from Tasks 2 & 3 near the top or inside the `impl Voice` block in `plugin303/src/dsp/voice.rs`:
  ```rust
  // Helper to evaluate a 1D polynomial
  fn eval_poly_1d(x: f32, coeffs: &[f32]) -> f32 {
      let mut sum = 0.0;
      for (deg, &k) in coeffs.iter().enumerate() {
          sum += k * x.powi(deg as i32);
      }
      sum
  }

  // Helper to evaluate a 2D polynomial (up to degree 3)
  fn eval_poly_2d(x: f32, y: f32, terms: &[(usize, usize, f32)]) -> f32 {
      let mut sum = 0.0;
      for &(i, j, k) in terms {
          sum += k * x.powi(i as i32) * y.powi(j as i32);
      }
      sum
  }

  // Define static coefficients arrays using output from calibration sweeps.
  // Example formatting (replace placeholder numbers with actual outputs from Tasks 2 & 3):
  const VCO_PITCH_COEFFS: [f32; 4] = [0.0, 0.0, 0.0, 0.0]; // [a0, a1, a2, a3] -> matching polyval order
  const LOW_CUTOFF_COEFFS: &[(usize, usize, f32)] = &[];
  const LOW_RESO_COEFFS: &[(usize, usize, f32)] = &[];
  const HIGH_CUTOFF_COEFFS: &[(usize, usize, f32)] = &[];
  const HIGH_RESO_COEFFS: &[(usize, usize, f32)] = &[];
  ```

- [ ] **Step 2: Update VCO pitch calculations in `Voice::tick`**
  Modify pitch selection inside `Voice::tick` (lines 771-787) so that `osc1_f` and `osc2_f` are corrected for Low/High but nominal for Ultra:
  ```rust
  // Calculate target pitch in semitones
  let p_nominal = self.current_pitch_st - 69.0 + self.osc1_pitch_st + tuning_st + pitch_drift_st + pitch_thermal_st + 69.0;
  
  // Nominal frequency (used by Ultra VCO)
  let osc1_f_nominal = 440.0 * 2.0f32.powf((p_nominal - 69.0) / 12.0);
  
  // Corrected frequency (used by Low/High polyBLEP)
  let dp = eval_poly_1d(p_nominal, &VCO_PITCH_COEFFS);
  let osc1_f_corrected = 440.0 * 2.0f32.powf((p_nominal + dp - 69.0) / 12.0);
  
  self.osc.set_freq(osc1_f_corrected, self.sr);
  self.osc_hi.set_freq(osc1_f_corrected, self.sr);
  self.vco.set_freq(osc1_f_nominal);

  let sub_f_corrected = osc1_f_corrected * 0.5;
  self.sub_osc.set_freq(sub_f_corrected, self.sr);
  self.sub_osc_hi.set_freq(sub_f_corrected, self.sr);

  if self.osc2_vol > 0.0 {
      let p2_nominal = self.current_pitch_st - 69.0 + self.osc2_pitch_st + tuning_st + pitch_drift_st + pitch_thermal_st + 69.0;
      let osc2_f_nominal = 440.0 * 2.0f32.powf((p2_nominal - 69.0) / 12.0);
      let dp2 = eval_poly_1d(p2_nominal, &VCO_PITCH_COEFFS);
      let osc2_f_corrected = 440.0 * 2.0f32.powf((p2_nominal + dp2 - 69.0) / 12.0);
      
      self.osc2.set_freq(osc2_f_corrected, self.sr);
      self.osc2_hi.set_freq(osc2_f_corrected, self.sr);
      self.vco2.set_freq(osc2_f_nominal);
  }
  ```

- [ ] **Step 3: Update VCF parameter mappings in `Voice::tick`**
  Modify VCF parameter dispatch inside `Voice::tick` (lines 811-830) to compute and apply the polynomial mapping:
  ```rust
  // Calculate nominal control values in [0, 1] range
  let final_cutoff_hz = (mod_cutoff_hz * cutoff_drift_factor * cutoff_thermal_factor * age_cutoff_factor)
      .clamp(20.0, self.sr * 0.45);
  let final_reso = (self.reso_knob * age_reso_factor).clamp(0.0, 1.0);

  let c_nominal = (final_cutoff_hz / 200.0).ln() / 20.0f32.ln();
  let c_nominal = c_nominal.clamp(0.0, 1.0);
  let r_nominal = final_reso;

  // Ultra: bypasses correction
  self.ladder.set_cutoff(final_cutoff_hz as f32);
  self.ladder.set_reso(final_reso);

  // Low: TPT ladder
  let c_low = eval_poly_2d(c_nominal, r_nominal, LOW_CUTOFF_COEFFS).clamp(0.0, 1.0);
  let r_low = eval_poly_2d(c_nominal, r_nominal, LOW_RESO_COEFFS).clamp(0.0, 1.0);
  let fc_low = 200.0 * 20.0f32.powf(c_low);
  self.filter.set_cutoff(fc_low);
  self.filter.set_reso(r_low);

  // High: 2x oversampled nodal ladder
  let c_high = eval_poly_2d(c_nominal, r_nominal, HIGH_CUTOFF_COEFFS).clamp(0.0, 1.0);
  let r_high = eval_poly_2d(c_nominal, r_nominal, HIGH_RESO_COEFFS).clamp(0.0, 1.0);
  let fc_high = 200.0 * 20.0f32.powf(c_high);
  self.ladder_hi.set_cutoff(fc_high as f32);
  self.ladder_hi.set_reso(r_high);
  ```

- [ ] **Step 4: Run build to verify compilation**
  Run: `cargo build`
  Expected: Success

- [ ] **Step 5: Commit**
  ```bash
  git add plugin303/src/dsp/voice.rs
  git commit -m "feat: integrate VCO pitch and VCF voicing calibration polynomials into Voice DSP"
  ```

---

### Task 5: Verification Gates and Integration Tests

**Files:**
*   Create: `plugin303/tests/tier_calibration.rs`

**Interfaces:**
*   Consumes: `Voice` and `Tier` from `plugin303`.

- [ ] **Step 1: Create integration test suite**
  Create `plugin303/tests/tier_calibration.rs` with the following test validations:
  ```rust
  mod common;
  
  use common::{gold_dir, load_f32, load_gold_op, spectral_match};
  use plugin303::dsp::filter::Tier;
  use plugin303::dsp::voice::Voice;

  /// Sub-sample zero-crossing measurement helper for VCO raw pitch verification
  fn realized_pitch_note(v: &mut Voice, note: u8, sr: f32) -> f32 {
      v.note_on(note);
      v.set_gate(true, true, false, false);
      // Settle
      for _ in 0..(sr * 0.1) as usize {
          v.tick();
      }
      
      let n = (sr * 0.3) as usize;
      let mut prev = v.tick();
      let (mut first_t, mut last_t, mut count) = (-1.0f64, 0.0f64, 0i64);
      for i in 0..n {
          let y = v.tick();
          if prev < 0.0 && y >= 0.0 {
              let frac = (-prev as f64) / ((y - prev) as f64);
              let t = i as f64 + frac;
              if first_t < 0.0 {
                  first_t = t;
              } else {
                  last_t = t;
                  count += 1;
              }
          }
          prev = y;
      }
      if count == 0 || last_t <= first_t {
          return 0.0;
      }
      let hz = (count as f64 * sr as f64 / (last_t - first_t)) as f32;
      69.0 + 12.0 * (hz / 440.0).log2()
  }

  #[test]
  fn test_vco_pitch_parity() {
      let sr = 48000.0;
      for note in [24, 36, 48, 60, 72] {
          // Measure realized pitch on Ultra (source of truth)
          let mut v_ultra = Voice::new(sr);
          v_ultra.set_tier_immediate(Tier::Ultra);
          let p_ultra = realized_pitch_note(&mut v_ultra, note, sr);

          // Measure Low and verify within 0.1% frequency error (~1.7 cents)
          let mut v_low = Voice::new(sr);
          v_low.set_tier_immediate(Tier::Low);
          let p_low = realized_pitch_note(&mut v_low, note, sr);
          assert!((p_low - p_ultra).abs() < 0.017, "Low pitch error on note {note} too high: {p_low} vs {p_ultra}");

          // Measure High and verify within 0.1% frequency error
          let mut v_high = Voice::new(sr);
          v_high.set_tier_immediate(Tier::High);
          let p_high = realized_pitch_note(&mut v_high, note, sr);
          assert!((p_high - p_ultra).abs() < 0.017, "High pitch error on note {note} too high: {p_high} vs {p_ultra}");
      }
  }

  #[test]
  fn test_voicing_parity_lsd() {
      let gold = load_f32(&gold_dir().join("voice_A2_mid.f32"));
      let op = load_gold_op("voice_A2_mid.json");

      // Verify High tier vs Ultra/Gold: Welch LSD <= 2.0 dB, Correlation >= 0.98
      let mut v_high = Voice::new(op.sr);
      v_high.set_tier_immediate(Tier::High);
      v_high.set_cutoff(op.cutoff);
      v_high.set_reso(op.reso);
      v_high.note_on(op.note);
      let model_high: Vec<f32> = (0..gold.len()).map(|_| v_high.tick()).collect();
      let (lsd_high, corr_high) = spectral_match(&gold, &model_high, op.sr);
      println!("Calibrated High vs Gold: LSD = {lsd_high:.2} dB, corr = {corr_high:.3}");
      assert!(lsd_high <= 2.0, "High LSD {lsd_high:.2} dB exceeds 2.0 dB budget");
      assert!(corr_high >= 0.98, "High correlation {corr_high:.3} below 0.98 budget");

      // Verify Low tier vs Ultra/Gold: Welch LSD <= 3.5 dB, Correlation >= 0.95
      let mut v_low = Voice::new(op.sr);
      v_low.set_tier_immediate(Tier::Low);
      v_low.set_cutoff(op.cutoff);
      v_low.set_reso(op.reso);
      v_low.note_on(op.note);
      let model_low: Vec<f32> = (0..gold.len()).map(|_| v_low.tick()).collect();
      let (lsd_low, corr_low) = spectral_match(&gold, &model_low, op.sr);
      println!("Calibrated Low vs Gold: LSD = {lsd_low:.2} dB, corr = {corr_low:.3}");
      assert!(lsd_low <= 3.5, "Low LSD {lsd_low:.2} dB exceeds 3.5 dB budget");
      assert!(corr_low >= 0.95, "Low correlation {corr_low:.3} below 0.95 budget");
  }
  ```

- [ ] **Step 2: Run the test suite**
  Run: `cargo test -p plugin303 --test tier_calibration`
  Expected: All tests pass.

- [ ] **Step 3: Commit**
  ```bash
  git add plugin303/tests/tier_calibration.rs
  git commit -m "test: add VCF/VCO voicing calibration integration tests"
  ```
