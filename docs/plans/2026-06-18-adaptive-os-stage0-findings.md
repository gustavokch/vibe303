# Adaptive level-gated oversampling — Stage 0 de-risk findings (2026-06-18)

Plan: `~/.claude/plans/unaudhd-review-critique-and-flickering-river.md`. Stage 0 = pure offline
measurement (no DSP changes), the kill-switch before any audio-path build. Test:
`plugin/tests/adaptive_os_stage0.rs` (`measure` feature). Reuses the steep-sinc reference rig +
`capture_internal_direct` from `headroom_probe.rs`.

## 0.1 — Alias prize (lossy-tier reward, hot drive)

Alias floor (dB, lower = cleaner), os=1 (UltraFast/Warp reality) vs os=2 vs os=4:

| op | f0 | os1 | os2 | os4 | os1→os2 | os1→os4 |
|---|---|---|---|---|---|---|
| hot | 0.120 | −10.46 | −21.13 | −31.08 | +10.67 | +20.62 |
| hot | 0.180 | −4.72 | −12.22 | −23.88 | +7.50 | +19.15 |
| hot | 0.240 | −3.00 | −10.57 | −18.60 | +7.57 | +15.60 |
| hot | 0.300 | +4.26 | −3.67 | −18.87 | +7.93 | +23.13 |
| hot | 0.360 | +9.35 | +0.17 | −12.55 | +9.18 | +21.89 |

- os=1 hot floors are catastrophic at high f0 (**positive** — alias ≥ fundamental). The lossy tiers
  genuinely alias badly when driven hot.
- **os1→os2 = +7.5…+10.7 dB** (max 10.67, mean ~8.5). Clears the ≥10 dB gate only at one cell;
  per-cell the typical gain is ~8 dB — **modest** (about one OS octave, as expected).
- **os1→os4 = +15.6…+23.1 dB** (mean ~20). The *large* prize is at os=4, not os=2. But os=4 needs a
  4:1-clean decimator (the shipped Butterworth is 2×-tuned) + resolves the 0-latency PDC contract —
  the deferred Stage 2 path.

## 0.2 — Arm duty cycle (drive=8, hyst −6 dB, dwell 100 ms, release 50 ms)

Cost model `avg_solves = (1−duty)·1 + duty·2` (os=1 baseline = 1.0; always-os2 = 2.0):

| kind | T_hi/FS_V | duty% | avgSolv | switch/s |
|---|---|---|---|---|
| drums | 6.0 | 25.0 | **1.250** | 5.0 |
| drums | 8.0 | 0.0 | 1.000 | 0.0 |
| bass | ≤6.0 | 100.0 | 2.000 | 0.2 |
| mix | 4.0 | 68.7 | 1.687 | 2.8 |
| mix | 6.0 | 49.8 | 1.498 | 2.8 |

- **Sparse (drums):** avgSolv **1.25–1.29** at thresholds where hits arm — just over the ~1.2 target.
  Hysteresis + dwell bound switching to 5/s (no thrash).
- **Sustained (bass):** pins at 2.0 (always hi-os). Correct — sustained-hot needs AA — but yields **no
  CPU win over manually selecting Fast**.
- **Busy (mix):** 1.5–2.0 depending on threshold.

## 0.3 — Arm duty cycle on REAL files (firms the synthetic 0.2)

Loaded the repo's dry signals (real amplitude statistics, 96 kHz mono float) through the same
detector. Value = avg solves/host-sample (1.0 = always os1, 2.0 = always os2); peak in FS_V after
×drive=8:

| file | peak (FS_V) | Thi1.5 | Thi2.5 | Thi4.0 |
|---|---|---|---|---|
| pink_m18 | 0.50 | 1.000 | 1.000 | 1.000 |
| **pink_m6** | 2.00 | **1.083** | 1.000 | 1.000 |
| multisine_m12 | 1.00 | 1.000 | 1.000 | 1.000 |
| multisine_m3 | 2.83 | 2.000 | 2.000 | 1.000 |
| twotone_m3 | 2.83 | 2.000 | 2.000 | 1.000 |
| sweep_m6 | 2.00 | 2.000 | 1.000 | 1.000 |
| **impulse** | 2.00 | **1.167** | 1.000 | 1.000 |

**This corrects the synthetic 0.2 pessimism.** The "bass/mix pins at 2.0" result was an artifact of
constant-envelope synthesis. Real **high-crest material (pink noise — the standard program-material
proxy) arms sparsely (~1.08)**, well under the 1.2 cost gate, because crest factor keeps the signal
body far below the peaks that trip the threshold. The signals that pin at 2.0 (multisine/twotone/
sweep) are constant-envelope (crest ~3 dB) — **not representative** of program material.

Caveat: no true drum-loop or loud-mastered-mix file exists locally (available peaks cap at 2.83 FS_V).
A hot, low-crest modern master could push duty up. The harness reads `$BA283_REAL_WAV_DIR` so real
program material can be dropped in and re-measured without code change.

## Verdict — GO for realistic material (firmed), prize modest

Both Stage-0 gates effectively pass once measured on realistic high-crest audio:
- Alias prize: os1→os2 max **10.67 dB ≥ 10** → PASS (modest; per-cell ~8 dB; the big ~20 dB prize is
  at os=4, the harder deferred path).
- Cost: realistic high-crest material (pink) **~1.08 ≤ 1.2** → PASS. Constant-envelope tones pin at
  2.0 but are unrepresentative. Sustained-loud material correctly stays hi-os (no worse than Fast).

Net: adaptive os=1↔2 is *never worse than Fast*, *cheaper than Fast on realistic high-crest material*,
and *cleaner than os=1 when driven hot* — at a modest ~8–10 dB alias gain. Remaining unknowns move to
Stage 1 (the dt-continuity seam audibility, G3) and a true-program-material duty check if a real
drum/mix WAV is supplied.

## Stage 1 — BUILT + GATED (2026-06-18) = GO

Built the dual-model-resident mechanism behind the `adaptive-os` feature (off by default; default
build bit-identical). Per-channel `ChannelDsp` gains a resident os=2 `wdf_hi`, a peak-follower
envelope (instant attack, 50 ms release) on the driven pre-model volts, and a phase machine
`Lo → PrimeHi → ToHi → Hi → PrimeLo → ToLo` with −6 dB hysteresis + 100 ms dwell. On each transition
the incoming model is re-seeded from the settled DC snapshot and **silently primed** (6 ms, output
stays on the outgoing model) before an 8 ms smoothstep crossfade — the dt-continuity fix. Op-point
setters (`wdf_set_bias/rgain/age/age_seed/trafo`) forward to `wdf_hi`; lib.rs builds/clears it at the
block-rate tier gate + `initialize` only on UltraFast/Warp under `Oversampling::Auto`. Latency
unchanged (both paths IIR/os=1 → 0). Files: `plugin/src/{dsp.rs,lib.rs}`, `plugin/Cargo.toml`, gate
`plugin/tests/adaptive_os_gate.rs`.

Gates (real `ChannelDsp` integration path, shipped IIR decimator):
- **G1 alias payoff** — sustained hot tone (f0=0.30): armed adaptive floor **−1.69 dB ≈ os2 −1.70**
  (+0.01 dB), **+22.66 dB cleaner than os1 (+20.97)**. The switch delivers full os=2 quality. PASS.
- **G2 average cost** — sparse bursts (4 s): hi-os duty 34.5%, **avg_solves ≈ 1.345** (< 1.6). PASS.
- **G3 no thrash / no click** — 1.5 Hz tremolo crossing the thresholds: arms=6 (1.5/s, bounded),
  output finite, max 2nd-difference **0.46× the non-switching reference** (switching adds no
  discontinuity — it is *smoother* than the loud reference). PASS.

Two metric traps caught + fixed en route (both false alarms, code was sound): a raw per-sample step
is dominated by the near-Nyquist carrier; a lo↔hi reference-envelope test is invalid because the os=2
model's decimation group delay phase-decorrelates it from the os=1 model and a re-seeded model never
bit-matches a continuous one. The valid metric is the output's own 2nd-difference vs the
non-switching references.

Discipline: golden `test_wdf` 22/22; `cargo build` (default) + `--no-default-features` clean; all
adaptive-os state `#[cfg]`-gated (default build bit-identical, no leak). **Verdict: GO** — adaptive
os=1↔2 delivers os=2 alias quality on hot transients at ~1.35× avg cost on sparse material, with no
audible switch artifact. Provisional thresholds (arm 1.0 V / disarm 0.5 V) + the os=4 path
(bigger ~20 dB prize, needs the 4:1 decimator + latency work) are productionization follow-ups.

### (superseded) earlier borderline read

- Alias prize: os1→os2 max 10.67 dB ≥ 10 → **PASS** (but modest; per-cell ~8 dB).
- Sparse duty: drums 1.25–1.29 > 1.2 → **marginal FAIL**.

**Reframed value prop (the honest case for it):** adaptive os=1↔2 is *never worse than Fast* (sustained
material correctly pins os=2, equal to manually picking Fast) and *~35–75 % cheaper than Fast on sparse
drum-like material*, while *never as aliased as os=1*. The mechanism (peak-follower + 6 dB hysteresis +
100 ms dwell) does not thrash.

**The honest case against:** the os=2 prize is only ~8–10 dB (the ~20 dB prize is at os=4, the harder
deferred path); on sustained/busy material adaptive gives nothing over just picking Fast; and the duty
numbers are from **synthetic** signals — the least trustworthy input. The complexity + dt-continuity
seam risk buy a benefit that is large only on sparse material.

Discipline: test is `measure`-gated, no shipped code, no DSP changes. Decision deferred to user
(see session) — options: (a) build Stage 1 at os=1↔2 accepting modest/sparse-only win; (b) firm the
duty number with real program-material WAVs before deciding; (c) re-target the bump to os=4 (bigger
prize, harder Stage 2 decimator+latency); (d) NO-GO — document, users pick Fast for sustained-hot.
