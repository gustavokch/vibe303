# PR #84 F6 — Drive-swept NN levelNorm parity: investigated, tooling shipped, baking BLOCKED by placeholder weights

## Context
PR #84 gave the **WDF** backend a Drive-swept levelNorm makeup (`1/G_sat(drive)`), fixing the
"DRIVE sounds weak" sag. Re-review finding **F6** flagged that the **NeuralNet** backend still uses
the small-signal `1/G_nn` with the analytic `/drive`, so it sags the same way. F6 was deferred with
the *assumption* that placeholder NN weights make a Drive-swept table throwaway. This task tackled
F6 to confirm/deliver.

## What was built
- **`plugin/src/bin/measure_nn_gi.rs`** — the permanent generator, mirroring `bin/measure_gi` (the
  WDF one). Sweeps the same `DRIVE_GRID_DB` (−12..+24, every 3 dB) through the embedded WaveNet at
  each of the 5 gain positions (`cond = rgain_to_gain_norm(R)`, `input_cond = 0.5` — the exact
  levelNorm-ON conditioning), measures the 128×-de-normed output/input RMS gain `G_nn_sat`, and
  emits a paste-ready `1/G_nn_sat` table. One command to regenerate when real weights land.

## What was found (the reason it is NOT wired in)
Running the generator against the **current placeholder weights** produces a **divergent**, not a
saturating, curve. `1/G_nn_sat` (rows = position, cols = drive dB):

```
// pos \ drive_dB   -12      -9      -6      -3       0       3       6       9      12      15      18    21    24
//  3 30 dB     0.021915 .021866 .022152 .023074 .026393 .039149 .080099 .120531 .006420 .000221 0.0  0.0  0.0
//  4 35 dB     0.013056 .013120 .013433 .014785 .017646 .027883 .055058 .115644 .006162 .000207 0.0  0.0  0.0
```

- Small-signal columns (−12..0 dB) ballpark-match the existing `NN_GAIN_CALIB` `gnn_inv`
  (e.g. 220 Ω: 0.0264 vs committed 0.0209) → **the harness is valid**.
- Above ~+12 dB the placeholder net does NOT compress — its output **explodes** (G_nn_sat → ~1e7),
  so `1/G_nn_sat → 0`. A real saturating amp would do the opposite (`1/G_nn_sat` rising, as the WDF
  table does).

**Consequence:** baking this table would multiply the NN wet by ~0 above +12 dB → **silence the NN
under drive** — strictly worse than today's bounded small-signal sag. levelNorm cannot level a
diverging model; this is a model (weights) problem, not a leveling problem.

This is the F6 deferral, now **evidenced** rather than assumed.

## Decision
The divergence is not only a baking problem — the **live** NN path already blows up: at +24 dB Drive
on a 0 dBFS input the placeholder net's de-normed output reaches ~1e8, and even after the small-signal
`norm_gain` (~0.0013) the wet is **~1e5** — a speaker-damaging blast. The NeuralNet backend is
selectable today, so this is a shipping hazard regardless of leveling. Clamping (the user's call) is
the right fix for the *hazard*; it is orthogonal to *leveling* (a clamp can't synthesize the right
`1/G_nn_sat` curve — that still needs real weights).

So, three parts:
1. **Ship NN safety clamps** (`dsp.rs`, weight-agnostic, always on, bit-exact in-domain):
   - **Input-domain clamp** `NN_INPUT_CLAMP = 4.0` (+12 dBFS, net-input convention) on `driven` —
     **NeuralNet arm only** (the WDF arm keeps the unclamped `driven`; its circuit is self-limiting).
     Keeps the net inside its trained range so hot Drive saturates at the bound instead of
     extrapolating into the divergent regime (onset ≈ +12→+15 dB).
   - **Finite guard** on the raw model output (NaN/Inf → 0) so a pathological model can never poison
     `last_wet`, DPR state, the mix, or the host.
   - **Final-output ceiling** `NN_OUTPUT_CEIL = 8.0` (+18 dBFS, speaker domain) on the returned
     sample — last-line backstop. Applied to the FINAL output, NOT the hot pre-leveling intermediate
     (the NN runs ~G× the input by design; clamping there clips valid signal — found via the
     `nn_level_norm_holds_loudness` test regressing when the ceiling was mis-placed pre-leveling).
   - Test: `test_nn_levers.rs::nn_output_is_bounded_under_extreme_drive` (0 dBFS @ +24 dB Drive,
     levelNorm OFF → finite and ≤ ceiling; was ~1e5 before).
2. **Ship the generator tool** (`bin/measure_nn_gi`) + an evidenced TODO in `lib.rs` so the leveling
   table is one command from done once real weights land.
3. **Keep** the live NN levelNorm arm on the safe small-signal `1/G_nn` trim; **do not** bake the
   divergent leveling table.

## When real weights land (the remaining ~1 hr)
1. `cargo run --release --bin measure_nn_gi`; confirm the curve is monotonic/sane (`1/G_nn_sat`
   rising with drive, no collapse).
2. Add `nn_makeup: &'static [f32]` per position to `GAIN_STEPS`; paste the rows.
3. Reuse the WDF interp (`wdf_makeup_at`'s body generalizes to any `&[f32]` curve) for
   `nn_makeup_at(pos, drive_db)`.
4. Add a drift guard in `tests/calib_nn_makeup.rs` (re-measure the 2-D grid vs embedded), mirroring
   `embedded_makeup_matches_remeasured_gain`.
5. Switch the NN arm to `norm_apply_gain(nn_makeup_at(pos, drive_db_block), drive_block)`.
