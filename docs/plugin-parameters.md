# BA283 VST3 — Exposed Parameters

Source: `plugin/src/lib.rs` (`Ba283Params`). Rows are in declaration order.

| mapping | parameter | unit | range | default_value | description |
|---|---|---|---|---|---|
| `backend` | Backend | enum | Neural Net, Circuit (WDF) | Circuit (WDF) | Which emulation runs per sample. NeuralNet = streaming FiLM WaveNet (`model.cond.nam`, zero added latency); Wdf = circuit-faithful MNA solver. |
| `ui_scale` | UI Scale | enum | Auto, 100%, 125%, 150%, 175%, 200% | Auto | Editor HiDPI scale. UI-only, **not** host-automatable (`.non_automatable()`); persisted in plugin state. Auto follows the host-reported scale. |
| `wdf_tier` | WDF Tier | enum | Realtime, Fast, Ultra Fast, Warp | Realtime | WDF solver speed tier. Realtime = bit-exact ref; Fast = predict+schur+freeze-caps+ls-trim (~5×); Ultra Fast = + cheap transport-exp & os 2→1 (~9×); Warp = + drop BJT junction-charge model (~12×). WDF-only; inert for NeuralNet. |
| `precision` | Precision | enum | High, Medium, Low | High | Lossy precision dial, orthogonal to tier. High = identity (bit-exact at Realtime); Medium/Low = reduced device charge model. WDF-only; inert for NeuralNet. |
| `oversampling` | Oversampling | enum | Auto, 1x, 2x | Auto | Internal antialiasing rate. Auto defers to the tier (bit-exact default); 1×/2× force the internal rate. WDF-only; inert for NeuralNet. |
| `drive_db` | Drive | dB | −12.0 … 24.0 (linear) | 0.0 | Input drive gain. Smoothed (linear 20 ms). |
| `bias` | Bias | — | 0.0 … 1.0 (linear) | 0.5 | Bias control. Smoothed (linear 50 ms). |
| `output_db` | Output | dB | −24.0 … 12.0 (linear) | 0.0 | Output gain. Smoothed (linear 20 ms). |
| `mix` | Mix | % | 0.0 … 100.0 (linear) | 100.0 | Dry/wet mix. Smoothed (linear 20 ms). |
| `eq_on` | EQ | bool | false / true | false | Neve 1073/1076-style EQ ON/OFF (**both backends**, pre-model). OFF (default) = provable bit-exact no-op. |
| `eq_hpf_freq` | EQ HPF | enum | Off, 50, 80, 160, 300 Hz | Off | Switched high-pass corner (~18 dB/oct). Off bypasses the HPF. Both backends. |
| `eq_low_freq` | EQ Low Freq | enum | 35, 60, 110, 220 Hz | 60 Hz | Low-shelf corner (S3). Both backends. |
| `eq_low_gain` | EQ Low | dB | −16.0 … 16.0 (linear) | 0.0 | Low-shelf gain. 0 dB = flat. Per-block glided. |
| `eq_mid_freq` | EQ Mid Freq | enum | 360, 700, 1600, 3200, 4800, 7200 Hz | 1600 Hz | Mid/presence-bell center (S5). Both backends. |
| `eq_mid_gain` | EQ Mid | dB | −18.0 … 18.0 (linear) | 0.0 | Mid-bell gain (proportional-Q narrows with boost). 0 dB = flat. Per-block glided. |
| `eq_high_gain` | EQ High | dB | −16.0 … 16.0 (linear) | 0.0 | High-shelf gain (fixed 12 kHz). 0 dB = flat. Per-block glided. |
| `level_norm` | Level Norm | bool | false / true | false | When on, scale wet so output RMS tracks input RMS (stereo-linked, applied before output_db/mix). Off = faithful preamp (linear 50 ms). |
| `gain_stepped` | Stepped Gain | bool | false / true | false | Hard-snap `circuit_gain` to 5 real BA283 wafer positions (15/20/25/30/35 dB) instead of gliding. Authenticity feature; no CPU effect. WDF-only. |
| `circuit_gain` | Circuit Gain | Ω | 47.0 … 10000.0 (skewed) | 220.0 | R_KJ gain shunt. WDF-only; inert for NeuralNet. Block-rate `G0` rebuild, glided per block. |
| `age` | Age | — | 0.0 … 1.0 (linear) | 0.0 | Component aging drift (caps + resistors). 0 = pristine (bit-exact). WDF-only; inert for NeuralNet. Smoothed (linear 50 ms). |
| `age_reroll` | Re-roll Unit | bool | false / true | false | Momentary trigger: rising edge picks a new random drift pattern (different vintage unit). WDF-only. |
| `age_seed` | (age_seed) | — | u64 | `DEFAULT_AGE_SEED` | Persisted aging seed = "unit identity". `#[persist]`, NOT automatable; serialized plugin state so saved projects recall the same unit. Re-roll mutates it. |
| `editor_state` | (editor_state) | — | 660 × 900 | — | Persisted egui editor state (window size/scale). `#[persist]`, NOT a host parameter. |
| `trafo_on` | Transformer | bool | false / true | true | _Feature `trafo-core` (on by default)._ Jiles-Atherton LO1166 output-core ON/OFF. OFF = bit-exact linear core. WDF-only. |
| `trafo_hyst` | Hysteresis | — | 0.0 … 1.0 (linear) | 0.5 | _Feature `trafo-core`._ Hysteresis dial — loop width (pinning `k` + reversibility `c`). WDF-only. Smoothed (linear 50 ms). |
| `trafo_sat` | Saturation | — | 0.0 … 1.0 (linear) | 0.5 | _Feature `trafo-core`._ Saturation dial — knee position (`b_sat`). WDF-only. Smoothed (linear 50 ms). |

## Notes

- Host-automatable params carry `#[id]`. `ui_scale` has an `#[id]` but is marked `.non_automatable()` (UI-only). `age_seed` and `editor_state` are persisted state (`#[persist]`), not host-exposed parameters.
- The three `trafo_*` rows exist only when the crate is built with the `trafo-core` feature (the default). `--no-default-features` drops them.
- EQ params (`eq_*`) apply to **both** backends; `wdf_tier`/`precision`/`oversampling`/`circuit_gain`/`gain_stepped`/`age`/`age_reroll`/`trafo_*` are inert on the NeuralNet backend.
- `bias` and `age` are unitless (0–1 normalized). Smoothing is not a unit/range; it is noted in the description where present.
