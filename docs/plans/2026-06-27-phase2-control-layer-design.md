# Phase 2 Control / Musical Layer — M4–M7 on the Fidelity-Tier Voice (Design)

**Date:** 2026-06-27
**Status:** Design approved (brainstorm); implementation plan to follow (`writing-plans`).
**Predecessor:** Fidelity tiers F1–F6 COMPLETE — 3-tier voice (Low/High/Ultra), click-free
crossfade, PDC, zero-alloc, spectrally gated. See `HANDOFF.md`.
**Supersedes (re-targets):** `docs/plans/2026-06-26-phase2-303-voice-design.md` — its M4–M8
*semantics* are reused, but its architecture predates the tier pivot (it assumed one TPT
ladder, fitted-DSP osc, OS-as-a-param). This design lands the control layer on the existing
tier `Voice`.

## Context

The fidelity DSP core is done but **not yet a playable instrument**: `params.rs` exposes
only `tier`; `lib.rs` consumes `NoteOn` only (explicit M4/M6 deferral comment); the voice
has one hardcoded linear AD used as the VCA; there is **no NoteOff, velocity, knob params,
no MEG vs VEG split, no accent, no glide, no sequencer, no GUI**. This round builds the
control / musical layer that turns the done core into a souped-up TB-303.

## Locked decisions (brainstorm)

- **Deliverable:** full playable synth incl. GUI — M4 → M5 → M6(reduced) → M7.
- **Note source:** host MIDI wired first (gate/velocity/accent); internal 16-step sequencer
  next; MIDI overrides the sequencer while held.
- **Accent:** circuit-faithful — leaky-integrator accent-sweep that **stacks on consecutive
  accents** (rising cutoff peaks, Whittle); exp-decay AD shapes.
- **Oscillator:** **saw-only for now.** Square/sweep/osc2/sub (M6 *osc* extras) deferred —
  they collide with the per-tier osc split (Low/High polyBLEP saw vs Ultra circuit VCO
  pulse/ramp taps) and need their own design pass. M6 here is reduced to the sequencer
  souped levers (per-step gate-length + velocity).
- **Build order:** tiers are the done DSP substrate; layer control on top, wiring into the
  existing `Voice` / `render_tier` path. No tier/DSP-core changes.

## Approach

A thin, tier-agnostic **control plane** above `Voice::render_tier`. The audio chain stays
exactly as built; modulation is injected into the two existing fan-out setters
(`set_cutoff`, `set_reso`), and the single VCA `env` is replaced with **two** envelopes
(MEG → cutoff, VEG → VCA) plus an **accent integrator** and a **glide** slew. Sequencer and
MIDI feed one shared `{pitch, gate, accent, slide, velocity}` event bus consumed per-sample.
GUI is `nih_plug_egui`, staged. All new state is fixed-size, preallocated → `no_alloc` gate
stays green.

### Signal / control plane

```
[seq | MIDI] → {pitch,gate,accent,slide,vel} bus
   pitch → glide slew → osc.set_freq        (per tier: Saw | Vco303)
   gate  → MEG (fast-attack AD) ─┐
   gate  → VEG (fast-attack AD) ─┼→ VCA
   accent gate → accent-sweep integrator (stacks) → +cutoff, −MEG decay, +VCA
   cutoff = knob + env_mod·MEG + accent_cut → Voice::set_cutoff (fans to all tiers)
   out = render_tier · VEG · (1 + accent_vca·vel)
```

Envelopes/accent/glide live **outside** `render_tier`, so a tier crossfade never retriggers
the contour.

### New / changed files (`plugin303/src/`)

- **`dsp/env.rs` (rewrite)** — `Env`: fast-attack **AD** default (exp-curve segments) +
  **advanced ADSR** mode collapsing *bit-identical* to AD when off (A fast, S=0, R=decay).
  Two instances: MEG (filter), VEG (VCA). Keep `set_attack/set_decay`; add
  `set_sustain/set_release/gate_off` + `advanced` flag.
- **`dsp/accent.rs` (new)** — accent-sweep leaky integrator (one-pole, slow decay) that does
  not fully discharge between steps → consecutive accents stack into rising peaks. Outputs
  `+cutoff`, `−MEG decay`, `+VCA` offsets, driven by an accent gate.
- **`dsp/glide.rs` (new)** — one-pole pitch (log-freq) slew, armed only on slide steps /
  legato MIDI overlap; ~50–200 ms. Targets osc `set_freq`.
- **`control/sequencer.rs` (new)** — 16-step engine, per step
  `{semitone, octave: ±, gate: Note|Rest|Tie, accent, slide, gate_len, velocity}`. Host
  transport (tempo/ppq/playing) → 16th clock; emits the bus. Extended/slide-gate logic per
  `sequencer-envelopes.md` (slide → gate extends into next step + glide armed; tie →
  extended gate). Standalone free-run fallback clock.
- **`dsp/voice.rs` (wire)** — replace `env` with `meg`+`veg`+`accent`+`glide`; add
  `note_on(note, velocity, accent, slide)` / `note_off()` / gate handling; per-sample
  glide→freq, advance MEG/VEG + accent, compute cutoff, VCA. `render_tier` untouched;
  crossfade intact.
- **`params.rs` (expand)** — `tune, cutoff, reso, env_mod, decay (MEG), accent, glide,
  volume, poles {2,3,4}, advanced_env + per-env A/S/R`, sequencer `run/sync/length`, and a
  persisted 16-step pattern (`#[persist]`). Keep `tier`. Smoothed knobs via nih-plug
  `Smoother`.
- **`lib.rs` (wire)** — drain MIDI (`NoteOn{note,velocity}` → gate-on + velocity→accent/VCA;
  `NoteOff` → gate-off, mono last-note priority + held-note stack); read transport once per
  block; tick sequencer; MIDI-held overrides sequencer. Remove the `lib.rs:102` deferral.
- **`editor.rs` (new, M7)** — `nih_plug_egui` staged: knob row → 16-step grid editor →
  advanced-env panel (gated by `advanced_env`). GUI writes params only; no audio-thread
  alloc/lock.

## Milestones

- **M4 — Envelopes + accent + glide + MIDI gate** → playable mono MIDI synth.
- **M5 — Sequencer** (16-step PRASE + octave + transport sync + MIDI override + persistence).
- **M6 — Sequencer souped levers** (per-step gate-length + velocity). *Osc extras deferred.*
- **M7 — GUI** (staged egui panel).

Each milestone: builds, tests green, audible/measured before the next.

## Reuse (don't rebuild)

- `Voice::set_cutoff/set_reso` already fan out to all three tier backends — MEG/accent ride
  them; tier switches stay click-free on the param axis for free.
- `midi_to_freq`; `Vco303::set_freq` per-note LUT is alloc-free (F6.6) — glide can call it
  per sample.
- Tier crossfade / PDC / no-alloc machinery untouched, already gated (`tests/tier_switch.rs`,
  `tests/no_alloc.rs`).
- 303 semantics + numbers: `vibe303-old/sequencer-envelopes.md`; M4/M5/M6 of the 2026-06-26
  design doc.

## Risks / watch-items

- **Per-sample cutoff modulation cost** — MEG drives `set_cutoff` every sample (re-runs the
  nodal-ladder coeff update on High/Ultra). Acceptable (Ultra is offline-grade); if High
  slips RT budget, throttle MEG→cutoff to a control-rate sub-block.
- **No-alloc gate** — every new struct fixed-size preallocated; persisted pattern edits
  double-buffer, never realloc in `process`.
- **Tier switch must not retrigger** envelopes/accent/glide (they live outside `render_tier`).
- **Transport edge cases** — standalone free-run clock; loop-jump/stop handled by `reset()`.

## Testing / validation

- `cargo test -p plugin303` stays green (65+); add: envelope AD shape + fast-attack timing +
  ADSR≡AD-when-off bit guard; accent-sweep stacking/decay; glide slews only on slide steps;
  sequencer gate logic (note/rest/tie, slide→extended-gate+glide-arm, accent, gate-len/
  velocity, transport-sync boundaries); MIDI NoteOff gate-off + velocity→accent; `no_alloc`
  across the new path.
- `render303` — render an accented line; eyeball the rising-peak cutoff sweep.
- Manual DAW smoke-test (`docs/DAW_SMOKE_TEST.md`, the pending F6.9): MIDI clip with
  accents/slides, internal sequencer, A/B vs *Da Funk*; confirm click-free tier switch + PDC
  under the new control layer.

## References

- `HANDOFF.md` — F1–F6 state, gotchas.
- `vibe303-old/sequencer-envelopes.md` — 303 sequencer/envelope/accent semantics (Whittle /
  Devil Fish / Stinchcombe).
- `docs/plans/2026-06-26-phase2-303-voice-design.md` — predecessor design (M-milestone
  semantics).
- `eval/303/VCF_MAP.md`, `circuit/vco303.toml`, `circuit/vcf303.toml` — validated circuit IR.
