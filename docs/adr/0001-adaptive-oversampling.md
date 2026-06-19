# ADR-0001: Level-Gated Adaptive Oversampling

## Status

Accepted — productionized 2026-06-18 (`feat/adaptive-os-prod`).

The `adaptive-os` cargo feature is part of the **default** feature set
(`default = ["trafo-core", "adaptive-os"]`), so stock builds compile the
mechanism in. The runtime **`Adaptive OS` toggle ships off** — the `BoolParam`
default is `false` (`plugin/src/lib.rs:683`), so the user opts in. Building with
`--no-default-features` drops the feature entirely, yielding a bit-exact
non-adaptive A/B reference (all adaptive state is `#[cfg(feature = "adaptive-os")]`
gated; the default-features build with the toggle off is bit-identical to it).

## Context

The WDF circuit backend exposes four speed tiers. The two lossy tiers
(`UltraFast`, `Warp`) run the solver at **`os=1`** (no internal oversampling) to
hit their CPU budget. Measured against a steep-sinc reference, `os=1` aliases
badly under hot drive — the alias floor climbs from ≤−40 dB on quiet material to
**positive** (alias ≥ fundamental) by ~1.0 V input
(`docs/plans/2026-06-18-adaptive-os-stage0-findings.md`, §0.4):

| in_v peak (V) | os=1 worst alias | os=2 worst |
|---|---|---|
| 0.25 | −40.6 dB | −50.3 dB |
| 0.50 | −15.3 dB | −44.4 dB |
| 1.00 | +3.5 dB | −23.4 dB |

The floor falls off a cliff at **~0.4 V**: clean below it, catastrophic above.

The naive fix — always oversample (static `os=2` or `os=4`) — pays the CPU cost
continuously, including on the quiet/moderate material that is *already clean at
`os=1`* and buys nothing from oversampling. On realistic high-crest program
material (pink noise) the body of the signal sits far below the peaks, so a
static rate wastes most of its extra solves.

## Decision

Keep the lossy tier at `os=1` by default and **engage a higher-rate model only on
hot transients**, gated by signal level.

Mechanism (`plugin/src/dsp.rs:34-73`, `plugin/src/lib.rs`):

- **Envelope detector.** A peak follower (instant attack, 50 ms exponential
  release, `OS_REL_S`) runs on the *driven, pre-model* input volts.
- **Dual resident models.** Each `ChannelDsp` keeps the `os=1` base model plus a
  resident higher-rate sibling `wdf_hi` built at **`os=4`** (`ADAPTIVE_HI_OS = 4`,
  `plugin/src/lib.rs:44`). Op-point setters (`wdf_set_bias/rgain/age/age_seed/
  trafo`) forward to both so the sibling stays in sync.
- **Arm / disarm with hysteresis + dwell.** Arm when the envelope exceeds
  `OS_ARM_V = 0.4 V` (just below the audibility knee); disarm below
  `OS_DISARM_V = 0.2 V` (−6 dB hysteresis), and only after a `OS_DWELL_S = 100 ms`
  minimum dwell. This bounds the switch rate (no thrash).
- **Seamless handoff.** On a transition the incoming model is re-seeded from the
  settled DC snapshot, then **silently primed** for `OS_PRIME_S = 6 ms` (solved on
  the live input while the outgoing model still drives the output) so its biquads
  and operating point catch up. Only then does an `OS_XFADE_S = 8 ms` smoothstep
  crossfade run. The phase machine is `Lo → PrimeHi → ToHi → Hi → PrimeLo → ToLo`
  (`OsPhase`, `plugin/src/dsp.rs:64-73`). Priming is the fix for the
  dt-continuity seam: re-seeding from DC mid-swing without it injects a large
  settling transient.
- **Inert unless applicable.** The block-rate gate builds/clears `wdf_hi` only when
  the toggle is on **and** tier ∈ {`UltraFast`, `Warp`} **and** `Oversampling =
  Auto`. Toggle off ⇒ `wdf_clear_hi` ⇒ the adaptive solve returns exactly
  `wdf.process_host_sample` ⇒ bit-identical to the non-adaptive build.

## Rationale

- **Right cost where it matters.** Quiet/moderate material (`in_v < 0.4 V`) is
  already clean at `os=1`, never arms, and keeps the full CPU savings; hot
  transients get the cleanest alias floor exactly when it is most audible.
- **`os=4`, not `os=2`.** The large alias prize is at `os=4` (+15.6…+23.1 dB vs
  `os=1`), not `os=2` (+7.5…+10.7 dB, ~8 dB typical). `os=4` is admissible at
  **zero added latency** because the shipped 4th-order Butterworth decimator is
  already within **+0.57 dB** of the steep-sinc ideal at 4:1 (worst case, top
  octave) — the alias floor is dominated by in-band BJT transport foldback the
  decimator never has to touch (findings §A0). `WdfModel::new_with` retunes the
  existing biquad decimator at `fs = host*os`, so the `os=4` model needs **no
  decimator/`build.rs`/`process` change** — only `cfg.os = 4, cfg.decimate = true`.

## Alternatives considered

- **In-solve ADAA** (antiderivative anti-aliasing inside the Newton solve, per-term
  or per-residual). Rejected: destroys Newton convergence — avgIters explode from
  ~9 to 100–200 — and produces worse aliasing. The only remaining variant
  (per-port Albertini) is out of scope for the BA283 nodal MNA (not a WDF tree).
  See [[ba283-insolve-adaa]].
- **Static `os=4`.** Delivers the same alias prize but pays ~4× the solves
  continuously. Adaptive pays only on transients: **avg 2.38 solves vs 4.0 static
  = ~41% saved** (G2 below).
- **Off-thread `wdf_hi` build.** Investigated and declined (findings §C). The
  resident sibling allocs at the same user-gesture moments as `wdf_set_tier`
  (tier/OS/toggle change), which already allocs on the audio thread and ships
  accepted; moving only `wdf_hi` off-thread adds an RT-safe-drop mechanism for
  near-zero real gain. Revisit only if the whole tier rebuild moves off-thread.
- **2nd-difference / envelope click metrics.** Two seam-detection metrics were
  tried and found invalid (a raw per-sample step is dominated by the near-Nyquist
  carrier; a lo↔hi reference-envelope test is decorrelated by the `os` model's
  decimation group delay). The valid gate is the output's own 2nd-difference vs the
  non-switching reference.

## Consequences

- **Default behavior unchanged.** Toggle ships off (opt-in), so a plugin update
  never silently changes a project's sound or CPU. Off ⇒ bit-identical to the
  non-adaptive build.
- **Latency unchanged.** Both paths decimate via IIR (no per-transient FIR group
  delay), so integer latency stays ~0 and the 0-PDC contract holds.
- **Validated gains** (real `ChannelDsp` path, shipped IIR decimator, `HI_OS = 4`;
  findings §A2):
  - **G1 alias payoff:** **+32.2 dB** cleaner than `os=1` on a sustained hot tone.
  - **G2 average cost:** duty 46%, **avg 2.38 solves** vs static `os=4` 4.0 → ~41%
    saved.
  - **G3 no thrash / no click:** arms bounded to 1.5/s, output finite, max
    2nd-difference **0.86×** the non-switching reference (under the 2.0× click bar;
    the 1↔4 dt jump raised the seam from 0.26× at `os=2` as predicted, still no
    click, no extra prime needed).
  - Golden `test_wdf` 22/22; `cargo build` (default) and `--no-default-features`
    both clean.

## References

- Design + gate findings: `docs/plans/2026-06-18-adaptive-os-stage0-findings.md`
- Implementation: `plugin/src/dsp.rs` (envelope, `OsPhase` machine, thresholds),
  `plugin/src/lib.rs` (`adaptive_os` `BoolParam`, `ADAPTIVE_HI_OS`, block-rate gate)
- Gate tests: `plugin/tests/adaptive_os_gate.rs` (G1/G2/G3),
  `plugin/tests/adaptive_os_stage0.rs` (`measure` feature: alias prize, duty,
  threshold calibration, 4:1 decimator)
- Related: [[ba283-insolve-adaa]] (rejected in-solve ADAA), the WDF quality tiers,
  the 0-latency oversampling/decimator contract
