# Phase 2 Control Layer (M4–M7) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Turn the done 3-tier `plugin303` DSP voice into a playable, souped-up TB-303 — MEG/VEG envelopes, circuit-faithful accent, glide, full knob params + MIDI gate (M4), a 16-step sequencer (M5), per-step gate-length/velocity (M6), and an egui GUI (M7).

**Architecture:** A tier-agnostic control plane sits above the existing `Voice::render_tier`. The audio chain is unchanged; modulation rides the existing `set_cutoff`/`set_reso` fan-out, the single VCA `env` is split into MEG (→cutoff) + VEG (→VCA) plus an accent leaky-integrator and a glide slew. Sequencer + MIDI feed one `{pitch,gate,accent,slide,vel}` bus. Envelopes/accent/glide live OUTSIDE `render_tier` so tier crossfades never retrigger them. All new state is fixed-size → the `no_alloc` gate stays green.

**Tech Stack:** Rust, nih-plug (VST3/CLAP), nih_plug_egui, `assert_no_alloc`. Tests are `cargo test -p plugin303`.

**Design doc:** `docs/plans/2026-06-27-phase2-control-layer-design.md`. **303 semantics:** `~/Git/vibe303-old/sequencer-envelopes.md`.

**Compat constraint:** `Voice::note_on(note: u8)` is called by `tests/no_alloc.rs` and the `voice.rs` unit tests. KEEP that signature (it becomes "trigger with velocity 1.0, no accent, no slide"); add fuller methods alongside. Do not break existing callers.

**Conventions:** TDD per task — write failing test, run it red, minimal impl, run green, commit. Run the FULL suite (`cargo test -p plugin303`) before each commit so the 65+ existing gates stay green. `cargo fmt -p plugin303` + `cargo clippy -p plugin303 --tests` clean before commit.

---

## Milestone M4 — Envelopes + Accent + Glide + MIDI Gate

End state: a playable mono MIDI synth — NoteOn/NoteOff gate, velocity→accent, MEG sweeps cutoff, VEG drives VCA, accent stacks, slide glides. All knob params exposed.

### Task M4.1: MEG/VEG envelope — rename + exp curve + gate-off

**Files:**
- Modify: `plugin303/src/dsp/env.rs`
- Test: in-file `#[cfg(test)] mod tests`

Current `AdEnv` is a linear AD with no gate-off. Generalize it to an `Env` that keeps the
fast-attack AD default but uses an exponential-approach decay and supports an explicit
gate-off (needed for VEG release + ADSR later). Keep it usable as both MEG and VEG.

**Step 1: Write the failing test** — append to `env.rs` tests:

```rust
#[test]
fn decay_is_exponential_not_linear() {
    // Exp decay: the drop in the first half of the decay window exceeds the drop
    // in the second half (a linear ramp would make them equal).
    let sr = 48_000.0;
    let mut e = Env::new(sr);
    e.set_attack(1.0);
    e.set_decay(100.0); // ~4800 samples
    e.trigger();
    for _ in 0..60 { e.tick(); } // past attack, at ~peak
    let start = e.tick();
    let mut mid = start;
    for _ in 0..2_400 { mid = e.tick(); }
    let mut end = mid;
    for _ in 0..2_400 { end = e.tick(); }
    let first_half = start - mid;
    let second_half = mid - end;
    assert!(first_half > second_half * 1.5, "decay not exponential: {first_half} vs {second_half}");
}

#[test]
fn gate_off_before_peak_releases() {
    let mut e = Env::new(48_000.0);
    e.set_attack(50.0); // slow attack so we can gate off mid-rise
    e.set_decay(50.0);
    e.trigger();
    for _ in 0..200 { e.tick(); }
    let lvl = e.tick();
    e.gate_off();
    let mut tail = lvl;
    for _ in 0..6_000 { tail = e.tick(); }
    assert!(tail < lvl * 0.1, "gate_off did not release: {tail} from {lvl}");
}
```

**Step 2: Run red** — `cargo test -p plugin303 --lib env 2>&1 | tail` → FAIL (`Env` undefined).

**Step 3: Implement.** Rename `AdEnv`→`Env`, keep `new/set_attack/set_decay/trigger/tick`.
Make decay an exponential approach toward 0 (one-pole coefficient from the decay time
constant) rather than a fixed linear `dec_inc`; add a `Release` stage + `gate_off()` that
switches to a release ramp (defaults to the decay coefficient → AD behavior unchanged when
gate-off never fires). Add a thin `pub type AdEnv = Env;` alias OR update the `voice.rs`
import — prefer updating the import (one line) and removing the alias to avoid dead names.
Keep the existing `idle_outputs_zero` / `stays_in_unit_range` tests passing.

**Step 4: Run green** — `cargo test -p plugin303 --lib env` → PASS. Then full suite green.

**Step 5: Commit** — `feat(303): exp-decay Env with gate-off (MEG/VEG base)`.

### Task M4.2: ADSR-advanced mode collapsing bit-identical to AD

**Files:** Modify `plugin303/src/dsp/env.rs` + tests.

**Step 1: Failing test:**

```rust
#[test]
fn advanced_off_is_bit_identical_to_ad() {
    let sr = 48_000.0;
    let mut ad = Env::new(sr);   ad.set_attack(3.0); ad.set_decay(200.0); ad.trigger();
    let mut adv = Env::new(sr);  adv.set_attack(3.0); adv.set_decay(200.0);
    adv.set_advanced(true); adv.set_sustain(0.0); adv.set_release(200.0); adv.trigger();
    for _ in 0..20_000 {
        assert_eq!(ad.tick().to_bits(), adv.tick().to_bits(), "advanced≠AD when S=0,R=decay");
    }
}
```

**Step 2: Run red.** **Step 3:** Add `advanced: bool`, `sustain`, `release` fields +
`set_advanced/set_sustain/set_release`. When `!advanced`: ignore sustain/release, decay to 0
exactly as the AD path (the existing code path — guarantees bit-identity). When `advanced`:
decay to `sustain`, hold, release on `gate_off`. **Step 4:** green + full suite. **Step 5:**
commit `feat(303): advanced ADSR mode (collapses to AD)`.

### Task M4.3: Accent-sweep leaky integrator

**Files:** Create `plugin303/src/dsp/accent.rs`; register `pub mod accent;` in `dsp/mod.rs`.

The accent sweep is a one-pole leaky integrator that does NOT fully discharge between steps,
so consecutive accents stack into rising peaks (Whittle). On each accented gate it receives a
kick; it decays slowly between steps; its level scales `+cutoff`, `−MEG decay`, `+VCA`.

**Step 1: Failing test** (in `accent.rs`):

```rust
#[test]
fn consecutive_accents_stack_to_rising_peaks() {
    let sr = 48_000.0;
    let mut a = AccentSweep::new(sr);
    a.set_amount(1.0);
    let step = (sr * 0.125) as usize; // 16th @ 120 BPM
    let mut peak1 = 0.0f32;
    a.gate_on();
    for _ in 0..step { peak1 = peak1.max(a.tick()); }
    let mut peak2 = 0.0f32;
    a.gate_on(); // second accent before full discharge
    for _ in 0..step { peak2 = peak2.max(a.tick()); }
    assert!(peak2 > peak1 * 1.1, "second accent did not stack: {peak1} -> {peak2}");
}

#[test]
fn single_accent_decays_back_down() {
    let sr = 48_000.0;
    let mut a = AccentSweep::new(sr);
    a.set_amount(1.0);
    a.gate_on();
    for _ in 0..200 { a.tick(); }
    let hi = a.tick();
    for _ in 0..(sr as usize) { a.tick(); } // 1 s, no further accents
    let lo = a.tick();
    assert!(lo < hi * 0.2, "accent sweep did not decay: {hi} -> {lo}");
}
```

**Step 2: Run red.** **Step 3:** Implement `AccentSweep { level, kick, decay_coef, amount }`
with `new(sr)`, `set_amount(f32)`, `gate_on()` (adds `kick` to `level`, clamped), `tick()`
(applies one-pole decay toward 0, returns `level*amount`), and accessor offsets
`cutoff_offset()/decay_scale()/vca_offset()` derived from the swept level. Pick decay time so
the level retains a meaningful fraction across one 16th step (slow discharge) — document the
constant. **Step 4:** green + full suite. **Step 5:** commit `feat(303): accent-sweep leaky integrator (stacks)`.

### Task M4.4: Glide (pitch slew)

**Files:** Create `plugin303/src/dsp/glide.rs`; register in `dsp/mod.rs`.

One-pole slew in log-frequency (so glide rate is musical across octaves), armed only when a
slide is active; when disarmed it jumps instantly to target.

**Step 1: Failing test:**

```rust
#[test]
fn armed_glide_slews_disarmed_jumps() {
    let sr = 48_000.0;
    let mut g = Glide::new(sr);
    g.set_time(100.0); // ms
    g.set_target(110.0); g.reset_to(110.0);
    // disarmed: instant
    g.set_armed(false); g.set_target(220.0);
    assert!((g.tick() - 220.0).abs() < 1.0, "disarmed should jump");
    // armed: slews, not instant
    g.reset_to(110.0); g.set_armed(true); g.set_target(220.0);
    let first = g.tick();
    assert!(first > 110.0 && first < 200.0, "armed should slew gradually: {first}");
    for _ in 0..20_000 { g.tick(); }
    assert!((g.tick() - 220.0).abs() < 1.0, "armed should reach target");
}
```

**Step 2: Run red.** **Step 3:** Implement `Glide { cur_log, target_log, coef, armed }` —
slew `cur_log` toward `target_log` by `coef` each `tick` when armed, snap when not; `tick()`
returns `exp2`/`powf` back to Hz. **Step 4:** green + full suite. **Step 5:** commit
`feat(303): log-freq glide slew`.

### Task M4.5: Wire MEG/VEG/accent/glide into Voice

**Files:** Modify `plugin303/src/dsp/voice.rs` + its in-file tests.

Replace the single `env: AdEnv` with `meg: Env`, `veg: Env`, `accent: AccentSweep`,
`glide: Glide`. Add base-cutoff state (`cutoff_base: f32`) and modulation params
(`env_mod: f32`, `accent_amt`). Per-sample in `tick()`, BEFORE `render_tier`:
- `let f = self.glide.tick();` then `self.osc.set_freq(f,sr); self.osc_hi.set_freq(f,sr); self.vco.set_freq(f);`
- advance `let acc = self.accent.tick();`
- `let cut = self.cutoff_base + self.env_mod * self.meg.tick() * MEG_SWEEP_HZ + acc*ACC_CUT_HZ;` → `self.set_cutoff(cut.clamp(...))`
- AFTER `render_tier`: `out * self.veg.tick() * (1.0 + acc * ACC_VCA)`

API (keep `note_on(note)` working):
- `note_on(note)` → `self.trigger(note, /*vel*/1.0, /*accent*/false, /*slide*/false)`
- `note_on_full(note, vel: f32, accent: bool, slide: bool)` — sets glide target (armed=slide),
  triggers meg+veg, `if accent { self.accent.gate_on(); }`, stores vel.
- `note_off()` → `self.meg.gate_off(); self.veg.gate_off();`
- setters: `set_env_mod`, `set_decay` (sets MEG decay), `set_accent_amount`, `set_glide`,
  `set_volume`. (`set_cutoff` becomes `set_cutoff_base`; keep a thin `set_cutoff` for tests
  that sets the base.)

**Step 1: Failing tests** (append to `voice.rs` tests):

```rust
#[test]
fn note_off_silences_voice() {
    let mut v = Voice::new(48_000.0);
    v.set_cutoff(2_000.0);
    v.note_on(57);
    for _ in 0..200 { v.tick(); }
    v.note_off();
    for _ in 0..48_000 { v.tick(); } // 1 s release
    let mut acc = 0.0f32;
    for _ in 0..2_000 { let s = v.tick(); acc += s*s; }
    assert!((acc/2_000.0).sqrt() < 1e-3, "note_off did not silence");
}

#[test]
fn env_mod_sweeps_cutoff() {
    // With env_mod up, early (env high) output is brighter than late (env low).
    fn hf(envmod: f32) -> f32 {
        let mut v = Voice::new(48_000.0);
        v.set_cutoff(200.0); v.set_env_mod(envmod); v.set_decay(400.0);
        v.note_on(45);
        let mut early = 0.0f32;
        for _ in 0..2_000 { let s=v.tick(); early += s*s; }
        early
    }
    assert!(hf(1.0) > hf(0.0) * 1.2, "env_mod did not open the filter");
}
```

**Step 2: Run red.** **Step 3:** implement the wiring above; pick `MEG_SWEEP_HZ`,
`ACC_CUT_HZ`, `ACC_VCA` constants (document; tune by ear later). Update the existing
`produces_signal_after_note_on` / `cutoff_setter_changes_brightness` tests if the
`set_cutoff` semantics shift (they set the base cutoff — should still pass).
**Step 4:** green + FULL suite (watch `no_alloc.rs`, `voice_esr.rs`, `tier_*`). **Step 5:**
commit `feat(303): wire MEG/VEG/accent/glide into Voice (M4)`.

### Task M4.6: Expand params.rs

**Files:** Modify `plugin303/src/params.rs`. Test: `plugin303/tests/params_tier.rs` style.

Add `FloatParam`s (with `Smoother` + ranges + units): `tune` (semitones, ±12), `cutoff`
(Hz, skewed 30–12k), `reso` (0–1), `env_mod` (0–1), `decay` (ms, 30–2000, skewed), `accent`
(0–1), `glide` (ms, 0–300), `volume` (dB or gain). `IntParam`/`EnumParam` `poles {2,3,4}`.
`BoolParam advanced_env` + per-env `attack/sustain/release`. Keep `tier`.

**Step 1: Failing test** (`plugin303/tests/params.rs`, new): construct
`Plugin303Params::default()`, assert each param exists with expected default + range bounds.

**Step 2: Run red.** **Step 3:** implement. **Step 4:** green. **Step 5:** commit
`feat(303): full synth params (cutoff/reso/envmod/decay/accent/glide/...)`.

### Task M4.7: Wire MIDI gate + params in lib.rs

**Files:** Modify `plugin303/src/lib.rs`.

Per block: push smoothed param values into the voice (`set_cutoff_base`, `set_reso`,
`set_env_mod`, `set_decay`, `set_accent_amount`, `set_glide`, `set_volume`, `set_poles`).
In the event loop: `NoteOn{note,velocity}` → mono last-note-priority push onto a fixed
held-stack + `voice.note_on_full(note, velocity, accent_from_vel, slide_from_legato)`;
`NoteOff{note}` → pop; if stack empty `voice.note_off()`, else retrigger the new top (legato
→ slide armed). Map high velocity (≥ ~0.8) to accent for MIDI play. Remove the `lib.rs:102`
deferral comment.

**Step 1: Failing test** — `lib.rs` is awkward to unit-test without a host buffer; instead
add a `voice.rs` test for the mono held-stack helper (extract note-priority into a small
`fn` or a `NoteStack` struct with a unit test: push 60, push 64, release 64 → returns to 60).

**Step 2: Run red.** **Step 3:** implement `NoteStack` (fixed `[u8; 16]` + len, zero-alloc)
+ the lib.rs wiring. **Step 4:** green + full suite. **Step 5:** commit
`feat(303): MIDI gate (NoteOff + velocity→accent + legato slide) + param push (M4)`.

### Task M4.8: M4 verification gate

- `cargo test -p plugin303` — all green (existing 65+ plus new env/accent/glide/voice/params).
- `cargo run --bin render303 -p plugin303 -- --note A2 --tier high --out /tmp/m4.f32` runs.
- `cargo fmt -p plugin303` + `cargo clippy -p plugin303 --tests` clean.
- Build the bundle (`./scripts/build-linux-x86_64-v3.sh`) — loads without error.
- Commit any fixups. **M4 done: playable MIDI synth.**

---

## Milestone M5 — 16-Step Sequencer (task outline; expand to step-granularity when reached)

**Files:** Create `plugin303/src/control/sequencer.rs` (+ `pub mod control;` in `lib.rs`).

- **M5.1** `Step { semitone: i8, octave: i8, gate: Gate {Note,Rest,Tie}, accent: bool, slide: bool, gate_len: f32, velocity: f32 }` + `Pattern { steps: [Step; 16], len: u8 }`. Unit tests for construction/defaults. Commit.
- **M5.2** `Sequencer` clock: given `(tempo_bpm, ppq_pos, playing)` from transport, emit a step-advance on each 16th boundary. Test boundary detection with synthetic ppq values. Commit.
- **M5.3** Gate logic: Note→regular gate, Rest→none, Tie/slide→extended gate into next step; slide → glide armed on the FOLLOWING step's note (per `sequencer-envelopes.md`). Tests for each case (note/rest/tie/slide-extends, slide-arms-next). Commit.
- **M5.4** Emit the `{pitch,gate,accent,slide,velocity}` bus → drive `voice.note_on_full`/`note_off`. Standalone free-run clock fallback when `!playing` (internal tempo param). Commit.
- **M5.5** Persist the pattern: nih-plug `#[persist = "pattern"]` field (serde) on `Plugin303Params`; double-buffered read on the audio thread (no realloc in `process`). Test round-trip serialize/deserialize. Commit.
- **M5.6** `lib.rs`: tick sequencer per block; MIDI-note-held OVERRIDES the sequencer (suspend seq while a MIDI note is down, resume on release). Test the override flag logic via a helper. Commit.
- **M5.7** Verification: full suite green; `no_alloc` still green with the sequencer in the path; bundle loads; an internal pattern auto-plays in standalone. **M5 done.**

---

## Milestone M6 — Sequencer Souped Levers (task outline)

- **M6.1** Per-step `gate_len` scales the effective gate duration (and thus MEG/VEG decay window) for that step. Test: longer `gate_len` → longer audible tail. Commit.
- **M6.2** Per-step `velocity` scales VCA + accent intensity. Test: higher velocity step → louder + brighter. Commit.
- **M6.3** Verification: full suite green; bundle loads. **M6 done.** *(Osc extras — square/sweep/osc2/sub — remain deferred to a separate design round.)*

---

## Milestone M7 — GUI (task outline)

**Files:** Create `plugin303/src/editor.rs`; add `nih_plug_egui` dep; wire `editor()` in `lib.rs`. See `nih-plug-best-practices` skill.

- **M7.1** egui scaffold: `Plugin303::editor()` returns a window; render the tier + a couple of knobs bound to params (param-write only, no audio-thread access). Manual: knob moves the param. Commit.
- **M7.2** Full knob row: cutoff/reso/envmod/decay/accent/tune/volume + tier + poles, 303-style layout. Commit.
- **M7.3** 16-step grid editor: pitch + note/rest/tie + accent/slide/octave lanes + gate-len/velocity, writing into the persisted pattern via the lock-free store. Commit.
- **M7.4** Advanced-env panel: per-env A/S/R, shown only when `advanced_env` on. Commit.
- **M7.5** Verification: bundle loads in a DAW; GUI edits patterns + knobs live; `no_alloc` audio path unaffected. **M7 done.**

---

## Final verification (whole round)

- `cargo test -p plugin303` fully green; `cargo fmt` + `cargo clippy --tests` clean.
- `./scripts/build-linux-x86_64-v3.sh` installs the VST3/CLAP bundle.
- **Manual DAW smoke-test** (`docs/DAW_SMOKE_TEST.md`, also the pending F6.9): MIDI clip with
  accents/slides, run the internal sequencer, sweep cutoff/reso/accent, A/B vs the *Da Funk*
  reference; confirm click-free tier switch + correct PDC under the new control layer.
- Update `HANDOFF.md` (M4–M7 done; osc-extras deferred), then finish-branch / PR.
