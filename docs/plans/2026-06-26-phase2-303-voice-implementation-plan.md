# Phase 2 — TB-303 nih-plug Voice Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build `plugin303`, a monophonic nih-plug VST3/CLAP TB-303 synth voice, grounded
against the Phase-1 ngspice circuit models, souped up with a 2nd osc, sub, saw→square
sweep, 2/3/4-pole filter tap + self-oscillation, and a 16-step sequencer.

**Architecture:** New self-contained workspace crate. Monophonic voice: VCO block →
TPT diode-ladder VCF → VCA, driven by a 16-step sequencer + live MIDI, modulated by
MEG/VEG envelopes and an accent-sweep circuit. Fixed 2×/4×/8× oversampling on nonlinear
blocks. Built vertical-slice-first (Strategy A): M1 makes sound end-to-end, M2–M8 add
fidelity and features.

**Tech Stack:** Rust, nih-plug (rev `f36931f7af4646065488a9845d8f8c2f95252c23`,
features `vst3`,`standalone`), nih_plug_egui, assert_no_alloc. Validation against
`eval/303/*.py` ngspice harnesses.

**Design doc:** `docs/plans/2026-06-26-phase2-303-voice-design.md`
**Repo:** `~/Git/vibe303` (standalone 303 repo; ba283 preamp product removed). Work on a
fresh branch off `main`.

> **Note (repo move):** authored against the ba283-vst3 worktree, then the 303 work was
> relocated to its own repo `vibe303` and the ba283 preamp product removed. References
> below to "ba283 `plugin/`" mean the **preserved shared DSP/scaffold** kept during that
> removal (oversample, WDF engine, nih-plug Plugin/standalone idioms, assert_no_alloc).

---

## Conventions for the implementing engineer

- **TDD always:** write the failing test, run it red, write minimal code, run it green, commit.
- **No-alloc audio thread:** nothing in `process()` / the per-sample path may allocate or
  lock. The `assert_no_alloc` test is the gate (see ba283 `plugin/` for the pattern).
- **DSP math is unit-tested headless** (no host). Pure functions/structs with `tick()` —
  test them directly; the plugin shell just wires them.
- **Commit after every green step.** Small commits.
- **Run from the repo root** `~/Git/vibe303` unless noted.
- **Reference impl:** the ba283 crate (`plugin/`) shows the nih-plug Plugin impl, params,
  editor, oversample, and the assert_no_alloc test — copy idioms, don't copy guts.

---

# MILESTONE M1 — Walking skeleton (audible end-to-end)

**Outcome:** MIDI note in → naive-antialiased saw → 4-pole TPT ladder → AD VCA → audio out,
loadable as VST3 + runnable as standalone. assert_no_alloc green. This de-risks the whole
nih-plug/workspace integration before any fidelity work.

---

### Task M1.1: Workspace crate scaffold that compiles

**Files:**
- Modify: `Cargo.toml` (workspace members)
- Create: `plugin303/Cargo.toml`
- Create: `plugin303/src/lib.rs`
- Modify: `bundler.toml`

**Step 1: Add the crate to the workspace.**
Edit `Cargo.toml` members:
```toml
[workspace]
members = ["plugin", "xtask", "plugin303"]
resolver = "2"
```

**Step 2: Create `plugin303/Cargo.toml`.**
```toml
[package]
name = "plugin303"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
nih_plug = { git = "https://github.com/robbert-vdh/nih-plug.git", rev = "f36931f7af4646065488a9845d8f8c2f95252c23", features = ["vst3", "standalone"] }
nih_plug_egui = { git = "https://github.com/robbert-vdh/nih-plug.git", rev = "f36931f7af4646065488a9845d8f8c2f95252c23" }

[dev-dependencies]
assert_no_alloc = { version = "1.1", default-features = false, features = ["warn_debug", "warn_release"] }
```

**Step 3: Create a minimal `plugin303/src/lib.rs`** — a synth that exports but outputs
silence (fills the skeleton). Use an output-only IO layout + `MidiConfig::Basic`.
```rust
use nih_plug::prelude::*;
use std::sync::Arc;

mod params;
use params::Plugin303Params;

pub struct Plugin303 {
    params: Arc<Plugin303Params>,
}

impl Default for Plugin303 {
    fn default() -> Self {
        Self { params: Arc::new(Plugin303Params::default()) }
    }
}

impl Plugin for Plugin303 {
    const NAME: &'static str = "TB303";
    const VENDOR: &'static str = "desmen";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> { self.params.clone() }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        for channel in buffer.as_slice() {
            for s in channel.iter_mut() { *s = 0.0; }
        }
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for Plugin303 {
    const VST3_CLASS_ID: [u8; 16] = *b"desmen-tb303-vs3";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}
impl ClapPlugin for Plugin303 {
    const CLAP_ID: &'static str = "com.desmen.tb303";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Souped-up TB-303 voice");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] =
        &[ClapFeature::Instrument, ClapFeature::Synthesizer];
}
nih_export_clap!(Plugin303);
nih_export_vst3!(Plugin303);
```

**Step 4: Create a minimal `plugin303/src/params.rs`.**
```rust
use nih_plug::prelude::*;

#[derive(Params)]
pub struct Plugin303Params {}

impl Default for Plugin303Params {
    fn default() -> Self { Self {} }
}
```

**Step 5: Add the bundler entry.** Append to `bundler.toml`:
```toml
[plugin303]
name = "TB303"
```

**Step 6: Verify it builds.**
Run: `cargo build -p plugin303`
Expected: compiles clean.

**Step 7: Commit.**
```bash
git add Cargo.toml plugin303/ bundler.toml Cargo.lock
git commit -m "feat(303): plugin303 crate scaffold (nih-plug synth, silent)"
```

---

### Task M1.2: polyBLEP saw oscillator (TDD)

A minimal antialiased saw — refined to circuit-faithful in M3, but good enough that M1
doesn't scream aliasing.

**Files:**
- Create: `plugin303/src/dsp/mod.rs`
- Create: `plugin303/src/dsp/osc.rs`
- Modify: `plugin303/src/lib.rs` (add `mod dsp;`)

**Step 1: Write the failing test** in `plugin303/src/dsp/osc.rs`:
```rust
pub struct Saw { phase: f32, inc: f32 }

impl Saw {
    pub fn new() -> Self { Self { phase: 0.0, inc: 0.0 } }
    pub fn set_freq(&mut self, hz: f32, sr: f32) { self.inc = hz / sr; }
    pub fn tick(&mut self) -> f32 { todo!() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saw_mean_near_zero_over_one_period() {
        let sr = 48_000.0;
        let mut s = Saw::new();
        s.set_freq(480.0, sr); // 100 samples/period
        let n = 4_800;
        let sum: f32 = (0..n).map(|_| s.tick()).sum();
        assert!((sum / n as f32).abs() < 0.05, "DC = {}", sum / n as f32);
    }

    #[test]
    fn saw_stays_in_range() {
        let sr = 48_000.0;
        let mut s = Saw::new();
        s.set_freq(440.0, sr);
        for _ in 0..10_000 {
            let v = s.tick();
            assert!(v.abs() <= 1.5, "out of range: {v}");
        }
    }
}
```

**Step 2: Run red.** `cargo test -p plugin303 osc::` → FAIL (`todo!`).

**Step 3: Implement polyBLEP saw:**
```rust
impl Saw {
    pub fn tick(&mut self) -> f32 {
        let mut v = 2.0 * self.phase - 1.0;          // naive saw
        v -= poly_blep(self.phase, self.inc);         // correct the discontinuity
        self.phase += self.inc;
        if self.phase >= 1.0 { self.phase -= 1.0; }
        v
    }
}

fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt { let t = t / dt; t + t - t * t - 1.0 }
    else if t > 1.0 - dt { let t = (t - 1.0) / dt; t * t + t + t + 1.0 }
    else { 0.0 }
}
```
Create `plugin303/src/dsp/mod.rs` with `pub mod osc;` and add `mod dsp;` to `lib.rs`.

**Step 4: Run green.** `cargo test -p plugin303 osc::` → PASS.

**Step 5: Commit.**
```bash
git add plugin303/src/dsp/ plugin303/src/lib.rs
git commit -m "feat(303): polyBLEP saw oscillator (M1 stub, fidelity in M3)"
```

---

### Task M1.3: TPT 4-pole ladder lowpass (TDD)

Linear 4-pole TPT first (resonance/tanh/self-osc come in M2). Sin-coefficient form per the
design's Cytomic reference.

**Files:**
- Create: `plugin303/src/dsp/ladder.rs`
- Modify: `plugin303/src/dsp/mod.rs` (`pub mod ladder;`)

**Step 1: Write the failing test** — a 4-pole LP must pass DC and kill HF:
```rust
pub struct TptLadder { /* 4 one-pole states + coeffs */ }

impl TptLadder {
    pub fn new(sr: f32) -> Self { todo!() }
    pub fn set_cutoff(&mut self, hz: f32) { todo!() }
    pub fn tick(&mut self, x: f32) -> f32 { todo!() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(f: &mut TptLadder, freq: f32, sr: f32) -> f32 {
        let mut acc = 0.0f32; let n = 20_000;
        let mut ph = 0.0f32;
        for _ in 0..n {
            let x = (2.0 * std::f32::consts::PI * ph).sin();
            let y = f.tick(x);
            acc += y * y;
            ph += freq / sr; if ph >= 1.0 { ph -= 1.0; }
        }
        (acc / n as f32).sqrt()
    }

    #[test]
    fn passes_dc_attenuates_hf() {
        let sr = 48_000.0;
        let mut f = TptLadder::new(sr);
        f.set_cutoff(1_000.0);
        let lo = rms(&mut f, 100.0, sr);   // well below cutoff
        let mut f2 = TptLadder::new(sr);
        f2.set_cutoff(1_000.0);
        let hi = rms(&mut f2, 8_000.0, sr); // 3 octaves above
        assert!(lo > 0.6, "passband too low: {lo}");
        assert!(hi < 0.05, "stopband too high: {hi}");
    }
}
```

**Step 2: Run red.** `cargo test -p plugin303 ladder::` → FAIL.

**Step 3: Implement** four cascaded 1-pole TPT stages (sin-coeff `g`), no resonance yet:
```rust
pub struct TptLadder { g: f32, z: [f32; 4], sr: f32 }

impl TptLadder {
    pub fn new(sr: f32) -> Self { let mut s = Self { g: 0.0, z: [0.0;4], sr }; s.set_cutoff(1_000.0); s }
    pub fn set_cutoff(&mut self, hz: f32) {
        let wd = std::f32::consts::PI * (hz.clamp(20.0, self.sr*0.45)) / self.sr;
        self.g = wd.tan(); // per-stage 1-pole TPT coefficient
    }
    pub fn tick(&mut self, x: f32) -> f32 {
        let mut v = x;
        for z in self.z.iter_mut() {
            let hp = (v - *z) / (1.0 + self.g);
            let lp = hp * self.g + *z;
            *z = lp + hp * self.g;
            v = lp;
        }
        v
    }
}
```
(Coefficient form can be swapped to the exact sin-form `g0/g1/g2` in M2 when resonance is
added; the linear cascade above is enough to pass M1.3.)

**Step 4: Run green.** `cargo test -p plugin303 ladder::` → PASS.

**Step 5: Commit.**
```bash
git add plugin303/src/dsp/ladder.rs plugin303/src/dsp/mod.rs
git commit -m "feat(303): 4-pole TPT ladder lowpass (linear, M1)"
```

---

### Task M1.4: Fast-attack AD envelope (TDD)

**Files:**
- Create: `plugin303/src/dsp/env.rs`
- Modify: `plugin303/src/dsp/mod.rs` (`pub mod env;`)

**Step 1: Failing test** — gate-on rises fast to ~1, gate-off decays toward 0:
```rust
pub struct AdEnv { /* state */ }
impl AdEnv {
    pub fn new(sr: f32) -> Self { todo!() }
    pub fn set_decay(&mut self, secs: f32) { todo!() }
    pub fn gate(&mut self, on: bool) { todo!() }
    pub fn tick(&mut self) -> f32 { todo!() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn attacks_fast_then_decays() {
        let sr = 48_000.0;
        let mut e = AdEnv::new(sr);
        e.set_decay(0.2);
        e.gate(true);
        for _ in 0..(sr as usize / 1000) { e.tick(); } // 1 ms
        let after_attack = e.tick();
        assert!(after_attack > 0.9, "attack too slow: {after_attack}");
        e.gate(false);
        for _ in 0..(sr as usize / 2) { e.tick(); } // 0.5 s
        let after_decay = e.tick();
        assert!(after_decay < 0.2, "decay too slow: {after_decay}");
    }
}
```

**Step 2: Run red.** `cargo test -p plugin303 env::` → FAIL.

**Step 3: Implement** a one-pole AD (fast fixed attack coefficient, decay from param):
```rust
pub struct AdEnv { level: f32, atk: f32, dec: f32, gate_on: bool }
impl AdEnv {
    pub fn new(sr: f32) -> Self {
        Self { level: 0.0, atk: 1.0 - (-1.0 / (0.0005 * sr)).exp(), dec: 0.0, gate_on: false }
            .with_decay(0.2, sr)
    }
    fn with_decay(mut self, secs: f32, sr: f32) -> Self { self.set_decay_sr(secs, sr); self }
    fn set_decay_sr(&mut self, secs: f32, sr: f32) { self.dec = 1.0 - (-1.0 / (secs.max(1e-4) * sr)).exp(); }
    // store sr if you prefer; shown inline for clarity
    pub fn gate(&mut self, on: bool) { self.gate_on = on; }
    pub fn tick(&mut self) -> f32 {
        let target = if self.gate_on { 1.0 } else { 0.0 };
        let coeff = if self.gate_on { self.atk } else { self.dec };
        self.level += coeff * (target - self.level);
        self.level
    }
}
```
(Adjust `set_decay` to keep `sr`; the test only needs the behavior above.)

**Step 4: Run green.** `cargo test -p plugin303 env::` → PASS.

**Step 5: Commit.**
```bash
git add plugin303/src/dsp/env.rs plugin303/src/dsp/mod.rs
git commit -m "feat(303): fast-attack AD envelope (M1)"
```

---

### Task M1.5: Voice — wire osc → ladder → VCA, MIDI pitch (TDD)

**Files:**
- Create: `plugin303/src/voice.rs`
- Modify: `plugin303/src/lib.rs` (`mod voice;`)

**Step 1: Failing test** — note-on produces non-silent output; note-off → decays to silence:
```rust
pub struct Voice { /* osc, ladder, veg, sr */ }
impl Voice {
    pub fn new(sr: f32) -> Self { todo!() }
    pub fn note_on(&mut self, midi: u8) { todo!() }
    pub fn note_off(&mut self) { todo!() }
    pub fn render(&mut self) -> f32 { todo!() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn note_on_makes_sound_note_off_silences() {
        let sr = 48_000.0;
        let mut v = Voice::new(sr);
        v.note_on(57); // A3
        let mut energy = 0.0f32;
        for _ in 0..4_800 { let s = v.render(); energy += s*s; }
        assert!(energy > 0.01, "no sound after note_on: {energy}");
        v.note_off();
        for _ in 0..48_000 { v.render(); } // 1 s decay
        let mut tail = 0.0f32;
        for _ in 0..4_800 { let s = v.render(); tail += s*s; }
        assert!(tail < 1e-4, "did not silence: {tail}");
    }
}
```

**Step 2: Run red.** `cargo test -p plugin303 voice::` → FAIL.

**Step 3: Implement** the wiring (midi→Hz via `440 * 2^((n-69)/12)`):
```rust
use crate::dsp::{osc::Saw, ladder::TptLadder, env::AdEnv};

pub struct Voice { osc: Saw, vcf: TptLadder, veg: AdEnv, sr: f32 }
impl Voice {
    pub fn new(sr: f32) -> Self {
        let mut vcf = TptLadder::new(sr); vcf.set_cutoff(2_000.0);
        Self { osc: Saw::new(), vcf, veg: AdEnv::new(sr), sr }
    }
    pub fn note_on(&mut self, midi: u8) {
        let hz = 440.0 * 2.0_f32.powf((midi as f32 - 69.0) / 12.0);
        self.osc.set_freq(hz, self.sr);
        self.veg.gate(true);
    }
    pub fn note_off(&mut self) { self.veg.gate(false); }
    pub fn render(&mut self) -> f32 {
        let saw = self.osc.tick();
        let filtered = self.vcf.tick(saw);
        filtered * self.veg.tick()
    }
}
```

**Step 4: Run green.** `cargo test -p plugin303 voice::` → PASS.

**Step 5: Commit.**
```bash
git add plugin303/src/voice.rs plugin303/src/lib.rs
git commit -m "feat(303): Voice wiring (osc->ladder->VCA, MIDI pitch)"
```

---

### Task M1.6: Plugin `process()` — render the voice, handle MIDI

**Files:**
- Modify: `plugin303/src/lib.rs`

**Step 1: Add a `Voice` to the plugin and store sample rate in `initialize`.**
```rust
fn initialize(&mut self, _l: &AudioIOLayout, cfg: &BufferConfig, _c: &mut impl InitContext<Self>) -> bool {
    self.voice = Voice::new(cfg.sample_rate);
    true
}
```
(Make `voice: Voice` a field; init it in `Default` with a placeholder 48k that `initialize`
overwrites.)

**Step 2: Replace `process()` to consume NoteEvents and render mono→both channels:**
```rust
fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers,
           context: &mut impl ProcessContext<Self>) -> ProcessStatus {
    let mut next = context.next_event();
    for (i, frame) in buffer.iter_samples().enumerate() {
        while let Some(ev) = next {
            if ev.timing() != i as u32 { break; }
            match ev {
                NoteEvent::NoteOn { note, .. } => self.voice.note_on(note),
                NoteEvent::NoteOff { .. }      => self.voice.note_off(),
                _ => {}
            }
            next = context.next_event();
        }
        let s = self.voice.render() * 0.3;
        for sample in frame { *sample = s; }
    }
    ProcessStatus::Normal
}
```

**Step 3: Build.** `cargo build -p plugin303` → compiles.

**Step 4: Manual smoke test (standalone).**
Run: `cargo run -p plugin303 --bin plugin303` *(if no standalone bin exists, add one — see
ba283 `plugin/src/bin/standalone.rs` for the `nih_export_standalone` pattern, or bundle
and load the VST3.)*
Expected: playing MIDI produces a filtered saw tone.

**Step 5: Commit.**
```bash
git add plugin303/src/lib.rs
git commit -m "feat(303): process() renders voice from MIDI (M1 audible)"
```

---

### Task M1.7: assert_no_alloc gate (TDD)

**Files:**
- Create: `plugin303/tests/no_alloc.rs`

**Step 1: Write the test** that drives `Voice::render` under the alloc guard:
```rust
use assert_no_alloc::*;
#[global_allocator]
static A: AllocDisabler = AllocDisabler;

#[test]
fn render_does_not_allocate() {
    let mut v = plugin303::test_voice(48_000.0); // expose a ctor for tests
    v_note_on(&mut v);
    assert_no_alloc(|| { for _ in 0..48_000 { let _ = v.render(); } });
}
```
(Expose what the test needs via `pub` or a small `#[doc(hidden)] pub fn` in the crate;
mirror how ba283 exposes its render path to `tests/`.)

**Step 2: Run red** (compile error / missing export) → fix exports → run.

**Step 3: Run green.** `cargo test -p plugin303 --test no_alloc` → PASS (0 allocations).

**Step 4: Commit.**
```bash
git add plugin303/tests/no_alloc.rs plugin303/src/lib.rs
git commit -m "test(303): assert_no_alloc gate on voice render"
```

**M1 DONE** when: all M1 tests green, `cargo build -p plugin303` clean, and a MIDI note
produces a filtered saw in standalone/host.

---

# MILESTONES M2–M8 — Task outlines

> Expand each into bite-sized TDD tasks (same shape as M1) when you reach it — much of the
> detail depends on M1's concrete types. Each milestone ends with: builds, tests green,
> measured/audible.

## M2 — Filter fidelity
- Swap `TptLadder` to the exact sin-coefficient form (`g0/g1/g2`, `k=1/Q`) from the design's
  Cytomic reference; add resonance `k` and per-stage `tanh` saturation.
- Add `set_reso`, `set_poles(2|3|4)` (output-tap remix of the 4 stage states).
- Self-oscillation: drive `k→0`; verify bounded by `tanh`.
- **Fit harness:** compare `TptLadder` AC magnitude vs `eval/303/run_vcf303_spice.py` gold
  (cutoff tracking, slope, resonance peak fc/height, tap slopes). Check in a golden JSON
  fixture; test asserts bounded error. (Reuse the ngspice harness already in the repo.)
- Tests: passband/stopband per tap; resonance peak rises with `k`; self-osc sustains; fit error bound.

## M3 — Oscillator fidelity (faithful Saw + Square)
- Define `OscCore` trait. Fit Saw + Square shapes to `circuit/vco303.toml` ngspice output +
  Din Sync C-note references (non-canonical waves — reproduce real edges, not ideal ramps).
- Waveform **switch** param {Saw, Square}; Square PWM duty V/oct-tracked 45→71%.
- Tests: spectra/waveshape snapshot vs vco303 gold within tolerance; duty tracking across pitch.

## M4 — Envelopes + accent + glide
- MEG (filter env) → cutoff via env-mod amount; VEG → VCA. Both fast-attack AD.
- Accent-sweep slew-limiter (leaky integrator; stacks on consecutive accents) → +cutoff,
  −MEG decay, +VCA.
- Glide/slide one-pole pitch slew, armed only on slide.
- Tests: env-mod moves cutoff; accent stacking curve; glide slew time; slide-only arming.

## M5 — Sequencer + MIDI override
- 16-step engine; per-step `{semitone, octave, gate: Note|Rest|Tie, accent, slide,
  gate_len, velocity}`; host-transport clock → 16th boundaries (read ppq/tempo).
- Gate logic: tie/slide → extended gate; slide → glide arm. MIDI note-on overrides while held.
- Pattern persisted via a custom nih-plug `Params` persist field (fixed 16-slot array).
- Internal free-run clock fallback when no host transport (standalone).
- Tests: step advance on 16th; tie/slide/extended gate; accent lane; MIDI override; persistence round-trip.

## M6 — Souped levers
- osc2 (detune cents, mix), sub (octave-down square), saw→square **Sweep** mode (morph +
  HPF'd-opposing-wave quirk) + sweep-position knob.
- Per-step gate-length + per-step velocity wired into envelopes/VCA.
- Tests: detune beats osc1; sub one octave down; sweep endpoints == faithful Saw/Square;
  per-step gate-len changes env gate duration; velocity scales VCA.

## M7 — GUI (nih_plug_egui)
- `editor.rs`: 303-style knob panel (cutoff, reso, envmod, decay, accent, tuning, waveform,
  osc2/sub/sweep, OS factor, poles, volume). Stage 1 = knobs.
- Stage 2 = 16-step grid editor (pitch + lane toggles + per-step gate-len/velocity).
- Stage 3 = advanced-env panel toggling AD↔full ADSR (A/S/R revealed; off ⇒ AD-identical).
- Tests: param round-trips through the editor state; advanced-off ADSR ≡ AD (bit guard).

## M8 — Research tiers + OS switch finalize
- Fixed 2×/4×/8× oversampling: cascade `Oversample2x` (lift from ba283 `plugin/src/oversample.rs`)
  for ×4/×8; OS-factor param; reallocate stage state OUTSIDE `process` (initialize/handshake).
- Flagged `ladder_wdf` (Cargo feature) — diode-ladder WDF research impl behind `Filter303`.
- Flagged `osc_wdf` (Cargo feature) — circuit-osc research impl behind `OscCore`.
- Tests: OS switch = no click / no realloc-in-callback (assert_no_alloc); feature-gated impls
  compile + pass the same fit harness (or are documented research spikes if they can't).

---

## Definition of done (Phase 2)
- All milestone tests green; assert_no_alloc green; VST3 + standalone build via xtask/bundler.
- VCF + VCO measurably track their Phase-1 ngspice gold (fit fixtures pass).
- 16-step sequencer + MIDI play a recognizable 303 line with accent/slide; souped levers work.
