# Line Preamp Path — Implementation Gap Documentation

**Date:** 2026-06-17
**Status:** Research / gap-capture only. No implementation. For future reference.
**Trigger:** Investigating a switchable LINE preamp path with a MIC/LINE toggle.

---

## Context — why this note exists

The plugin emulates the Neve channel-amp card, but **only a slice of the real
signal chain**. A MIC/LINE input toggle (real hardware feature) is not present.
Before designing it we mapped exactly what is and isn't modeled today, so a
future implementer doesn't rediscover the same gaps. This is the gap inventory,
not a plan to build.

---

## Real hardware chain vs what is modeled

Real EH 10023 channel strip (per `plugin/src/eq.rs:3-4`):

```
MIC  → T1(10468) → BA284 → EQ → BA283/AV → T3(LO1166) → OUT
LINE → T_line(31267) ─────→ EQ → BA283/AV → T3(LO1166) → OUT   (bypasses BA284)
```

Modeled today (WDF circuit + WaveNet NN backends):

```
[host audio] → EQ(eq.rs) → BA283/AV + LO1166 (wdf/ or wavenet.rs) → out
```

So the modeled span is **EQ + BA283/AV + LO1166 only**. Everything left of EQ —
both input transformers, the BA284 mic-pre, and the MIC/LINE switch — is absent.
Net effect: today's voicing already behaves like the **line-amp** (BA283), with
gain set by the BA283 feedback resistor R_KJ, not by a mic preamp.

---

## Gap inventory

### G1 — No input transformer modeled (mic or line)
- Explicit non-goal in `docs/plans/2026-06-01-ba283-design.md:20`:
  "Modelling input transformer, hi-Z line, or microphone loading. Single fixed
  input impedance."
- Hardware parts identified during research:
  - **MIC input transformer: Neve 10468** (classic 1073 Marinair, step-up).
  - **LINE input transformer: Neve 31267** (lower ratio, feeds amp directly).
- Neither is in the WDF netlist (`data_gen/netlist.py:1-41` starts at the BA283
  `INPUT` node) nor in the WaveNet graph.
- Missing transformer character: turns-ratio gain/loss, LF rolloff (primary L +
  source/term R), HF leakage resonance, and core saturation. Note a JA-core
  saturation model already exists for the output transformer behind the
  `trafo-core` feature — reusable shape for an input transformer.

### G2 — No BA284 mic-preamp stage
- The MIC path's gain/character stage (BA284, a full ~3-transistor amp like
  BA283) is unmodeled. Today's gain comes only from BA283 R_KJ (15–35 dB,
  stepped table in `lib.rs` GAIN_STEPS).
- Consequence: there is no faithful "mic" voicing distinct from "line" — the
  current model is effectively the shared downstream amp for both.

### G3 — No MIC/LINE toggle / no LINE path
- No `BoolParam` or routing exists for input mode. The pre-model insertion point
  is well-defined though: EQ runs pre-model (`eq.rs`), and the input transformer
  sits upstream of EQ, so a LINE transformer block would insert **ahead of EQ**.
- Toggle wiring pattern is established and reusable: `eq_on`, `level_norm`,
  `gain_stepped` all follow lib.rs BoolParam → `editor.rs` `toggle_pill()` →
  block-rate read in `process()`.

### G4 — Backend asymmetry for any pre-stage
- WDF is a live MNA solve: a transformer can be added as a real coupled-inductor
  with true input loading of BA283.
- WaveNet is trained end-to-end on the BA283 response with **no input
  transformer in its graph**; it cannot load a circuit. A LINE transformer would
  have to be either (a) a standalone DSP pre-block feeding both backends, or
  (b) WDF-integrated with WaveNet greyed/approximated. Undecided.

### G5 — Level/scaling unknown for line drive
- Input voltage convention is `0 dBFS = 0.5 V` (`INPUT_FS_VOLTS`, dsp.rs).
- Line sensitivity vs mic (the 31267 ratio / level offset into the BA283 INPUT
  node) is not characterized. Needed to place LINE level correctly.

### G6 — No data / measurement for the line path
- Training + WDF validation data (`data_gen/netlist.py`, SPICE reamp, eval
  harnesses) all start at the BA283 input. No SPICE model or measurement of
  31267, 10468, or BA284 exists. Any faithful add needs new netlist + data-gen.

---

## Design decisions reached during research (for whoever builds this)

- **LINE is the net-new path.** Existing model already ≈ the line amp, so:
  - MIC mode = today's behavior, unchanged.
  - LINE mode = add the 31267 line transformer in front of the existing chain.
  - BA284 + 10468 (full mic-pre fidelity) deferred — biggest effort, revisit later.
- **Insertion point** for the 31267 block = ahead of EQ (matches hardware:
  line xfmr → EQ → BA283).

## Open questions (unresolved — answer before implementing)

1. Backend scope: shared pre-EQ DSP block for both backends, vs WDF-integrated
   coupled-inductor with WaveNet approximated/greyed.
2. 31267 model data: real winding values (Lp/Ls, ratio, Rp/Rs, term/Zobel) vs
   parametric model from level + FR, vs research the part first.
3. Line level/gain mapping into the `0 dBFS = 0.5 V` convention.
4. Whether MIC mode stays literally untouched or later gets 10468+BA284.

---

## Key file anchors

| Concern | Location |
|---|---|
| Signal-flow comment (chain) | `plugin/src/eq.rs:3-8` |
| Pre-model EQ stage, bit-exact bypass | `plugin/src/eq.rs` |
| Input voltage convention `0dBFS=0.5V` | `plugin/src/dsp.rs` (`INPUT_FS_VOLTS`) |
| process_sample / backend split | `plugin/src/dsp.rs` (`process_sample`) |
| BA283 gain table (R_KJ steps) | `plugin/src/lib.rs` (`GAIN_STEPS`) |
| BoolParam toggle pattern | `lib.rs` (`eq_on`,`level_norm`,`gain_stepped`) + `editor.rs` `toggle_pill()` |
| WDF netlist (starts at BA283 INPUT) | `data_gen/netlist.py:1-41` |
| Output transformer JA core (reusable) | `trafo-core` feature, `wdf/` |
| Non-goals (input xfmr excluded) | `docs/plans/2026-06-01-ba283-design.md:18-23` |
