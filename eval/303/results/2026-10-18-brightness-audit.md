# VCF brightness vs analog wear: before/after audit (2026-10-18)

Fix: `LadderVcf::set_analog_wear` scales the tier brightness instead of replacing it
(branch `fix/ladder-brightness-wear`). Raw output: `2026-10-18-brightness-audit-before.txt`,
`2026-10-18-brightness-audit-after.txt`. Command:
`cargo test -p plugin303 --release --test brightness_audit -- --ignored --nocapture --test-threads=1`.

## Effective brightness the plugin plays (first block, age 0)

Source: section A, row "plugin defaults (seed 1337, age 0)".

| Tier | Intended | Before | After |
|---|---|---|---|
| Low | 0.50 | 1.0000 | 0.5000 |
| High | 0.70 | 1.0000 | 0.7000 |
| Best | 0.70 | 1.0000 | 0.7000 |

## Spectral match vs SPICE gold (voice state row, section B)

The "voice state" row changes brightness only, so it is the row compared here. The "plugin defaults" row is the full default patch (analog wear and all other default params); compare it with the "plugin defaults, cal <intended>" control row, not with the gold directly.

| Tier | LSD before | LSD after | corr before | corr after | RMS before | RMS after |
|---|---|---|---|---|---|---|
| Low | 28.62 | 28.62 | 0.978 | 0.978 | -26.9 | -26.9 |
| High | 9.34 | 3.07 | 0.984 | 0.988 | -26.6 | -25.0 |
| Best | 9.59 | 3.15 | 0.984 | 0.988 | -26.6 | -25.0 |

## Low vs Best level (section C, voice state row) and parity (section D, voice state row)

| Metric | Before | After | Gate |
|---|---|---|---|
| Low−Best level, mean dB | +1.40 (per patch +1.19, +1.44, +0.96, +2.02) | +1.27 (per patch +1.16, +1.45, +0.91, +1.58) | ±2.5 dB per patch |
| Low vs Best LSD / corr | 15.63 / 0.993 | 10.56 / 0.995 | ≤17.0 / ≥0.975 |
| Draft vs Best LSD / corr | 20.41 / 0.970 | 24.81 / 0.962 | ≤42.0 / ≥0.94 |

## Gates

| Gate | Before | After |
|---|---|---|
| best_spectral LSD / corr | 3.15 / 0.988 | 3.15 / 0.988 |
| low_spectral LSD / corr | 28.62 / 0.978 | 28.62 / 0.978 |
| full suite (passed / failed / ignored) | 228 / 0 / 20 (pre-fix tree `4bf5fbc`, measured) | 230 / 0 / 20 |

The best_spectral gate in the before run set the VCF brightness explicitly to 0.70 (`set_circuit_brightness(cal)`), so it measured the intended brightness rather than what the voice played. It reads 3.15 both before and after. The gate now renders the voice's own brightness (`CIRCUIT_BRIGHT`), and after the fix that is 0.70, so the number is unchanged. The voice-state row above is where the played-brightness change shows (9.59 → 3.15 for Best).

## Verdict

The plugin now plays the intended brightness: Low 0.50, High 0.70, Best 0.70 on the first block, where before all three played 1.00. Against the SPICE gold, High's voice-state LSD fell from 9.34 to 3.07 dB (down 6.27) and Best's from 9.59 to 3.15 dB (down 6.44), with corr rising to 0.988. `LOW_LEVEL_TRIM` was not re-trimmed (still 1.55, unchanged across the branch); Low's spectral match (28.62 dB) and Low−Best level (mean +1.27 dB) stay within gate. Parity: Low-vs-Best LSD improved from 15.63 to 10.56 dB, as the plan expected. Draft-vs-Best LSD got worse, from 20.41 to 24.81 dB (corr 0.970 → 0.962), so the plan's expectation of improved parity did not hold for Draft. The audit had predicted this worsening; it is still inside budget (≤42.0 / ≥0.94).
