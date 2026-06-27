# HANDOFF — Souped-up TB-303 (vibe303)

**Repo:** `~/Git/vibe303` — standalone 303 project (remote `gustavokch/vibe303`, branch
`main`). The BA283 preamp product was removed (PR #1, merge `67b222f`); git history
retains it (pre-removal ba283 code at `af74b60`). Phase-1 circuit work carried over.

**Active branch:** `feat/phase2-303-m1` (Phase-2 build in progress).
**Design + plan:** `docs/plans/2026-06-26-phase2-303-voice-design.md` +
`docs/plans/2026-06-26-phase2-303-voice-implementation-plan.md`.
**Original design doc:** `~/.claude/plans/brainstorming-lets-use-our-eventual-waffle.md`.

---

## Goal

A monophonic nih-plug VST3/CLAP TB-303 synth (`plugin303`), realtime DSP grounded against
the Phase-1 ngspice circuit models. Phase 1 (SPICE/ACME circuit model) is COMPLETE; Phase 2
(the plugin voice) is now being built, vertical-slice-first (Strategy A), milestones M1–M8.

**Phase-2 locked decisions** (see the design doc): VCF = TPT diode-ladder ships (WDF flagged
tier later); VCO = fitted antialiased DSP ships (WDF osc flagged tier); fixed 2×/4×/8×
oversampling (default 2×, NO adaptive-os — it causes underruns); souped levers (2nd osc +
detune, sub, saw↔square Sweep, 2/3/4-pole tap + self-osc); built-in 16-step sequencer
(per-step gate-len + velocity) + live MIDI; envelopes = faithful AD default, GUI-gated
advanced ADSR.

---

## Current progress (Phase 2)

**M2 filter fidelity — COMPLETE on `feat/phase2-303-m2`.** Each task TDD (red→green),
committed separately. Next: finish the branch (PR/merge) → M3 osc fidelity.

- ✅ **M2.1 (a701daa)** — ZDF resonance feedback in `TptLadder`: 4-pole TPT closed by a
  `k`-weighted global loop solved instantaneously (`y4=a^4*u+Σ`, `u=x−k*y4`, `a=g/(1+g)`).
  `set_reso(r∈0..1)`→`k∈0..4` (4=self-osc); `k=0` reduces to the M1 cascade.
- ✅ **M2.2 (5aaf5cd)** — per-stage tanh saturation: each integrator state soft-clipped
  `H*tanh(x/H)` (H=8), ~linear small-signal (AC fit untouched), bounds self-osc/overdrive.
- ✅ **M2.3 (aa7ede9)** — `set_poles(2|3|4)` output tap (−12/−18/−24 dB/oct); forward pass
  captures each stage's LP output. Test rms warms up before measuring deep stopband.
- ✅ **M2.4 (ead8fd6)** — `run_vcf303_spice.py --json` → committed `eval/303/vcf303_gold.json`;
  `plugin303/tests/vcf303_fit.rs` asserts bias-robust invariants within bound (corner ratio
  ~6.0, fc-tracking resonance peak at cutoff reaching gold ~20 dB, per-pole tap step ~14 dB).
  Absolute dB not compared (SPICE passband peaks with bias). serde_json dev-dep.

**Full plugin303 test suite: 16 passed (4 suites).** (Voice still uses the default 1 kHz
cutoff; wiring cutoff/reso/env-mod to params lands in M4/M6/M7.)

**M1 walking skeleton — COMPLETE (merged, PR #2 `a589b96`→`ad6495a`).** Each task TDD,
committed separately.

- ✅ **M1.1 (9e7094c)** — `plugin303` workspace crate. Silent nih-plug synth: output-only
  IO layout, `MidiConfig::Basic`, Vst3/Clap exports, empty `Plugin303Params`. `cargo build
  -p plugin303` clean.
- ✅ **M1.2 (59db027)** — `plugin303/src/dsp/osc.rs` `Saw`: polyBLEP antialiased saw
  (M1 stub; circuit-faithful saw/square fitted to vco303 comes in M3). 2 tests (DC≈0, range).
- ✅ **M1.3 (d97eaed)** — `plugin303/src/dsp/ladder.rs` `TptLadder`: linear 4-pole TPT
  (4 cascaded 1-pole stages, sin/tan-coeff `g`; resonance/tanh/tap come in M2). 1 test
  (passband>0.6 / stopband<0.05).
- ✅ **M1.4 (d7f205b)** — `plugin303/src/dsp/env.rs` `AdEnv`: gate-triggered attack-decay,
  linear ramps, Idle/Attack/Decay state machine (exp decay + accent in M4). 3 tests.
- ✅ **M1.5 (7e667ba)** — `plugin303/src/dsp/voice.rs` `Voice`: osc(Saw)→TptLadder→VCA(AdEnv);
  `note_on` maps equal-tempered MIDI→Hz (A4=69=440) + triggers env. 3 tests.
- ✅ **M1.6 (85eb7d8)** — `process()` holds a `Voice`; `initialize()` builds at
  `cfg.sample_rate`, `reset()` clears tail; drains `NoteEvents` sample-accurately
  (`NoteOn`→`note_on`), writes mono voice to all channels. Note now audible in a host.
- ✅ **M1.7 (f0f98d3)** — `plugin303/tests/no_alloc.rs`: `AllocDisabler` gate asserts the
  `voice.tick` 4096-sample hot path allocates 0× (`dsp` made `pub` for the test).

**Phase 1 — COMPLETE (reference, see appendix + `eval/303/VCF_MAP.md`).** VCO core
oscillates + 1 V/oct (ngspice); VCF = 4-pole LP + bias-tracking cutoff + fc-tracking
resonance (active Q9 feedback) + tap-select 2/3/4-pole. ACME parity = NO-GO for both
(ngspice is ground truth). Circuit IR: `circuit/{vco303,vcf303}.toml`. Drift guard green,
`tests/circuitgen` 8/8.

---

## What worked

- **TDD per task** (write failing test → red → minimal impl → green → commit) keeps each
  M1 step small and verified. nih-plug builds clean against rev `f36931f7`.
- **Phase-1 ngspice harnesses** (`eval/303/run_v{co,cf}303_spice.py`) are the gold the
  Phase-2 DSP fits against (M2/M3).
- **circuitgen drift guard** rewritten to cover the 303 backends via `render_circuits()`
  (was BA283-only); regenerate with `python -m tools.circuitgen`.

## What didn't work (don't repeat)

Phase-2 build: nothing failed yet. Phase-1 gotchas still relevant for the DSP-fit work:
- **ACME = NO-GO for both 303 circuits** (osc relaxation needs adaptive timestepping; VCF
  ideal-current bias defeats ACME's DC homotopy → nonphysical branch). ngspice only.
- **PySpice can't drive ngspice-46** (all-zero vectors) → netlist with PySpice, RUN ngspice
  as a subprocess + parse `wrdata`. PySpice unit suffixes parse as atto in ngspice → emit
  harness sources via `raw_spice` with plain numbers. ngspice eats the first deck line as
  the title → start raw decks with `* comment`.
- **numpy 2.0** removed `ndarray.ptp()` → use `np.ptp(x)`.
- **VCF ladder AC rig:** bottom sink must be an ideal current sink (open at AC), not a
  resistor (a low-Z sink shorts the differential signal, cutoff stops tracking bias).

---

## Next steps

1. **Finish M1** (this branch): M1.4 AD env → M1.5 Voice → M1.6 process() → M1.7
   assert_no_alloc. Each TDD + commit per the plan. Then a note should be audible in a host.
2. **Complete the branch** (superpowers:finishing-a-development-branch): verify suite,
   PR/merge `feat/phase2-303-m1`.
3. **M2 filter fidelity** — swap `TptLadder` to the exact sin-coeff form + resonance `k` +
   per-stage `tanh`; 2/3/4-pole tap; self-osc; fit to `eval/303/run_vcf303_spice.py` gold
   (golden JSON fixture, bounded error).
4. **M3 osc fidelity** → M4 envelopes+accent+glide → M5 sequencer+MIDI → M6 souped levers →
   M7 GUI → M8 research tiers + OS switch. Outlines in the implementation plan.

---

## Commands

```bash
cd ~/Git/vibe303
# build / test the plugin
cargo build -p plugin303
cargo test  -p plugin303
# circuitgen: regenerate backends / drift-guard
uv run --no-project python -m tools.circuitgen [--check]
uv run --no-project --with pytest python -m pytest tests/circuitgen/ -q
# ngspice 303 renders (the Phase-2 fit gold)
uv run --no-project --with PySpice --with numpy python eval/303/run_vco303_spice.py 5
uv run --no-project --with PySpice --with numpy python eval/303/run_vcf303_spice.py --reso
# ACME (project-scoped; NO-GO verdict — reference only)
julia --project=eval/acme_vs_spice eval/303/vco303_acme.jl 5 48000 0.15
```

Memory: mem0 (`user=gustavo`, `app_id=gustavokch-vibe303`). File memory under
`~/.claude/projects/-home-gustavo-Git-vibe303/memory/` (note: fixed-oversampling-over-adaptive).

---

## Appendix — Phase 1 reference (COMPLETE)

Full per-step detail lives in **`eval/303/VCF_MAP.md`** (§1–§13: schematic read, active Q9
resonance, tap-select poles, ACME NO-GO) and the design doc. Condensed:

- **VCO core** (`circuit/vco303.toml`, 21 nodes 15R/5C/6Q/1D/1J): imported from the osc
  netlist; ngspice OSCILLATING + 1 V/oct (CV 4/5/6 → 231/477/1000 Hz). Self-start = ramp
  supply rails 0→V over 1 ms via PWL (`uic`/`.ic` and plain `.op` both fail). Solver opts:
  `method=gear, rshunt=1e12, gmin=1e-10, reltol=1e-3, abstol=1e-9, vntol=1e-6, itl4=1000,
  trtol=7`.
- **VCF** (`circuit/vcf303.toml`): 4-rung diode ladder (8×1N4148 + .033/.033/.033/.018µF) +
  T38 diff input + T20/T22 gain + T21 buffer + **active Q9 CE-amp resonance feedback** (the
  unity-buffer model never resonated; the real inverting CE amp closes the loop near fc).
  `run_vcf303_spice.py` gates: default 4-pole LP cutoff-tracks-bias; `--reso` fc-tracking
  resonance (532 Hz@16µA → 2.7 kHz@128µA); `--poles` tap-select (4=tl, 3=bl, 2=cl); `--op`.
- **circuitgen pipeline** (`tools/circuitgen` + `tools/schemagen`): IR (`[[diode]]`/`[[jfet]]`
  primitives) → SPICE + ACME emitters; `targets.py` registry renders vco303/vcf303 under
  `eval/303/`; `tests/circuitgen` drift-guards the generated files.
