# BA283 FiLM+GRU — Fidelity Ablation Matrix (Design)

**Date:** 2026-06-06
**Status:** approved design, pending implementation plan
**Driver:** lower `val_esr` from ~0.097 (−10 dB) toward −20 dB+. Full dataset
re-gen is permitted.

## Problem

The last FiLM+GRU run plateaus at `val_esr ≈ 0.097` (−10.3 dB). Good neural-VA
amp models reach −20 to −30 dB. We have **one** data point and **five** plausible
causes — capacity, aliasing, gain-grid sparsity, train/val domain gap, and SPICE
target-quality floor. Committing a multi-hour full re-gen on a *guess* risks
burning the batch and still missing the wall.

**Approach ③: diagnose before redesign.** Run a small one-factor-at-a-time (OFAT)
ablation to localize the bottleneck, then commit only the axes that move the
needle into a single principled redesign (separate plan).

## Two findings that shape the design

1. **Gain grid is sparse** — 5 points (0, .25, .5, .75, 1.0) across a 19 dB span
   (`data_gen/run_spice.py:332`).
2. **Held-out gains are never evaluated.** Current val = held-out *stems at the
   same 5 trained gains*. FiLM's interpolation across the span — the whole point
   of conditioning — is unmeasured. So `val_esr=0.097` says nothing about gain
   generalization. **Every ablation cell adds a held-out-gain eval** to close this.

## Venue (hybrid) and baseline discipline

The real training venue is Colab GPU; `ba283_spice_gain` lives on gdrive. Local
`data/` is stale bias-era. ngspice re-gen is local → upload → train.

- Train-only cells (no re-gen) run **local CPU** on a tiny subset for fast ranking.
- Re-gen cells run on **Colab GPU** after subset upload.
- **CPU and GPU absolute ESR are not comparable.** Each cell is compared only
  against a baseline run in the *same* venue: `B0_local` for C1, `B0_colab` for
  C4/C5. Reproducing `B0 ≈ 0.097` is the gate before any delta is trusted.

## Round 1 — Core 4 (highest-prior suspects)

| cell | axis | change vs B0 | venue | re-gen | suspect |
|---|---|---|---|---|---|
| **B0_local** | — | current config | local | no | reference for C1 |
| **C1** | capacity-width | hidden 32 → **64** GRU | local | no | capacity ceiling |
| **B0_colab** | — | current config | Colab | no | reference for C4/C5 |
| **C4** | SR / aliasing | render + train **88.2 kHz** | Colab | yes (subset) | folded-harmonic aliasing |
| **C5** | gain density | **9-pt** grid + held-out-gain eval | Colab | yes (subset) | FiLM interpolation |
| **C7** | target floor | re-verify subset wet vs **ACME/WDF** — *not a training run* | local | no | SPICE target quality |

Round 1 deliberately defers C2 (LSTM), C3 (α 0.95), C6 (domain mix) to round 2 —
they are cheap to add but lower-prior. Escalate to them only if Core-4 fails to
explain the gap.

## Controls

- Fixed seed `0xBA283`; fixed epoch budget + early-stop (e.g. 60 epochs,
  patience 15); identical val stems; identical train-subset size across a venue.
- **Subset:** all gains × 3–4 representative stems spanning stimulus types
  (1 music + 1 pink-burst + 1 sweep + 1 multisine), trimmed short. Keeps each run
  under ~45 min.
- Single seed for ranking; **re-run the winning axis on 2–3 seeds** before
  committing, to separate signal from seed noise.

## Eval protocol

- **Primary:** `val_esr` at 44.1 kHz on held-out stems at trained gains. For C4
  (88.2 kHz), downsample the model output to 44.1 kHz with a matched anti-alias
  filter *before* scoring → apples-to-apples ESR.
- **Per-gain `val_esr`** — catches a single gain dragging the mean.
- **Held-out-gain `val_esr`** — eval at intermediate gains (.125/.375/.625/.875).
  Only meaningful where intermediate renders exist (native in C5's 9-pt grid; for
  B0/C1's 5-pt grid this is directional only).
- **C4 aliasing metric:** render an 8×-oversampled gold reference (352.8 kHz)
  AA-downsampled to 44.1 kHz; report high-band (> 0.5·Nyquist) spectral error /
  THD for B0 vs C4. Lower for C4 ⇒ aliasing was real.
- **C7:** `ESR(SPICE wet vs ACME)` and `ESR(SPICE wet vs WDF)` on the subset via
  the existing `eval/acme_vs_spice/` and `eval/wdf_vs_spice/` harnesses. This is
  an independent estimate of the achievable ESR **floor**.

## Decision rule

Rank ΔESR (dB) vs the venue-matched baseline. An axis "explains" the gap if it
improves `val_esr` by **≥ ~3 dB** (≈ halving ESR) or closes **≥ 50%** of the
0.097→target gap. Winning axes graduate to the full-redesign plan; the rest are
dropped (YAGNI).

**Special case — C7 first:** if the target floor (ESR of SPICE vs ACME/WDF) is
itself near 0.097, fidelity is **target-bound** — no arch/data change on the
neural side can beat a corrupted target. Then the redesign starts at data-gen
(SPICE topology / oversampling), not the model. Run/read C7 before over-investing
in C1/C4/C5.

## Risks & mitigations

| risk | mitigation |
|---|---|
| Subset too small → starves data axes (C5), masking the effect | keep all gains + stim variety; treat C5 as directional, confirm at full scale post-commit |
| Cross-venue ESR mis-compared | only compare within venue (B0_local↔C1; B0_colab↔C4/C5) |
| 88.2 kHz eval unfair to 44.1 kHz | downsample C4 output with matched AA filter before ESR |
| Seed noise mistaken for signal | re-run winner ≥ 2 seeds before committing |
| Held-out-gain eval impossible on 5-pt grid | native in C5 (9-pt); directional elsewhere |

## Deliverable

A results table (`cell, ΔESR_dB, per-gain ESR, held-out-gain ESR, notes`) plus a
one-paragraph verdict naming the wall. That verdict is the input to the full
FiLM+GRU redesign plan.

## Round 2 (conditional)

If Core-4 + C7 do not localize the wall: add C2 (hidden32 LSTM), C3 (α 0.95),
C6 (music-weighted mix), same controls/protocol.
