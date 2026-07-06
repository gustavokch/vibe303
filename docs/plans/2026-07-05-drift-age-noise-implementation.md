# Analog Character Modeling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rethink the voice drift, component age, and manufacturing tolerance parameters to map to accurate physical analog behaviors, and add a discrete thermal noise parameter with asynchronous, background-threaded CV-Pitch LUT calibration to prevent audio thread glitches.

**Architecture:** Component age is modeled as systematic wear (resistors drift up, capacitors dry out, transistor gain decreases), while tolerance is modeled as a seed-specific random Gaussian deviation per component instance. Thermal noise is mapped to 22 discrete states, and all 22 states' CV-Pitch LUTs are precalculated in a background thread upon parameter/sample-rate changes and double-buffered for lock-free, zero-allocation audio-thread access. JFET reset phase jitter and linear MNA node noise injection are added to the oscillator and filter solvers.

**Tech Stack:** Rust (nih-plug, standard threading and atomic primitives, `assert_no_alloc`).

## Global Constraints

- **Audio Thread Safety:** Zero heap allocation or deallocation inside `tick()`, `process()`, or `reset()`.
- **Pitch Tracking Calibration:** Overall pitch tracking must be calibrated to A440 (using secant CV search) under the selected age/tolerance values.
- **Background Recalculation:** Re-characterization of the CV-Pitch LUTs must happen in a background thread to prevent audio thread dropouts/clicks.
- **Line Length:** Line-length 100 for all code and tests.
- **x86-64-v3 baseline:** Retain AVX2 target compliance.

---

### Task 1: Parameter Definitions & UI Formatting

**Files:**
- Modify: [params.rs](file:///Users/gus/Git/vibe303/plugin303/src/params.rs)
- Modify: [lib.rs](file:///Users/gus/Git/vibe303/plugin303/src/lib.rs)
- Test: [params_tier.rs](file:///Users/gus/Git/vibe303/plugin303/tests/params_tier.rs)

**Interfaces:**
- Consumes: Nothing new.
- Produces: Updated parameter fields in `Plugin303Params` with the restored `comp_drift` (`FloatParam`) and updated `CompTolerance` and `thermal_noise` formatters.

- [ ] **Step 1: Write the failing test**
  Add tests to [params_tier.rs](file:///Users/gus/Git/vibe303/plugin303/tests/params_tier.rs) to verify the new parameter types, bounds, and string formatting:
  ```rust
  #[test]
  fn test_analog_character_parameter_ranges() {
      let params = Plugin303Params::default();
      assert_eq!(params.comp_age.value(), 0);
      assert_eq!(params.comp_drift.value(), 0.0);
      assert_eq!(params.tolerance.value(), CompTolerance::Zero);
      assert_eq!(params.thermal_noise.value(), 0);
      
      // Check formatting of thermal_noise
      let format_zero = params.thermal_noise.to_string();
      assert_eq!(format_zero, "Off");
  }
  ```

- [ ] **Step 2: Run test to verify it fails**
  Run: `cargo test -p plugin303 --test params_tier`
  Expected: FAIL (comp_drift not found, ranges mismatched)

- [ ] **Step 3: Modify params.rs and lib.rs**
  Update `Plugin303Params` to define:
  ```rust
  // plugin303/src/params.rs
  #[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Enum)]
  pub enum CompTolerance {
      #[default]
      #[name = "0%"]
      Zero,
      #[name = "0.5%"]
      PointFive,
      #[name = "2%"]
      Two,
      #[name = "5%"]
      Five,
      #[name = "10%"]
      Ten,
  }

  // Inside Plugin303Params:
  #[id = "comp_age"]
  pub comp_age: IntParam,          // 0..=10
  #[id = "comp_drift"]
  pub comp_drift: FloatParam,      // 0.0..=1.0
  #[id = "tolerance"]
  pub tolerance: EnumParam<CompTolerance>,
  #[id = "thermal_noise"]
  pub thermal_noise: IntParam,    // 0..=21 (0 = Off, 1..21 = 1.0..5.0 in 0.2 steps)
  ```
  Implement the custom string formatter for `thermal_noise` inside `Plugin303Params::default()`:
  ```rust
  thermal_noise: IntParam::new(
      "Thermal Noise",
      0,
      IntRange::Linear { min: 0, max: 21 }
  ).with_value_to_string(Arc::new(|v| {
      if v == 0 {
          "Off".to_string()
      } else {
          format!("{:.1}", 1.0 + (v - 1) as f32 * 0.2)
      }
  })),
  ```
  Update [lib.rs](file:///Users/gus/Git/vibe303/plugin303/src/lib.rs) to read `comp_drift` in `process()` and pass it to the voice:
  ```rust
  self.voice.update_params(
      // ...
      self.params.comp_age.value(),
      self.params.comp_drift.value(),
      self.params.tolerance.value(),
      self.params.thermal_noise.value(),
      // ...
  );
  ```

- [ ] **Step 4: Run test to verify it passes**
  Run: `cargo test -p plugin303 --test params_tier`
  Expected: PASS

- [ ] **Step 5: Commit**
  Run: `git commit -am "feat: define analog character parameters and formatting"`

---

### Task 2: Physical Wear & Tolerance Modeling in VCO and VCF

**Files:**
- Modify: [vco.rs](file:///Users/gus/Git/vibe303/plugin303/src/dsp/vco.rs)
- Modify: [ladder_wdf.rs](file:///Users/gus/Git/vibe303/plugin303/src/dsp/ladder_wdf.rs)
- Modify: [age_drift.rs](file:///Users/gus/Git/vibe303/plugin303/tests/age_drift.rs)

**Interfaces:**
- Consumes: Updated parameter types.
- Produces: `set_analog_wear` and `set_tolerance` methods that apply physical resistor, capacitor, and BJT changes using deterministic, instance-specific seeds.

- [ ] **Step 1: Write the failing test**
  Modify [age_drift.rs](file:///Users/gus/Git/vibe303/plugin303/tests/age_drift.rs) to assert that:
  - Age systematically shifts VCO cap and resistor values.
  - Different VCO instances (VCO1 and VCO2) get different static random values for the same tolerance setting.
  ```rust
  #[test]
  fn test_instance_specific_tolerance_seeds() {
      let mut vco1 = Vco303::new(48000.0);
      let mut vco2 = Vco303::new(48000.0);
      vco1.set_instance_id(1);
      vco2.set_instance_id(2);
      vco1.set_analog_wear(CompTolerance::Five, 0);
      vco2.set_analog_wear(CompTolerance::Five, 0);
      // Verify that capacitor multipliers differ due to separate seeds
      assert_ne!(vco1.get_c_mults(), vco2.get_c_mults());
  }
  ```

- [ ] **Step 2: Run test to verify it fails**
  Run: `cargo test -p plugin303 --test age_drift`
  Expected: FAIL (compilation errors / same multipliers)

- [ ] **Step 3: Implement Systematic Wear and Instance Seeds**
  In [vco.rs](file:///Users/gus/Git/vibe303/plugin303/src/dsp/vco.rs) and [ladder_wdf.rs](file:///Users/gus/Git/vibe303/plugin303/src/dsp/ladder_wdf.rs):
  - Add an `instance_id: u32` field and setters to `Vco303` and `LadderVcf`.
  - Update `set_analog_wear` to apply:
    - **Systematic Aging:**
      - Resistors: scale up systematically: $R \times (1.0 + \text{age} \times 0.015)$, clamped to $+10\%$.
      - Capacitors: scale down systematically: $C \times (1.0 - \text{age} \times 0.020)$, clamped to $-10\%$.
      - BJTs: scale BF/BR down systematically: $BF \times (1.0 - \text{age} \times 0.030)$, clamped to $-10\%$.
    - **Mfg Tolerance:**
      - Seed a LCG using `instance_id` + fixed offset.
      - Generate Gaussian offsets via Box-Muller. Scale standard deviation $\sigma$ based on the selected `CompTolerance`:
        - `Zero` $\rightarrow \sigma = 0.0$
        - `PointFive` $\rightarrow \sigma = 0.00167$
        - `Two` $\rightarrow \sigma = 0.00667$
        - `Five` $\rightarrow \sigma = 0.0167$
        - `Ten` $\rightarrow \sigma = 0.0333$
      - Resistors and capacitors multipliers are shifted by $1.0 + \text{gaussian} \times \sigma$.

- [ ] **Step 4: Run test to verify it passes**
  Run: `cargo test -p plugin303 --test age_drift`
  Expected: PASS

- [ ] **Step 5: Commit**
  Run: `git commit -am "feat: implement physical wear, tolerance tiers, and instance seeds"`

---

### Task 3: Asynchronous CV-Pitch LUT Precalculation

**Files:**
- Modify: [vco.rs](file:///Users/gus/Git/vibe303/plugin303/src/dsp/vco.rs)
- Modify: [no_alloc.rs](file:///Users/gus/Git/vibe303/plugin303/tests/no_alloc.rs)

**Interfaces:**
- Consumes: Static age, tolerance, and sample rate.
- Produces: Double-buffered `cv_luts` precalculation running asynchronously on a background worker thread.

- [ ] **Step 1: Write the failing test**
  Add a test to [no_alloc.rs](file:///Users/gus/Git/vibe303/plugin303/tests/no_alloc.rs) that updates `comp_age` or `tolerance` and asserts that the real-time audio thread does not allocate during or after the swap:
  ```rust
  #[test]
  fn test_lut_update_no_alloc_on_audio_thread() {
      let mut vco = Vco303::new(48000.0);
      assert_no_alloc(|| {
          vco.tick(); // Tick must remain alloc-free even if background calculation is running/completing
      });
  }
  ```

- [ ] **Step 2: Run test to verify it fails**
  Run: `cargo test -p plugin303 --test no_alloc`
  Expected: FAIL

- [ ] **Step 3: Implement Double-Buffered Background Re-tuning**
  Modify [vco.rs](file:///Users/gus/Git/vibe303/plugin303/src/dsp/vco.rs):
  - Change `cv_luts` to double buffer: `cv_luts: [[[f64; LUT_K]; 22]; 2]`.
  - Add `active_lut_idx: Arc<AtomicUsize>`.
  - Implement `pub fn update_luts_async(&self)`:
    - Spawn a standard thread or queue task.
    - The thread creates a clone of the `Vco303` solver, applies the current age and tolerance, and for each of the 22 noise steps, updates the transistor temperature and runs the secant solver to find the CVs for the 49 anchors.
    - Write the computed LUTs directly to the inactive index of the parent's `cv_luts`.
    - Once finished, write the active index atomic swap:
      `self.active_lut_idx.store(inactive_idx, Ordering::Release)`.
  - Ensure concurrent spawns are guarded or queued safely.

- [ ] **Step 4: Run test to verify it passes**
  Run: `cargo test -p plugin303 --test no_alloc`
  Expected: PASS

- [ ] **Step 5: Commit**
  Run: `git commit -am "feat: implement double-buffered asynchronous LUT recalculation"`

---

### Task 4: Thermal Warming Curve & Voice Integration

**Files:**
- Modify: [voice.rs](file:///Users/gus/Git/vibe303/plugin303/src/dsp/voice.rs)
- Modify: [lib.rs](file:///Users/gus/Git/vibe303/plugin303/src/lib.rs)

**Interfaces:**
- Consumes: `comp_drift`, `comp_age`, `tolerance`, `thermal_noise`.
- Produces: Integrated thermal temperature state and CV drift scaled by `comp_drift`.

- [ ] **Step 1: Write the failing test**
  Modify [age_drift.rs](file:///Users/gus/Git/vibe303/plugin303/tests/age_drift.rs) to verify that playing notes causes the voice's internal temperature to rise slowly, and Vco303 and LadderVcf track this temperature shift.
  ```rust
  #[test]
  fn test_thermal_warming_curve() {
      let mut voice = Voice::new(48000.0);
      voice.update_params(
          // ... with active gate
      );
      let t0 = voice.get_current_temperature();
      for _ in 0..48000 { voice.tick(); } // 1 second of play
      let t1 = voice.get_current_temperature();
      assert!(t1 > t0, "Temperature should rise when notes are active");
  }
  ```

- [ ] **Step 2: Run test to verify it fails**
  Run: `cargo test -p plugin303 --test age_drift`
  Expected: FAIL

- [ ] **Step 3: Implement Thermal Warming State**
  In [voice.rs](file:///Users/gus/Git/vibe303/plugin303/src/dsp/voice.rs):
  - Add `temperature: f64` state initialized to $20.0^\circ\text{C}$ (ambient).
  - Inside `tick()`, if `gate` is active, warm up towards $45.0^\circ\text{C}$ with a slow filter:
    `self.temperature += (45.0 - self.temperature) * (1.0 / (60.0 * self.sr as f64));` (60 second thermal constant).
    If inactive, cool down towards $20.0^\circ\text{C}$:
    `self.temperature += (20.0 - self.temperature) * (1.0 / (120.0 * self.sr as f64));`.
  - Pass the current temperature dynamically to the VCO and VCF solver state (which updates $V_T$ and shifts the physical curves).
  - Use `comp_drift` to scale the slow LFO 1/f pink noise CV pitch/cutoff modulation.

- [ ] **Step 4: Run test to verify it passes**
  Run: `cargo test -p plugin303 --test age_drift`
  Expected: PASS

- [ ] **Step 5: Commit**
  Run: `git commit -am "feat: integrate thermal warming curve and LFO drift modulation"`

---

### Task 5: Thermal Noise & VCO Reset Jitter

**Files:**
- Modify: [vco.rs](file:///Users/gus/Git/vibe303/plugin303/src/dsp/vco.rs)
- Modify: [ladder_wdf.rs](file:///Users/gus/Git/vibe303/plugin303/src/dsp/ladder_wdf.rs)
- Create: `plugin303/tests/noise_jitter.rs`

**Interfaces:**
- Consumes: `thermal_noise` parameter.
- Produces: Audio-rate white/pink noise injection and comparator threshold jitter.

- [ ] **Step 1: Write the failing test**
  Create `plugin303/tests/noise_jitter.rs` to verify that increasing `thermal_noise` introduces period duration jitter:
  ```rust
  #[test]
  fn test_vco_reset_jitter_is_non_zero() {
      let mut vco = Vco303::new(48000.0);
      vco.set_thermal(10); // Active thermal noise setting
      // Measure period differences between successive reset crossings
      let mut periods = Vec::new();
      // Tick and find zero crossings / reset points
      // Assert that std-dev of period differences is greater than 0
      assert!(std_dev(&periods) > 0.0);
  }
  ```

- [ ] **Step 2: Run test to verify it fails**
  Run: `cargo test -p plugin303 --test noise_jitter`
  Expected: FAIL (no test found, or jitter is zero)

- [ ] **Step 3: Implement Phase Jitter and Noise Injection**
  - In [vco.rs](file:///Users/gus/Git/vibe303/plugin303/src/dsp/vco.rs):
    - When `thermal_noise > 0`, generate white noise via LCG and add it to `CP_HIGH_THRESH` (e.g. scale the jitter range by `thermal_noise`).
    - Add noise to the target CV before interpolating.
  - In [ladder_wdf.rs](file:///Users/gus/Git/vibe303/plugin303/src/dsp/ladder_wdf.rs):
    - Inject noise currents directly to diode stage node values.

- [ ] **Step 4: Run test to verify it passes**
  Run: `cargo test -p plugin303 --test noise_jitter`
  Expected: PASS

- [ ] **Step 5: Commit**
  Run: `git commit -am "feat: implement VCO reset jitter and filter noise injection"`
