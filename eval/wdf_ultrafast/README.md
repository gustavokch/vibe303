# WDF ultra-fast tier — measure-first lever evaluation

Harness for the **ultra-fast** (lossy) WDF solver tier. Goal of the round: ~1.5–2×
over the existing `fast` tier while staying near the model's ~−30 dB-vs-SPICE floor.

## TL;DR conclusion

The only lever that reaches the speed target is **dropping internal oversampling
2→1 and bypassing the (then-needless) output decimation**. Every per-iteration /
iteration-count lever buys ≈nothing — `predict` warm-start already pins
avgIters≈1.8, so the solver is already near its Pareto frontier.

Final preset: `QualityConfig::ultra_fast()` = `fast()` + `os: 1` + `decimate: false`.

| metric | result |
|---|---|
| speed | **≈2.0× over fast** (≈3.9× over realtime); avgIters≈1.9 |
| ESR vs SPICE | **−30 to −36 dB** (realtime −25 to −26) — *better* than realtime |
| midband coherence | 1.000 |
| points | pink@0.5, sweep_m3@0.5, sweep_m3@1.0 (pink@1.0 lost to ngspice OOM) |

Why better-than-realtime: at os=2 the output is antialias-decimated by a 43 kHz
4th-order Butterworth, whose HF magnitude/phase **deviates from the circuit's true
response**. At os=1 there is nothing to decimate, so bypassing those biquads tracks
SPICE more faithfully. The os=1 nonlinear-harmonic aliasing fear did not
materialise (coherence stays 1.0 even on a bright sweep at full gain).

Levers that were measured and rejected (≈no speed within budget): `cheap-exp`
(1.03×), looser `newton_tol` (−24 dB error at 1e-4 for 1.07×), capped `max_iters`
(0×, already ≈2 iters), `cap-tier depletion` (0×), `no-ls-trim` (slower). A chord /
quasi-Newton Jacobian was scoped out for the same reason — its only saving (skip
the per-iter 6×6 refactor + J reassembly) is the same magnitude as `cheap-exp`'s
1.03×, since `freeze-caps` already removed the per-iter `pow_` calls and the
remaining `exp_` calls feed the residual.

## Scripts

- **`measure.py`** — *authoritative fidelity*. Caches the SPICE reference once
  (ngspice is slow + RAM-flaky), then scores arbitrary Rust render configs (ESR vs
  SPICE, midband coherence, HF FR error). Use this for the os-lever fidelity gate.

  ```bash
  uv run python -m eval.wdf_ultrafast.measure --stim sweep_m3 --gain 0.5 \
      --config "realtime:--tier realtime" \
      --config "fast:--tier fast" \
      --config "ultra_fast:--tier ultra-fast"
  ```

- **`sweep.py`** — per-lever **speed + added-error scan** (SPICE-free is fine here).
  Layers each candidate lever on `fast`, reports speedup vs fast + avgIters. The
  `added_esr vs realtime` column is the right gate for the *non-OS* levers; for the
  OS levers it is misleading (it counts the realtime model's own os=2 decimation
  phase, uncorrelated with SPICE error) — use `measure.py` for those.

  ```bash
  uv run python -m eval.wdf_ultrafast.sweep --stims pink_m3_s0 --no-spice --runs 3
  ```

## Standalone CLI levers (`ba283_standalone --backend wdf`)

`--tier ultra-fast` selects the assembled preset. Individual A/B overrides:
`--os <n>`, `--decimate`/`--no-decimate`, `--cheap-exp`/`--no-cheap-exp`,
`--tol <f>`, `--max-iters <n>`, `--cap-tier <full|depletion|off>`,
`--ls-trim`/`--no-ls-trim`.
