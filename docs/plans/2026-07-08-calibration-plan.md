# Envelope Curves, Accent & Glide Alignment — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the linear-ramp envelope generator and ad-hoc accent/glide logic in plugin303 with circuit-faithful RC-filter-based envelopes, a proper DecayEnvelope (MEG), accent mechanism, and exponential pitch slew — all derived from the jc303/Open303 reference. Scoped across all three fidelity tiers (Low/High/Ultra) and both topologies (Accurate/Fun).

**Architecture:** The 303's voicing character comes from three interacting systems: (1) a **DecayEnvelope** (pure exponential `y *= c`) as the main envelope generator (MEG) driving the VCF, shaped through two parallel RC (leaky-integrator) paths; (2) an **AnalogEnvelope** (RC-filtered AHDSR) for the VCA amplitude; (3) a **LeakyIntegrator** pitch slew limiter for glide. Accent modifies *both* envelopes' time constants and adds a VCA boost proportional to the MEG output. These three systems are tightly coupled in the per-sample loop.

**Tech Stack:** Rust (plugin303 crate, nih-plug), zero-alloc audio path, `f32` for audio / `f64` for coefficients.

## Global Constraints

- **Zero-alloc audio path** — no heap allocation in `process()`, `tick()`, or `reset()`.
- **Block-rate param reads** — envelope time constants updated once per block, not per sample.
- **All tiers and topologies** — the new RC-based envelopes, accent, and glide apply to Low, High, and Ultra tiers, and to both Accurate and Fun topologies. The "Advanced Env" toggle switches between the 303-faithful RC envelopes (off) and the user-tweakable ADSR (on).
- **Devil Fish extended ranges** — Decay knob range: 30–3000 ms (not the stock 200–2000 ms).
- **Measured hardware constants** — EnvMod uses the jc303 measured-curve scaling, not the simpler pitch-offset formula.
- **Dual accent trigger** — Sequencer flag (`step.accent`) AND live MIDI velocity ≥ 100 both trigger accent.
- **Existing test gates must still pass** — spectral, no-alloc, tier-switch, etc.

---

## Reference Architecture (from jc303/Open303)

### How the 303 envelopes actually work

**MEG (Main Envelope Generator = filter envelope):**
- Is a `DecayEnvelope` — a pure multiplicative-accumulator exponential decay: `y *= c` where `c = exp(-1 / (0.001 * tau * sr))`.
- On `trigger()`: `y = 1/c` (so the first `getSample()` outputs exactly `1.0`).
- Decay time constant switches between `normalDecay` (knob, 200–2000 ms) and `accentDecay` (fixed 200 ms on stock 303) depending on whether the note has accent.
- The raw MEG output is then passed through **two parallel RC paths** (leaky integrators `rc1` and `rc2`):
  - `rc1`: time constant = `normalAttack` (3 ms stock), smooths the MEG for the **envMod** contribution.
  - `rc2`: time constant = `accentAttack` (15 ms stock), smooths the MEG for the **accent** contribution. Only active when `accentGain > 0`.
- Cutoff modulation: `instCutoff = cutoff * 2^(envScaler*(rc1_out - envOffset) + accentGain*rc2_out)` — exponential (octave) modulation, not additive Hz.

**VCA (Amplitude Envelope):**
- Is an `AnalogEnvelope` — a proper RC-filtered AHDSR with exponential curves.
- Per-sample tick: `out = prev + coeff * (target - prev)` (one-pole lowpass towards the stage's target level).
- Coefficients: `coeff = 1 - exp(-1 / (tau))` where `tau = sr * 0.001 * time_ms * tauScale`.
- Stock 303 settings: attack=0 ms, decay=1230 ms, sustain=0, release=0.5 ms.
- On accent: release lengthens to `accentAmpRelease` (50 ms).
- The VCA output is further boosted by the MEG: `ampOut += 0.45*megOut + accentGain*4.0*megOut` (while note is on).

**Glide (Pitch Slew):**
- Is a `LeakyIntegrator` with time constant = `0.2 * slideTime` (stock slideTime = 60 ms → tau = 12 ms).
- Per-sample: `y = in + coeff * (y_prev - in)` where `coeff = exp(-1 / (0.001 * tau * sr))`.
- On `triggerNote()`: slew state is **hard-set** to the new frequency (no slide). On `slideToNote()`: target frequency changes but state is NOT reset (smooth slide).

---

## What's Wrong With Our Current Implementation

| Aspect | Current (vibe303) | Reference (jc303) | Impact |
|--------|-------------------|---------------------|--------|
| **MEG shape** | Linear ramp (`level -= dec_inc`) | Exponential decay (`y *= c`) | Wrong curve — linear sounds digital, not analog |
| **VCA shape** | Linear ramp | RC-filtered exponential (one-pole IIR) | Same — linear ramps don't sound like analog envelopes |
| **MEG → cutoff** | Additive Hz (`+ meg * envMod * 4000`) | Exponential/octave (`cutoff * 2^(...)`) via envScaler/envOffset | Completely wrong scaling — additive Hz means envMod doesn't track across the keyboard |
| **Accent on MEG** | Shortens decay via accumulator blend | Switches decay to fixed short value + separate RC path (rc2) boosting cutoff | Missing the dual-RC architecture; accent character is wrong |
| **Accent on VCA** | `* (1 + accent * acc * 0.5)` | `ampOut += 0.45*megOut + accentGain*4.0*megOut` | Missing MEG-driven VCA boost (key to the 303 "squelch") |
| **VCA timing** | Same as MEG (shared decay knob) | Independent: decay=1230ms, release=0.5ms (non-accent) / 50ms (accent) | VCA should be much longer decay than filter, with very short release |
| **Glide** | `1 - exp(-1/(ms*sr))` one-pole (correct shape) but `10 + knob*75` ms range | `LeakyIntegrator` with `0.2 * slideTime` where slideTime=60ms → 12ms tau | Range is roughly OK but missing the hard-set on trigger vs smooth on slide |
| **envMod scaling** | Linear 0..1 | Measured curve: `envScaler` and `envOffset` computed from cutoff×envMod interaction | The interaction between cutoff and envMod is not captured |

---

> [!WARNING]
> **Breaking change:** The VCA envelope timing will change significantly (from `200+knob*2800` ms decay to a fixed 1230 ms decay with 0.5 ms release). The accent VCA boost will also change character. This is intentional — it aligns with the hardware — but existing presets/patterns will sound different.

---

## Proposed Changes

### Component 1: RC-Based Envelope Generators

New file `plugin303/src/dsp/rc_env.rs` containing three zero-alloc structs that replace the current linear `AdsrEnv` for the classic 303 path.

---

#### [NEW] [rc_env.rs](file:///Users/gus/Git/vibe303/plugin303/src/dsp/rc_env.rs)

Three structs:

1. **`DecayEnv`** — Pure exponential decay (MEG). Replaces the filter envelope.
   - State: `y: f64`, `c: f64`, `y_init: f64`, `tau_ms: f64`, `sr: f64`
   - `trigger()`: `y = y_init` (= `1/c`, so first `getSample()` returns 1.0)
   - `tick() -> f64`: `y *= c; y`
   - `set_decay(tau_ms)`: recalculates `c = exp(-1 / (0.001 * tau * sr))`, `y_init = 1/c`

2. **`RcEnv`** — RC-filtered AHDSR envelope (VCA). Replaces the amplitude envelope.
   - State: `prev: f64`, `time_ms: f64`, `attack_coeff: f64`, `decay_coeff: f64`, `release_coeff: f64`, `note_on: bool`, plus level/time params.
   - Coefficient formula: `coeff = 1 - exp(-1 / (sr * 0.001 * time_ms * tau_scale))`
   - `tick() -> f64`: RC filter towards target level based on current stage (attack→peak, decay→sustain, release→0).
   - Stage tracking via accumulated time (not discrete state enum).

3. **`LeakyIntegrator`** — One-pole lowpass (for rc1/rc2 smoothing and pitch slew).
   - State: `y1: f64`, `coeff: f64`
   - `set_time_constant(tau_ms, sr)`: `coeff = exp(-1 / (0.001 * tau * sr))`
   - `tick(input) -> f64`: `y1 = input + coeff * (y1 - input); y1`
   - `set_state(val)`: hard-set `y1 = val` (for pitch slew on trigger)

---

#### [MODIFY] [mod.rs](file:///Users/gus/Git/vibe303/plugin303/src/dsp/mod.rs)

Add `pub mod rc_env;` to the DSP module list.

---

### Component 2: Voice Envelope & Modulation Rewiring

#### [MODIFY] [voice.rs](file:///Users/gus/Git/vibe303/plugin303/src/dsp/voice.rs)

**New fields** (replacing `meg: AdsrEnv`, `veg: AdsrEnv`, `accent_acc: f32`):

```rust
// --- Classic 303 Envelope Chain ---
/// Main Envelope Generator (pure exponential decay) — drives VCF cutoff
meg_decay: DecayEnv,
/// RC smoother for envMod path (tau = normalAttack, default 3ms)
rc1: LeakyIntegrator,
/// RC smoother for accent path (tau = accentAttack, default 15ms)  
rc2: LeakyIntegrator,
/// Amplitude envelope (RC-filtered AHDSR)
vca_env: RcEnv,
/// Pitch slew limiter (glide)
pitch_slew: LeakyIntegrator,
/// Amplitude de-click lowpass (optional, tau ~ 2ms)
amp_declicker: LeakyIntegrator,

// Accent state
accent_gain: f32,         // 0.0 or accent_knob value (set per-note)
normal_decay_ms: f64,     // decay knob → 200..2000 ms
accent_decay_ms: f64,     // fixed 200 ms (stock 303)

// EnvMod scaling (measured curve from hardware)
env_scaler: f64,
env_offset: f64,
```

**Keep** the existing `meg: AdsrEnv` and `veg: AdsrEnv` for the "Advanced Env" paths only. When `advanced_env == false`, all topologies (Accurate and Fun) use the new RC-based envelopes. The `tick()` method branches on `advanced_env`.

**`update_params()` changes:**

In the `!advanced_env` branch (both Accurate and Fun topologies):
```rust
// MEG decay from knob: 30..3000 ms (Devil Fish extended range)
self.normal_decay_ms = 30.0 + decay_knob as f64 * 2970.0;
// Accent decay: 30..3000 ms (Devil Fish)
self.accent_decay_ms = 30.0 + accent_decay_knob as f64 * 2970.0;

// VCA envelope: fixed 303 timings
self.vca_env.set_attack(0.0);     // instant attack
self.vca_env.set_decay(1230.0);   // ~1.2s decay
self.vca_env.set_sustain(0.0);    // no sustain
self.vca_env.set_release(0.5);    // 0.5 ms release (non-accent)

// RC smoothers
self.rc1.set_time_constant(3.0, sr);   // normalAttack = 3ms
self.rc2.set_time_constant(15.0, sr);  // accentAttack = 15ms

// envMod scaling (measured hardware curve)
self.calculate_env_mod_scaler(cutoff_hz, env_mod);
```

**`set_gate()` / `note_on()` changes:**

```rust
// Accent is triggered by sequencer flag OR MIDI velocity >= 100
let has_accent = accent || velocity >= 100;

// On trigger (new note, not slide):
if has_accent {
    self.accent_gain = self.accent_knob_val;
    self.meg_decay.set_decay(self.accent_decay_ms);
    self.vca_env.set_release(50.0);  // accent release = 50ms
} else {
    self.accent_gain = 0.0;
    self.meg_decay.set_decay(self.normal_decay_ms);
    self.vca_env.set_release(0.5);   // normal release = 0.5ms
}
self.meg_decay.trigger();
self.vca_env.note_on();

// On slide (legato):
// Do NOT retrigger MEG or VCA. Just update accent state and target pitch.
// Pitch slew state is NOT reset (smooth glide continues).

// On trigger (non-slide):
// Hard-set pitch slew to new frequency (no glide)
self.pitch_slew.set_state(new_freq_hz);
```

**`tick()` — filter cutoff modulation rewrite (Accurate path):**

```rust
// MEG output (pure exponential decay)
let meg_out = self.meg_decay.tick();

// RC1: smooth the MEG for envMod contribution
let rc1_out = self.rc1.tick(meg_out);

// RC2: smooth the MEG for accent contribution (only when accent is active)
let rc2_input = if self.accent_gain > 0.0 { meg_out } else { 0.0 };
let rc2_out = self.rc2.tick(rc2_input);

// Compute modulated cutoff (exponential/octave domain)
// envMod term uses measured hardware constants (jc303 calculateEnvModScalerAndOffset)
let env_mod_term = self.env_scaler * (rc1_out - self.env_offset);
let accent_term = self.accent_gain as f64 * rc2_out;
let inst_cutoff = base_cutoff_hz as f64 * 2.0_f64.powf(env_mod_term + accent_term);
```

**`tick()` — VCA modulation rewrite (Accurate path):**

```rust
let mut amp_out = self.vca_env.tick();
// MEG-driven VCA boost (key to the 303 squelch sound)
if self.vca_env.is_note_on() {
    amp_out += 0.45 * meg_out + self.accent_gain as f64 * 4.0 * meg_out;
}
// De-click lowpass
amp_out = self.amp_declicker.tick(amp_out);

// ... multiply audio by amp_out ...
```

**`tick()` — pitch glide rewrite:**

```rust
// Exponential pitch slew (LeakyIntegrator, not linear alpha blend)
let target_freq = midi_to_freq_from_st(self.target_pitch_st);
let inst_freq = self.pitch_slew.tick(target_freq);
// Use inst_freq directly for oscillator frequency
```

---

#### [MODIFY] [env.rs](file:///Users/gus/Git/vibe303/plugin303/src/dsp/env.rs)

Keep `AdsrEnv` as-is — it's still used by the "Advanced Env" toggle path. No changes needed.

---

### Component 3: EnvMod Scaling (Measured Hardware Curve)

#### [MODIFY] [voice.rs](file:///Users/gus/Git/vibe303/plugin303/src/dsp/voice.rs)

Add a private method implementing the jc303 measured-curve formula:

```rust
fn calculate_env_mod_scaler(&mut self, cutoff_hz: f64, env_mod_normalized: f64) {
    const C0: f64 = 3.138152786059267e+002;   // lowest nominal cutoff
    const C1: f64 = 2.394411986817546e+003;   // highest nominal cutoff
    const OF: f64 = 0.048292930943553;        // offset factor
    const OC: f64 = 0.294391201442418;        // offset constant
    const SLO_F: f64 = 3.773996325111173;     // scaler factor at low cutoff
    const SLO_C: f64 = 0.736965594166206;     // scaler constant at low cutoff
    const SHI_F: f64 = 4.194548788411135;     // scaler factor at high cutoff
    const SHI_C: f64 = 0.864344900642434;     // scaler constant at high cutoff

    let e = env_mod_normalized;  // already 0..1
    let c = exp_to_lin(cutoff_hz, C0, C1, 0.0, 1.0);
    let s_lo = SLO_F * e + SLO_C;
    let s_hi = SHI_F * e + SHI_C;
    self.env_scaler = (1.0 - c) * s_lo + c * s_hi;
    self.env_offset = OF * c + OC;
}
```

The cutoff mapping in the classic path should also use the jc303 range: `linToExp(knob, 0..1, 314..2394)` instead of our current `200 * 20^knob`.

---

### Component 4: Glide/Slide Behavior

The glide changes are incorporated into Component 2 (voice.rs). Key behavioral points:

- **Trigger (no slide):** `pitch_slew.set_state(new_freq)` — hard-sets the integrator, producing no slide.
- **Slide:** Only sets `target_freq` — the integrator smoothly approaches it.
- **Slide time:** `pitch_slew.set_time_constant(0.2 * slide_time_ms, sr)` where `slide_time_ms` defaults to 60.0.
- The slew operates in **frequency domain** (Hz), not pitch domain (semitones). This matches the hardware which operates on voltage→frequency.

---

### Component 5: Tests

#### [NEW] [tests/env_curve.rs](file:///Users/gus/Git/vibe303/plugin303/tests/env_curve.rs)

Integration tests verifying the new envelope shapes:

1. **MEG exponential shape test**: Trigger a `DecayEnv` with tau=200ms, verify output at t=tau is ~0.368 (1/e), and at t=5*tau is < 0.01.
2. **VCA RC-curve test**: Trigger an `RcEnv`, verify exponential approach to peak (not linear ramp). Verify release drops to near-zero within ~5×release_time.
3. **Accent shortens MEG decay**: Trigger with accent (tau=200ms) vs normal (tau=1000ms), verify the accented envelope decays faster.
4. **Accent boosts VCA**: Verify `ampOut += 0.45*meg + accentGain*4.0*meg` produces a measurable level boost during accent.
5. **Glide test**: Trigger a note, then slide to a new note. Verify the frequency transitions smoothly (exponential approach). Trigger a non-slide note, verify the frequency jumps instantly.
6. **envMod octave scaling**: Verify that with envMod=100% and cutoff=1000Hz, the peak instantaneous cutoff is significantly higher than 1000Hz (octave-domain modulation). Verify with envMod=0%, the cutoff stays at the nominal value.

#### [MODIFY] [tests/voice_esr.rs](file:///Users/gus/Git/vibe303/plugin303/tests/voice_esr.rs)

Update ESR budget if needed — the new envelope shapes may shift the spectral character slightly. The budget may need loosening or the gold files may need regeneration.

---

## Verification Plan

### Automated Tests

```bash
# Run all plugin tests (includes new env_curve tests + existing gates)
cargo test -p plugin303

# Run only the new envelope tests
cargo test -p plugin303 --test env_curve

# Verify no-alloc gate still passes
cargo test -p plugin303 --test no_alloc

# Verify spectral gates still pass
cargo test -p plugin303 --test ultra_spectral
cargo test -p plugin303 --test high_spectral
```

### Manual Verification

- Build the plugin (`cargo xtask bundle plugin303 --release`)
- Load in DAW with sequencer running an acid pattern (notes + accents + slides)
- Verify: accented notes have a brighter, punchier cutoff sweep with faster decay
- Verify: non-accented notes have a slower, smoother filter sweep
- Verify: slide notes glide smoothly between pitches
- Verify: the VCA envelope has a natural exponential decay, not a linear ramp
- Verify: no clicks on note transitions (amp de-clicker)
- Compare A/B with the jc303 VST if available
