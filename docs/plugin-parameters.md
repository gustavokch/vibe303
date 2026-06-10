# BA283 VST3 — Exposed Parameters

Source: `plugin/src/lib.rs` (`Ba283Params`).

| mapping | parameter | unit | range | default_value | description |
|---|---|---|---|---|---|
| `backend` | Backend | enum | Neural Net, Circuit (WDF) | Neural Net | Which emulation runs per sample. NeuralNet = default (current behaviour); Wdf = circuit-faithful MNA solver. |
| `wdf_tier` | WDF Tier | enum | Realtime, Fast, Ultra Fast, Warp | Realtime | WDF solver speed tier. Realtime = bit-exact ref; Fast = predict+schur+freeze-caps+ls-trim (~5×); Ultra Fast = os 2→1 + drop decimation (~2× over fast); Warp = + drop BJT charge model (~1.3× over ultra). WDF-only; inert for NeuralNet. |
| `precision` | Precision | enum | High, Medium, Low | High | Lossy precision dial, orthogonal to tier. High = identity (bit-exact at Realtime); Medium/Low = fixed-step solver + reduced charge model. WDF-only; inert for NeuralNet. |
| `drive_db` | Drive | dB | −12.0 … 24.0 (linear) | 0.0 | Input drive gain. Smoothed (linear 20 ms). |
| `bias` | Bias | — | 0.0 … 1.0 (linear) | 0.5 | Bias control. Smoothed (linear 50 ms). |
| `output_db` | Output | dB | −24.0 … 12.0 (linear) | 0.0 | Output gain. Smoothed (linear 20 ms). |
| `mix` | Mix | % | 0.0 … 100.0 (linear) | 100.0 | Dry/wet mix. Smoothed (linear 20 ms). |
| `level_norm` | Level Norm | bool | false / true | false | When on, scale wet so output RMS tracks input RMS (stereo-linked, applied before output_db/mix). Off = faithful preamp gain. |
| `circuit_gain` | Circuit Gain | Ohm | 47.0 … 10000.0 (skewed, factor −2.0) | 220.0 | R_KJ gain shunt. WDF-only; inert for NeuralNet. Block-rate via G0 rebuild, glided per block. |
| `age` | Age | — | 0.0 … 1.0 (linear) | 0.0 | Component aging: random per-component drift (caps + resistors) emulating vintage units. 0 = pristine, 1 = fully aged. WDF-only; inert for NeuralNet. Smoothed (linear 50 ms). |
| `age_reroll` | Re-roll Unit | bool | false / true | false | Momentary trigger: rising edge picks a new random drift pattern (different vintage unit). WDF-only. |
| `age_seed` | (age_seed) | — | u64 | `DEFAULT_AGE_SEED` | Persisted aging seed = "unit identity". `#[persist]`, NOT automatable; serialized with plugin state so a saved project recalls the same unit. Re-roll mutates it. |

## Notes

- First 12 rows = host-automatable params (`#[id]`). Last row = persisted state (`#[persist]`), not a host-exposed parameter.
- `bias` and `age` are unitless (0–1 normalized).
- Smoothing is not a unit/range; noted in description where present.
