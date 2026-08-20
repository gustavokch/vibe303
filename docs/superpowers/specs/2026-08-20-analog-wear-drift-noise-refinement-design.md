# Analog Wear, Tolerance, Drift & Thermal Noise Refinement Design

**Date:** 2026-08-20  
**Status:** Validated & Approved  
**Target:** `vibe303` Monosynth (`plugin303`)

---

## 1. Executive Summary

This design refines and physically grounds the four analog character dimensions of the `vibe303` engine:
1. **Component Age (`comp_age`)**: Physics-based 0–45 year vintage wear on resistors ($+0..+5\%$), film capacitors ($-0..-5\%$), and BJT gain/leakage ($\beta_F -0..-15\%$), eliminating the negative cutoff/resonance macro bug in `voice.rs`.
2. **Tolerance & Seeding (`tolerance`, `comp_seed`)**: Fixes the 10× percentage decimal error (`0.5% = 0.005`, `2% = 0.02`, `5% = 0.05`, `10% = 0.10`) and directly wires `comp_seed` into `vco.rs` and `ladder_wdf.rs` PRNG instances to produce unique per-unit component variations.
3. **Trimmer Recalibration (`recalibration_scale`)**: A new parameter (`0.0..1.0`, default `1.0`) controlling vintage trimmer adjustment (TM4 Width, TM5 Tune). At `1.0`, calibration compensates for worn parts; at `0.0`, raw uncompensated circuit non-linearities, V/Oct slope errors, and octave stretch/compression appear.
4. **Multi-Process Drift & Thermal Harmonization (`comp_drift`, `thermal_noise`)**: Multi-channel de-correlated pink noise walks for pitch, cutoff, resonance Q, and dual-oscillator beat; harmonizes operating temperature ($20^\circ\text{C}..45^\circ\text{C}$) across VCO, Ladder, and LUT cache bins, fixing the unphysical ladder temperature calculation.

---

## 2. Parameter Surface & Control Mappings

### 2.1 Parameter Declarations in `params.rs`

| Parameter | Type | Range | Default | Unit / Formatter | Description |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **`comp_age`** | `IntParam` | `0..100` | `0` | `" yrs"` (`v as f32 * 0.45` -> 0.0..45.0 yrs) | Vintage component wear |
| **`tolerance`** | `EnumParam<CompTolerance>` | `[Zero, PointFive, Two, Five, Ten]` | `Zero` | `0%`, `0.5%`, `2%`, `5%`, `10%` | Component manufacturing grade |
| **`recalibration_scale`** | `FloatParam` | `0.0..1.0` | `1.0` | `"%"` (0..100%) | Trimmer recalibration accuracy (1.0 = perfect A440/1V-Oct, 0.0 = unserviced) |
| **`comp_drift`** | `FloatParam` | `0.0..1.0` | `0.1` | `"%"` (0..100%) | Low-frequency pink noise wandering depth |
| **`thermal_noise`** | `IntParam` | `0..100` | `0` | `"Off"` / `format!("{:.1}°C", 20.0 + v*0.25)` | Microscopic thermal noise & chassis temp |
| **`comp_seed`** | `IntParam` | `0..999_999` | `1337` | Numeric serial ID | Unique hardware unit instance seed |

### 2.2 Tolerance Percentage Decimal Correction
In `voice.rs`:
```rust
let tol_f64 = match tolerance {
    CompTolerance::Zero => 0.0f64,
    CompTolerance::PointFive => 0.005f64, // 0.5%
    CompTolerance::Two => 0.02f64,        // 2.0%
    CompTolerance::Five => 0.05f64,       // 5.0%
    CompTolerance::Ten => 0.10f64,        // 10.0%
};
let tol_f32 = tol_f64 as f32;
```

---

## 3. Physical Component Wear & Tolerance Modeling

### 3.1 Component Wear Equations in `vco.rs` and `ladder_wdf.rs`

In `Vco303` and `LadderVcf`, `set_analog_wear(tolerance: f64, age: i32, seed: u32)` calculates:

* **Resistor Multipliers:**
  $$R_{\text{mult}, i} = \text{clamp}\Big(1.0 + 0.05 \cdot \left(\frac{\text{age}}{100.0}\right) + \delta R_i,\, 0.75,\, 1.25\Big)$$
  where $\delta R_i \sim \mathcal{N}(0, \sigma_R^2)$ with $\sigma_R = \frac{\text{tolerance}}{3.0}$.

* **Capacitor Multipliers:**
  $$C_{\text{mult}, i} = \text{clamp}\Big(1.0 - 0.05 \cdot \left(\frac{\text{age}}{100.0}\right) + \delta C_i,\, 0.75,\, 1.25\Big)$$
  where $\delta C_i \sim \mathcal{N}(0, \sigma_C^2)$ with $\sigma_C = \frac{\text{tolerance}}{3.0}$.

* **Active BJT & Diode Models:**
  $$\beta_F = \beta_{F,\text{nom}} \cdot \Big(1.0 - 0.15 \cdot \left(\frac{\text{age}}{100.0}\right) + \delta\beta_i\Big)$$
  $$I_s = I_{s,\text{nom}} \cdot (1.0 + \delta I_{s, i})$$

### 3.2 Seeding & Instance De-correlation
The PRNG in `set_analog_wear` is initialized with:
* VCO1: `seed ^ (self.instance_id * 0x9E3779B9)`
* VCO2: `seed ^ (self.instance_id * 0x9E3779B9) ^ 0x33333333`
* Ladder (High/Ultra): `seed ^ (self.instance_id * 0x9E3779B9) ^ 0x55AA55AA`

### 3.3 Low Tier Calibration Mirroring
Low tier (polyBLEP + TPT ladder) derives static offsets to maintain cross-tier parity:
* Cutoff multiplier: $\text{cutoff\_mult}_{\text{static}} = \frac{1.0}{\text{mean}(C_{\text{mult}})}$
* Resonance multiplier: $\text{reso\_mult}_{\text{static}} = 1.0 - 0.10 \cdot \left(\frac{\text{age}}{100.0}\right)$
* Delete the broken `let age_f32 = self.age as f32 / 10.0;` macro scaling in `voice.rs`.

---

## 4. Pitch LUT Calibration & `recalibration_scale`

### 4.1 Uncompensated Pitch Deviation $\Delta p_{\text{uncal}}(k)$
For each pitch anchor $k \in [36..60]$ (semitones):
1. Evaluate raw circuit oscillation frequency $f_{\text{raw}}(k)$ at nominal factory CV under aged/toleranced components.
2. Compute uncompensated deviation:
   $$\Delta p_{\text{uncal}}(k) = 1200 \cdot \log_2\left(\frac{f_{\text{raw}}(k)}{f_{\text{nominal}}(k)}\right)\quad \text{[cents]}$$

### 4.2 Effective Calibration Target
Apply the trimmer adjustment scaling:
$$\Delta p_{\text{eff}}(k) = \text{clamp}\Big(\Delta p_{\text{uncal}}(k) \cdot (1.0 - \text{recalibration\_scale}),\, -30.0\text{ cents},\, +30.0\text{ cents}\Big)$$
$$f_{\text{target, eff}}(k) = f_{\text{nominal}}(k) \cdot 2^{\frac{\Delta p_{\text{eff}}(k)}{1200.0}}$$

The secant solver searches for $CV(k)$ such that $f(CV(k)) = f_{\text{target, eff}}(k)$ in at most 10 iterations. If the secant solver fails to converge within 10 iterations near anchor bounds $k=36, 60$ (or error $\ge 1\%$), it falls back to linear extrapolation/interpolation between adjacent converged anchors (e.g. $k=37, 38$ or $k=58, 59$).

### 4.3 Background Worker & Double-Buffered Swapping
* `LutJob` in `worker.rs` and `vco.rs` is extended:
  ```rust
  pub struct LutJob {
      pub sr: f32,
      pub instance_id: u32,
      pub is_fun: bool,
      pub age: i32,
      pub tolerance: f64,
      pub recal_scale: f32,
      pub seed: u32,
      pub sequence: u64,
      pub sequence_checker: Arc<AtomicU64>,
      pub on_complete: Arc<dyn Fn([[f64; LUT_K]; 22]) + Send + Sync>,
  }
  ```
* Fast Path: When `age == 0 && tolerance == 0.0 && recal_scale == 1.0`, process-global `LUT_CACHE` provides instant hit.
* Audio thread remains strictly zero-alloc and lock-free (`VcoLutBuffer`).

---

## 5. Multi-Process Drift & Thermal Engine

### 5.1 Multi-Process Random Walk
`Voice` maintains 4 independent `DriftGenerator` instances seeded deterministically on `comp_seed`:
1. `drift_pitch`: Pitch wandering ($\pm 12\text{ cents} \cdot \text{comp\_drift}$, $f_c \approx 0.15\text{ Hz}$).
2. `drift_cutoff`: Filter cutoff wandering ($\pm 8\% \cdot \text{comp\_drift}$, $f_c \approx 0.08\text{ Hz}$).
3. `drift_reso`: Resonance Q breathing ($\pm 3\% \cdot \text{comp\_drift}$, $f_c \approx 0.05\text{ Hz}$).
4. `drift_pitch_osc2`: Independent pitch wandering for secondary oscillator.

Remove unused/dead generators (`drift_pitch_thermal`, `drift_pitch_age`, `drift_cutoff_age`).

### 5.2 Operating Temperature & Thermal Noise Harmonization
* Ambient temperature: $T_{\text{ambient}} = 20.0 + 20.0 \cdot \left(\frac{\text{thermal\_noise}}{100.0}\right)^\circ\text{C}$.
* Dynamic note self-heating: $\Delta T_{\text{dynamic}} \le +3.0^\circ\text{C}$ ($\tau_{\text{warmup}} = 15\text{ s}$, $\tau_{\text{cooldown}} = 30\text{ s}$). $\Delta T_{\text{dynamic}}$ and overall `temperature` persist globally per synth engine instance (`Voice`) across note triggers, voice re-allocations, and note steals (initialized once on synth instantiation to $20.0^\circ\text{C}$, updated per-sample, and never reset on note-on).
* Unified operating temperature: $T_{\text{op}} = T_{\text{ambient}} + \Delta T_{\text{dynamic}} \in [20.0^\circ\text{C}, 45.0^\circ\text{C}]$.
* Pass $T_{\text{op}}$ to `Vco303::set_thermal` and `LadderVcf::set_thermal` (eliminating the $\times 10.0$ factor bug in ladder).
* `voice.rs` smoothly interpolates between adjacent bins in the 22-step LUT buffer.

---

## 6. Verification & Test Plan

1. **Automated Unit & Regression Tests:**
   * `test_analog_character_parameter_ranges`: Validates parameter bounds, units, and `recalibration_scale` declaration.
   * `test_tolerance_percentage_scaling`: Asserts standard deviation scaling on 0.5%, 2%, 5%, 10%.
   * `test_comp_seed_variation`: Verifies that distinct `comp_seed` values produce different component multipliers and distinct waveforms.
   * `test_recalibration_scale_pitch_deviation`: Asserts that `recalibration_scale = 1.0` yields exact A440 tuning under age/tolerance, while `recalibration_scale = 0.0` exhibits bounded pitch deviation $\le 30\text{ cents}$.
   * `test_temperature_bounds_and_no_negative_cutoff`: Asserts that `comp_age = 100` and `thermal_noise = 100` maintain positive, valid cutoff ($> 20\text{ Hz}$) and stable resonance.
   * `no_alloc.rs`: Asserts zero heap allocations on the audio path during parameter changes and async LUT buffer swaps.
   * Full test suite regression (`cargo test -p plugin303`).
