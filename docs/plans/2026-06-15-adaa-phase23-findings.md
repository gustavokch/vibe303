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

## Broadband sweep confirmation (2026-06-16)

The above used a single tone at two folded bins. To rule out a methodology blind-spot, a **broadband**
alias metric was run over a **frequency sweep**: energy in the sub-fundamental band `(0, f0)` — where a
single tone has *no* legitimate content, so all of it is folded aliasing — relative to the
fundamental, with coherent (`f0 = p/N`) rectangular FFTs. Grid = 7 f0 (`0.12…0.36·host`) × 2 operating
points × {trafo OFF, ON} × {4th-order, 8th-order, raw os=2}. The WDF model is driven directly
(saturation stage out of path). Reproducible, feature-gated harness:
`plugin/tests/alias_sweep.rs` + `WdfModel::capture_os2_raw` (both behind the `measure` Cargo feature).
Run: `cargo test --features measure --release --test alias_sweep -- --nocapture`.

Alias/fundamental (dB); `4th` = shipped path (verified bit-identical to live `process_host_sample`
across every cell), `8th` = 4-biquad Butterworth, `raw2x` = pre-decimation os=2 stream:

| trafo | op | f0 | 4th | 8th | raw2x | Δ(8−4) |
|---|---|---|---|---|---|---|
| off | moderate | 0.160 | −38.87 | −38.87 | −38.87 | +0.00 |
| off | moderate | 0.240 | −19.61 | −19.61 | −19.61 | −0.01 |
| off | moderate | 0.360 | −2.24 | −2.46 | −2.49 | −0.23 |
| off | hot | 0.200 | −18.42 | −18.42 | −18.42 | −0.00 |
| off | hot | 0.280 | −0.32 | −0.35 | −0.35 | −0.03 |
| off | hot | 0.360 | +6.31 | +6.04 | +6.00 | −0.27 |
| on | hot | 0.200 | −23.59 | −23.59 | −23.59 | −0.00 |

(full 28-cell table printed by the harness)

**Phase 2 — confirmed, broadly.** `|8th − 4th| ≤ 0.27 dB` across **all 28 cells** (largest at the
top of the band, where the image is closest); typically ≈ 0. And **`raw2x ≈ 4th` everywhere** — the
alias energy is already in-band *before* decimation, so a steeper low-pass has nothing left to remove.
Direct confirmation of the in-solve-foldback mechanism. Steeper decimation remains a NO-GO.

**Phase 3 — confirmed, and sharpened.** The single-point 0.227 reading of "−0.0 dB" sat near a null.
The sweep shows the transformer's contribution is **non-uniform but one-sided: ON is always ≤ OFF**
(delta `0 → −5.17 dB`; e.g. hot f0=0.200, OFF −18.42 → ON −23.59). The JA core (RK4-band-limited,
one-step-lag-injected) **never adds aliasing** — at some f0 its gentle compression *reduces* the BJT
foldback. ADAA-ing a stage that adds no aliasing is pointless: NO-GO stands, now on broadband evidence
rather than one operating point.

**Net:** both NO-GO verdicts hold across the sweep; the harness is retained (off-by-default `measure`
feature) as a regression guard. Default `cargo test` and `--no-default-features` are unaffected.
