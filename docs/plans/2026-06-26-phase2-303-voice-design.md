# Phase 2 — Souped-up TB-303 nih-plug Voice (Design)

**Date:** 2026-06-26
**Status:** Design approved; implementation plan to follow (writing-plans).
**Predecessor:** Phase 1 (SPICE/ACME circuit model) COMPLETE — VCO core + diode-ladder
VCF built & ngspice-validated; ACME parity NO-GO for both (ngspice = ground truth).
See `HANDOFF.md`, `eval/303/VCF_MAP.md`, `circuit/vco303.toml`, `circuit/vcf303.toml`.

## Goal

A monophonic nih-plug VST3/CLAP synth — `plugin303` — that takes the Roland TB-303
"to the next level": faithful 303 voice (osc / diode-ladder VCF / MEG+VEG / accent /
16-step sequencer) plus souped-up levers, with the realtime DSP grounded against the
Phase-1 ngspice models.

## Locked decisions (from brainstorm)

- **Scope:** full voice architecture, built as a vertical slice first (Strategy A).
- **Packaging:** new `plugin303` workspace member crate in the `vibe303` workspace,
  self-contained; lift small DSP helpers (oversample pattern, smoothing idioms) from the
  preserved shared DSP modules as needed — no forced shared-lib refactor.
  (The ba283 preamp product was removed from this repo; only the reusable DSP/scaffold
  was kept.)
- **VCF engine:** **TPT diode-ladder ships** (Cytomic/Zavalishin, sin-coeff form, fitted
  to the vcf303 ngspice reference). **Rust-WDF ladder = flagged research tier**, later.
- **VCO engine:** **fitted antialiased DSP ships** (faithful saw/square fitted to vco303 +
  Din Sync references). **WDF circuit osc = flagged research tier**, later. (Phase 1 proved
  the relaxation core is ngspice-only — fixed-step solvers can't resolve its sub-µs edges.)
- **Oversampling:** FIXED, user-switchable **2× (default) / 4× / 8×**. Do NOT use ba283's
  `adaptive-os` — its CPU spikes cause buffer underruns (fixed = predictable load).
- **Souped levers (all in-scope):** 2nd osc + detune/mix; sub-osc (octave-down square);
  saw→square continuous **Sweep** mode; filter 2/3/4-pole tap select + resonance reaching
  true self-oscillation.
- **Control:** built-in iconic **16-step sequencer** (pitch + note/rest/tie + accent +
  slide + octave, souped per-step **gate-length** & **velocity**) AND live **MIDI** note
  input (overrides sequencer while held).
- **Envelopes:** default = faithful fast-attack **AD** (MEG + VEG). Advanced **ADSR** per
  envelope, GUI-gated; collapses bit-identical to AD when advanced mode off.

## Architecture

Monophonic voice (303 = one voice). Per-block `process()`, sample-accurate smoothed
params, no-alloc audio thread (assert_no_alloc gate, like ba283).

### Audio signal chain

```
[seq/MIDI] → pitch+glide ─▶ VCO BLOCK ─▶ VCF ─▶ VCA ─▶ out
                            osc1 (Saw|Square switch, +Sweep morph; PWM)
                            osc2 (detune, mix)        TPT diode-ladder
                            sub  (oct-down sq)        cutoff = knob +
                            └─ mix ─┘                 MEG·envmod + accent
                                                      reso → self-osc
                                                      2/3/4-pole tap     VEG·accent
```

### Control / modulation plane

```
clock(host transport, 16th) → SEQUENCER (16 steps × lanes)
   lanes: pitch, octave±, note/rest/tie, accent, slide, [gate-len], [velocity]
   ├─ pitch → glide slew (slide steps) → VCO V/oct
   ├─ gate  → MEG (fast-attack AD; advanced ADSR) → cutoff (× env-mod)
   ├─ gate  → VEG (fast-attack AD; advanced ADSR) → VCA
   ├─ accent → accent-sweep slew-limiter (stacks on consecutive accents)
   │           → +cutoff, −MEG decay, +VCA
   └─ velocity → VCA/accent scale
MIDI note-in → same pitch/gate/velocity bus (overrides seq when held)
```

## Components (`plugin303/src/`)

### DSP (`dsp/`)
- **`osc.rs` — VCO block.** Trait `OscCore`. Fitted impl: faithful **Saw** + **Square**
  (selected by a discrete waveform switch, matching the hardware), shapes fitted to the
  vco303 ngspice output + Din Sync C-note reference recordings (the real 303 waves are
  non-canonical — each resembles an HPF'd version of the other; reproduce the actual
  edges, not ideal ramps/squares). Square PWM duty V/oct-tracked **45→71%** (Whittle).
  Souped **Sweep** = continuous saw→square morph between the two faithful endpoints (not
  on the real 303). osc2 (detune cents, mix), sub (octave-down square). Flagged
  `osc_wdf` research impl targets the same two faithful shapes from the circuit directly.
- **`ladder.rs` — VCF.** Trait `Filter303 { set_cutoff/reso/poles, tick }`. Impl
  `TptLadder`: 4× cascaded 1-pole TPT (sin-coeff form) + per-stage `tanh` + resonance
  feedback `k` (`k→0` ⇒ self-osc); 2/3/4-pole = output-tap remix of already-computed
  states (free). Fitted to the vcf303 ngspice gold. Flagged `ladder_wdf` impl later.
- **`env.rs` — AD/ADSR.** Fast-attack AD default (MEG, VEG), analog-curve segments.
  Advanced full ADSR per env; A/S/R params persist but inert (A fast, S=0, R=decay ⇒
  AD-identical) unless advanced mode on.
- **`accent.rs`** — accent-sweep slew-limiter (leaky integrator, stacks consecutive
  accents) → cutoff/decay/VCA offsets.
- **`glide.rs`** — pitch slew (slide/portamento), one-pole, armed only on slide steps.
- **`smooth.rs`** — Cytomic dynamic-smoothing for CV / knob / per-step ramps (zipper-free,
  no lag on fast jumps).
- **`oversample.rs`** — FIXED 2×/4×/8× halfband up/down (lifted from ba283, adaptive
  stripped).

### Control
- **`sequencer.rs`** — 16-step engine; per-step `{semitone, octave, gate: Note|Rest|Tie,
  accent, slide, gate_len, velocity}`; host-transport clock → 16th steps; gate logic
  (tie/slide → extended gate; slide → glide arm).
- **`voice.rs`** — wires seq/MIDI → pitch/gate/vel bus → osc → filter → vca; owns
  envelopes/accent/glide; per-sample render.
- **`params.rs`** — `#[derive(Params)]`: tune, glide, waveform-mode {Saw,Square,Sweep} +
  sweep-pos, osc2 detune/mix, sub, cutoff, reso, envmod, MEG-decay, accent, VEG-decay,
  poles {2,3,4}, OS-factor {2,4,8}, advanced-env toggle, volume, seq run/sync/length +
  persisted 16-step pattern.

### Plugin shell
- **`lib.rs`** — `Plugin303` (process, MIDI events, transport read); Vst3/Clap export.
- **`editor.rs`** — nih_plug_egui: 303-style knob panel + 16-step grid editor + advanced-
  env panel. Staged (knobs → grid → advanced).

## Data flow / threading

**Per-block `process()`** (audio thread, no alloc):
1. Read host transport (tempo, playing, ppq) once.
2. Drain MIDI events → note/velocity bus.
3. Sequencer tick: advance on 16th boundaries (ppq); emit step events. MIDI note-on
   overrides seq pitch/gate while held.
4. Per-sample loop (inside OS-upsampled region for nonlinear blocks): glide→V/oct→osc
   block; MEG/VEG advance; accent integrator; cutoff = knob + MEG·envmod + accent →
   filter tick (tanh ladder, tap); VCA = VEG·(1+accent·vel) → out.
5. Downsample → write block. Per-sample param smoothing (nih-plug + `smooth.rs`).

**Threading.**
- Audio thread: render only, no alloc/lock (assert_no_alloc gate).
- GUI (egui): writes params via atomics/smoothers; reads scope/meter via lock-free atomics.
- Pattern edits: GUI writes into a fixed-size (16-step) atomic/double-buffered store the
  audio thread reads; no realloc mid-render.
- No background rebuild threads (coeffs recompute inline on param change).

**State/persistence.** Pattern + knobs + waveform switch + OS factor + advanced-env
toggle via nih-plug `Params` (custom persist field for the pattern). Preset = Params
snapshot.

**Error handling / robustness.**
- No host transport (standalone) → internal free-run clock at a tempo param.
- Denormals flushed; filter/env state clamped (self-osc bounded by tanh; NaN-guard reset
  on note-on).
- OS-factor change → reallocate halfband state OUTSIDE process (initialize / param
  handshake), never in the callback.
- Sample-rate change → recompute all coeffs in `initialize()`.

## Testing / validation

**Against the Phase-1 ngspice gold:**
- VCF fit: `TptLadder` AC magnitude vs `eval/303/run_vcf303_spice.py` gold — cutoff
  tracking, 4-pole slope, resonance peak fc/height, 2/3/4-pole tap slopes. Bounded fit
  error, checked-in golden fixture (like ba283 `wavenet_golden.json`).
- VCO fit: saw/square spectra + waveshape vs vco303 ngspice + Din Sync C-note references.

**Unit tests:**
- Sequencer gate logic (note/rest/tie, slide→extended-gate + glide arm, accent, per-step
  gate-len/velocity, transport-sync boundaries).
- Envelope AD shape + fast-attack timing; advanced ADSR ≡ AD when advanced-off (bit guard).
- Accent-sweep stacking/decay. Glide slew on slide steps only.
- assert_no_alloc in process (release-active). Denormal/NaN-reset guard; OS-switch =
  no click / no realloc-in-callback.

## Milestones (Strategy A — vertical slice first)

1. **M1 walking skeleton** — crate scaffold in workspace; MIDI note → 1 osc (faithful saw)
   → TptLadder → VEG/VCA → out; audible in standalone + DAW; assert_no_alloc green.
2. **M2 filter fidelity** — TPT fitted to vcf303 gold; cutoff/reso/envmod/self-osc;
   2/3/4-pole tap; fit test passes.
3. **M3 osc fidelity** — faithful Saw + Square (switch) fitted to vco303 / Din Sync; PWM
   duty tracking.
4. **M4 envelopes + accent** — MEG/VEG AD, accent-sweep, glide/slide.
5. **M5 sequencer** — 16-step engine + all lanes + transport sync + MIDI override;
   pattern persistence.
6. **M6 souped levers** — osc2 detune/mix, sub, saw→square Sweep; per-step gate-len/velocity.
7. **M7 GUI** — egui 303 panel (knobs) → step-grid editor → advanced-env panel.
8. **M8 research tiers** — flagged `ladder_wdf` + `osc_wdf`; finalize fixed 2×/4×/8× OS switch.

Each milestone: builds, tests green, audible/measured before the next.

## References

- `eval/303/VCF_MAP.md` — full VCF schematic read + §1–§13 findings (incl. active Q9
  resonance, tap-select poles, ACME NO-GO).
- `circuit/vco303.toml`, `circuit/vcf303.toml` — validated circuit IR.
- `eval/303/run_vcf303_spice.py`, `run_vco303_spice.py` — ngspice harnesses (the gold).
- Cytomic VA papers (SvfLinearTrapezoidalSin, DynamicSmoothing, matched-svf1-ivanstov,
  circuit-to-code) — TPT filter + control-smoothing building blocks. See design plan
  `~/.claude/plans/brainstorming-lets-use-our-eventual-waffle.md` §193.
- `vibe303-old/sequencer-envelopes.md` — 303 sequencer/envelope/accent semantics
  (Whittle / Devil Fish / Stinchcombe).
