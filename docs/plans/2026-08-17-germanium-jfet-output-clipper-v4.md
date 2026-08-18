# Add Global MNA Germanium JFET Output Clipper (v4 — staged solver)

## Summary

Add the WARMCLIPPER circuit as a bypassable global output-color stage. It
defaults enabled, is active in both Accurate and Fun topology, and processes
the voice after tier rendering, VCA envelope, and resonance compensation, but
before the master-volume fader.

The implementation preserves the supplied circuit's topology and uses a fixed
96-sample plugin latency: the existing 48-sample tier compensation plus a
48-sample output-clipper path.

**Source of truth:** `warm_clipper.cir` (ngspice-validated). The schematic PDF
is the human reference; use the current rev — rev A had a drafting bug at the
U4 summing node (Rmix2 routing), fixed in current files. The netlist was never
affected.

## Circuit contract (validated behavior to reproduce)

±12 V rails. U1 buffer → U2 drive amp (gain = 1 + R_DRIVE/4.7k) → 2N5457 shunt
VCA (Rs/Rpad pad, Rjd floor-set, Rjs degeneration) → U3 inverting stage with
2× series 1N34A per direction in feedback (+ Rf3, C_TONE) → U4 summer with
make-up gain. Bass leg: 2-pole LP (~106 Hz) from buffer → U6 polarity-
correction inverter → R_BASS into summer. Sidechain: 2-pole HPF
(~234 Hz/section) → U5 gain (R_SENS) → 1N4148 rectifier → envelope (Cenv/Renv)
→ attack diode D4 + Ratk → JFET gate (Cg; release Rg1). GB rail ≈ −3.8 V from
Rgb1/Rgb2 + Cgb.

- Soft drive → VCA rides gain: characterful limiter (~1 ms attack, ~220 ms
  program-leaning release).
- Hard drive → VCA bottoms out (~−21 dB floor), Ge stack clips (22–37 % THD,
  3rd-dominant, 2nd ≈ −30 dBc).
- Bass never ducks: sidechain HPF + clean blend leg.

## Solver architecture: stage-by-stage (mandatory, not conditional)

The circuit is **strictly feedforward**: the sidechain taps `drv`, and no node
feeds back into upstream audio. Exploit this unconditionally — do **not** build
one global Newton solve. Four small solvers per (oversampled) sample, each with
its own warm start, damped Newton, iteration cap, and finite-output fallback:

| Stage | Contents | Unknowns (≈) | Driven by |
|-------|----------|--------------|-----------|
| A — front-end | U1 buffer, U2 drive amp (saturating) | 4–6 | input sample |
| B — sidechain | HPF sc0/sc1, U5 (saturating), D3/Rd3, envelope Cenv/Renv, D4/Ratk, gate Cg/Rg1, GB rail | 10–12 | `drv` (from A, this sample) |
| C — VCA + clipper | Rs/Rpad, J1 channel + gate junctions, Rjd/Rjs, R4, U3 + Rf3/Ct/D1a/D1b/D2a/D2b | 8–10 | `drv` (A), `gate` (B) |
| D — blend + summer | Rb1/Cb1/Rb2/Cb2, U6, Rmix1/Rmix2, U4 (saturating) | 6–8 | `buf` (A), `clp` (C) |

Each stage is a small dense solve (~n³/3 with n ≤ 12 → ~600 flops per
factorization, 2–4 warm-started iterations), roughly an order of magnitude
cheaper than one ~35-unknown global Newton at the same oversampling rate, and
far easier to converge.

**Coupling policy (the one approximation):** J1's gate junctions couple the
gate node (stage B) to the VCA drain/source nodes (stage C) when the junction
conducts in deep limiting. Solve stage B using previous-sample `vjd`/`vjs` for
the gate-junction terms, then stage C with the fresh gate voltage. Optionally
run **one Gauss-Seidel sweep** (B→C→B) if the junction-conduction region shows
artifacts in the oracle tests; expected unnecessary because Ratk = 10 kΩ
dominates the junction impedance there. Document whichever choice passes the
tests.

All other stage interfaces are exact same-sample handoffs (A→B `drv`, A→C
`drv`, A→D `buf`, B→C `gate`, C→D `clp`).

**Reference solver (dev-only, optional):** keep a global Newton assembly behind
a test-only feature flag to cross-check the staged solver against the oracle
tables during development; never ship it in the audio path.

## Device models

- `Diode1N34A`: IS=0.5 µA, RS=8, N=1.6, BV=75, IBV=4 µA, CJO=4 p, VJ=0.75,
  M=0.33, TT=5 n, EG=0.67. Keep the leakage terms — they shape the always-on
  warmth; do not "clean up".
- `Diode1N4148`: IS=2.52 n, RS=0.568, N=1.752, BV=100, CJO=4 p, M=0.4,
  TT=20 n, VJ=0.75 — control path only.
- `Jfet::j2n5457()`: VTO=−1.8, BETA=1.2 m, LAMBDA=2.25 m, RD=40, RS=40,
  CGS=4.5 p, CGD=1.5 p, PB=1, IS=34 f. Channel law continuous through Vgs=VTO
  and Vds=0; soft-exponential gate junctions.

### Op-amp stamp (new, reusable) — MUST saturate
A linear VCVS is not acceptable: at hard drive U2 rails at ±12 V, and that rail
bounds the sidechain envelope amplitude and therefore the release time. A
linear model lets the envelope charge to tens of volts and the limiter gets
slower with more overdrive — audibly wrong. Implement the validated anti-windup
structure: 100 dB open loop into a ~10 Hz dominant pole (one cap state), then a
smooth clamp (tanh knee) of the integrator node to ±12.6 V, plus 47 Ω series
output resistance. Used by every stage that contains an op-amp (A: U1, U2;
B: U5; C: U3; D: U6, U4).

### Topology fidelity warnings
- Rhp0/Rhp, Renv, Rg1 and U5's non-inverting input are referenced to **GB
  (−3.8 V), not ground**. Easy to "simplify" during implementation; doing so
  silently breaks the rest state and control law.
- Cap states are trapezoidal companions. Watch trapezoidal ringing on the
  diode-driven `Cenv`/`Cg` (hard diode edges; Rd3/Ratk damp most of it):
  verify with a step test; if ringing shows, use BDF2 for just the sidechain
  caps.

## Level contract (loudness window, not quiet-unity)

A single quiet-level unity calibration is wrong for a default-on compressor:
with trim ≈ −17.7 dB (1/7.7), nominal 0.5–1.0 peak program lands in the
limiting plateau and the enabled path runs 12–16 dB quieter than bypass.
Instead:
- `g_in`: fixed input trim; start with 1.0 (internal peak) → **2 Vpeak** at the
  clipper input. Tune so the voice's ~p95 peaks sit at 1.5–2× the limiting
  threshold.
- `g_out`: fixed output trim; start at **−12 dB**.
- Acceptance: with stock knobs, enabled-vs-bypassed loudness within ≈ ±6 dB
  across quiet-to-nominal material (quiet passages somewhat hotter, hot peaks
  somewhat softer — that is the compressor working). Verify by measurement and
  ear; adjust g_in/g_out, not the circuit.
- Ensure the voice output is DC-free at the clipper input; otherwise add a
  ~5 Hz one-pole HPF there (the Ge stack rectifies DC into even-order junk).

## Parameters

- `clipper_bypass`: default false.
- `clipper_os`: 2x | 4x, default 2x.
- `clipper_drive` → R_DRIVE 22k..100k, default 47k.
- `clipper_threshold` → R_SENS 2.2k..22k, default 4.7k (higher = earlier,
  deeper gain reduction).
- `clipper_bass` → R_BASS 4.7k..22k, default 8.2k (lower = more clean bass).
- `clipper_tone` → C_TONE 470p..4.7n, default 1n (higher = darker clipped path).
- Knob→matrix application at **block rate (16–32 samples)**, re-stamp/re-factor
  the affected stages on change; do not re-factor per sample for
  nih_plug-smoothed params. (R_DRIVE touches stage A; R_SENS stage B; C_TONE
  stage C; R_BASS stage D — the staged solver makes each knob cheaper too.)
- Optional cheap voice: `clipper_asym` removes D2b (stronger even harmonics).
  Implement as a large series R in that branch so stage structure is constant.

## Oversampling, latency, transitions

- Preallocate complete 2x and 4x engines. Pad bypass and 2x paths to the 4x
  clipper's 48-sample group delay; retain the existing 48-sample tier
  compensation; report a constant 96-sample plugin latency. (Verify the 48 with
  a measurement test against the actual OS filter chain rather than assuming.)
- Fixed 10 ms equal-power crossfades for bypass and OS-mode changes. On OS
  change, initialize the incoming engine from a bounded rest/last-input state,
  run both paths only during the fade, then only the selected engine. A
  bypassed engine is not solved; on un-bypass, re-prime all stages via gmin OP
  solve, then fade in.

## Voice placement (unchanged)

In `Voice::tick()`: render tier/crossfade → VCA envelope + resonance
compensation → aligned clipper or aligned bypass → master volume fader last.

## UI

Always-active `OUTPUT CLIPPER` strip (BYPASS, OS, DRIVE, THRESH, BASS, TONE)
outside the Accurate-disabled Souped-up panel; extend the root GUI layout as
necessary.

## Tests — with numeric oracles from ngspice validation (±10 % unless noted)

- **DC rest state:** v(gate) = v(env) = v(gb) = −3.826 V; v(out) ≈ 0; all
  audio-path nodes ≈ 0.
- **Static/THD ladder @ 1 kHz** (run through the staged solver end-to-end):

  | In (V pk) | Out RMS | THD   |
  |-----------|---------|-------|
  | 0.02      | 0.107   | 0.6 % |
  | 0.05      | 0.246   | 3.2 % |
  | 0.20      | 0.576   | 15 %  |
  | 1.00      | 0.557   | 15 %  |
  | 2.00      | 0.640   | 28 %  |
  | 4.00      | 0.664   | 37 %  |

- **Bass preservation @ 0.5 V pk:** GR < 0.5 dB at 40–80 Hz, ≈ 3 dB at 500 Hz,
  ≈ 8 dB at 1 kHz (output rel. 40 Hz).
- **Two-tone (55 Hz + 1 kHz, 1 V pk each):** 55 Hz fundamental ducks ≤ 6 dB;
  1 kHz ducks ≤ 3 dB; 55 Hz 3rd harmonic improves ≈ −15 dBc (alone) →
  ≈ −44 dBc (two-tone).
- **Dynamics:** attack ≈ 1 ms; release ≈ 220 ms; **release time must not grow
  with overdrive depth** — fails if the op-amp clamp is missing/ineffective.
- **Coupling-policy check:** deep-limiting A/B — previous-sample junction
  coupling vs. one Gauss-Seidel sweep — must be within tolerance on the
  1 kHz ladder at 2 V and 4 V input; keep the simpler policy that passes.
- **Robustness:** bounded output under hot drive; finite output with NaN/Inf-
  injecting input (sanitize at stage input); denormal safety (FTZ/DAZ or offset
  trick — the 220 ms envelope decays will denormalize at 96/192 kHz engines);
  sample-rate independence at 44.1/48/96 kHz with fixed attack/release.
- **Knob behavior:** Drive increases clipping; Threshold increases gain
  reduction; Bass preserves low-frequency RMS under limiting; higher Tone
  capacitance reduces clipped-path HF energy.
- **Bypass/latency:** bypass dry equivalence after 48-sample clipper alignment;
  bypass, 2x, and 4x all report the same 96-sample plugin latency through tier
  changes; master volume changes only final gain, not clipper drive or
  sidechain state.
- **No-allocation:** enabled processing, bypass transition, 2x ↔ 4x transition.
- **Release cost bench:** bypass / 2x / 4x in the existing `ns/sample` and
  `% of one core @ 48 kHz` style. The staged solver ships regardless of the
  result; the bench exists to document headroom and to gate the *default*
  (4x default only if within 10 % of bypass on the worst-case voice path).

**Targeted verification:** `cargo test -p plugin303 --release --test no_alloc
--test topology` plus the new clipper tests and cost bench.

## Assumptions
- Circuit fidelity takes priority over a lightweight waveshaper approximation.
- The solver is stage-by-stage by design; the feedforward topology makes it
  exact except for the documented gate-junction coupling policy.
- The clipper is enabled by default and calibrated to a loudness window, not
  quiet-level unity.
- 4x remains available even while 2x is the default.
- The master-volume fader must not change clipper drive, sidechain behavior,
  or clipping tone.
- The GB-referenced sidechain topology is intentional and load-bearing.
