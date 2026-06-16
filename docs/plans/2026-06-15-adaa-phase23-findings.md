# ADAA Phases 2 & 3 — Findings (both NO-GO, measured)

Date: 2026-06-15
Branch: `feat/adaa-phase23` (off the Phase-0/1 PR branch `feat/adaa-antialiasing`, PR #56)
Follows: `docs/plans/2026-06-15-adaa-antialiasing-design.md`

## Method

A hard-driven sine (`f0 = 0.227·host`, host = 48 kHz) was pushed through the WDF backend and the
inharmonic alias content measured with a Hann-windowed single-bin DFT at the folded-alias bins
`0.092` and `0.135` (the 4th/5th-harmonic foldback frequencies, clear of the fundamental and its
sub-Nyquist harmonics). Metric = `alias / fundamental` in dB. Two operating points: moderate
(drive +12 dB·×4 gain, amp 0.5) and hot (drive ×12, amp 0.9). Throwaway measurement bins, deleted.

## Baseline alias floor (WDF Realtime, os=2, 4th-order IIR decimation)

| operating point | alias/fund |
|---|---|
| moderate | **−22.2 dB** |
| hot | **−10.5 dB** |

For reference, Warp (os=1, no decimation) measured ~−2 dB / +1 dB — i.e. the os=2 + decimation path
already buys ~20 dB over no oversampling.

## Phase 2 — steeper decimation filter: NO-GO

Replaced the 4th-order Butterworth decimation cascade (2 biquads) with an **8th-order** cascade
(4 biquads, standard Butterworth stage Q's), same 0.45·host cutoff, zero added latency.

**Result: identical to four significant figures** (−22.2 / −10.5 dB). The decimation filter is *not*
the limiter. The residual aliasing is **in-solve harmonic foldback**: the BJT nonlinearity, solved at
only 2× host, generates harmonics above the *internal* Nyquist (48 kHz) that fold *below* host-Nyquist
*before* decimation. A decimation low-pass can only remove the image band (24–48 kHz); it cannot
remove energy already folded into the audio band. The 4th-order cascade already clears the image band
adequately.

Levers that *would* lower this floor — raising the internal OS factor to 4 (rejected: doubles the
per-tier CPU, the whole point of the tier system) or ADAA on the circuit itself (impossible: the WDF
is a high-order coupled stateful solve, not a memoryless map — see Phase 0) — are out of scope.
Reverted; no change shipped.

## Phase 3 — ADAA the JA transformer (Langevin) saturation: NO-GO

Measured the transformer core's *contribution* to the alias floor before implementing ADAA: WDF
Realtime, transformer OFF vs ON with the SATURATION dial maxed (knee pulled hardest into the signal).

| operating point | trafo OFF | trafo ON (sat=1.0) | delta |
|---|---|---|---|
| moderate | −22.2 dB | −22.2 dB | **−0.0 dB** |
| hot | −10.5 dB | −10.6 dB | **−0.0 dB** |

**The transformer core adds no measurable aliasing.** It is integrated by RK4 (already band-limiting),
its gapped-LO1166 voicing gives a gentle saturation knee, and the magnetizing deviation is injected
*decoupled and one-step-lagged* outside the Newton loop. ADAA-ing the Langevin curve would antialias a
stage that contributes ~0 dB of aliasing — pointless. Not implemented.

## Verdict

The Phase-1 **ADAA2 output-saturation stage (PR #56) is the one place ADAA cleanly helps** this
plugin: a genuinely memoryless stage, antialiased by construction. The two stateful targets here are
either filter-insensitive (Phase 2) or alias-negligible (Phase 3). Both documented as NO-GO so the
measurement isn't re-run from scratch later.
