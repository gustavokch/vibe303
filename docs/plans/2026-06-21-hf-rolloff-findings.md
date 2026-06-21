# HF rolloff — root-cause findings (2026-06-21)

Investigation per `~/.claude/plans/systematic-debugging-...-melodic-ullman.md`.
Branch `fix/hf-rolloff-investig`. Harness: new `plugin/tests/fr_probe.rs` (`--features measure`).

## Question
Clean-signal HF rolloff at default settings (WDF, Realtime os=2). Separate **faithful**
(real circuit) rolloff from **numerical** (host sample-rate path) rolloff.

## Step A — faithfulness (WDF vs SPICE, Welch FR @ 96k data rate, os=4)
`uv run python -m eval.wdf_vs_spice.compare --stims sweep_m3,pink_m3_s0 --gains 0.0,0.5 --bias 0.77`
- **FR max err 20Hz–20kHz = +0.6 dB**, midband +0.0 dB, **coherence 1.000**, ESR median −29.9 dB.
- ⇒ WDF reproduces the netlist circuit FR to ±0.6 dB. At the data rate (os=4, fc=43 kHz, numerical
  filters out of band) the rolloff seen is the **real circuit** (Zobel snubber `1.5k+0.01µF` ≈10.6 kHz
  corner + transformer + BJT Miller caps). **Faithful — not a bug.**

## Step B — numerical attribution at the shipped solve rate (os=2), fr_probe
Three paths, identical os=2 solve (circuit FR common-mode, cancels in differences):
REF=`capture_internal_direct`+steep sinc · UPS=`capture_os_raw`(lerp)+steep sinc · FULL=`process_host_sample`(lerp+Butterworth).
Values dB re 1 kHz; low-level sine (linear regime).

**host = 48000 Hz**
```
 freq     REF     UPS    FULL   lerp(U-R)  dec(F-U)  total(F-R)
 5001   -0.63   -0.85   -0.85    -0.22     +0.00     -0.22
 9999   -2.32   -3.26   -3.27    -0.94     -0.00     -0.94
12000   -3.17   -4.53   -4.55    -1.37     -0.02     -1.38
15000   -4.52   -6.69   -6.82    -2.17     -0.13     -2.30
17001   -5.45   -8.28   -8.70    -2.83     -0.42     -3.25
18999   -6.42  -10.01  -11.20    -3.59     -1.19     -4.78
20001   -6.91  -10.92  -12.84    -4.01     -1.92     -5.94
21000   -7.41  -11.87  -14.84    -4.46     -2.97     -7.43
```
**host = 44100 Hz** (decimator fc=19.8 kHz — worse)
```
 freq     REF     UPS    FULL   lerp(U-R)  dec(F-U)  total(F-R)
 9999   -2.35   -3.47   -3.47    -1.11     -0.01     -1.12
15001   -4.62   -7.21   -7.50    -2.60     -0.29     -2.89
19000   -6.63  -10.94  -13.66    -4.31     -2.72     -7.03
19999   -7.15  -11.98  -16.24    -4.83     -4.25     -9.08
21000   -7.70  -13.10  -19.15    -5.39     -6.05    -11.44
```

## Verdict
- **Circuit (REF):** ~−6.9 dB @20k (48k) — faithful to netlist (Step A). Keep.
- **Numerical, fixable:**
  - **Linear-interp upsample = DOMINANT** numerical loss: **−4.0 dB @20k (48k)**, −4.8 dB (44.1k).
    `plugin/src/wdf/process/mod.rs:317` (`frac=(j+1)/os`, first-order hold). Initial suspicion
    (decimator) was wrong; the crude upsampler is the bigger offender.
  - **Butterworth decimator = secondary**, top octave only: −1.9 dB @20k (48k), −4.25 dB (44.1k).
    `plugin/src/wdf/build.rs:270` (`fc=0.45·host_sr`, 4th-order).
  - Total non-faithful loss: **−5.9 dB @20k (48k)**, **−9.1 dB @20k (44.1k)** on top of the circuit.

## Fix implemented (steep linear-phase FIR) + verified
Replaced the os>1 host path's linear-interp upsample + Butterworth decimator with a steep
windowed-sinc (Kaiser β=9.62) **polyphase upsampler + FIR decimator** (`plugin/src/wdf/resample.rs`,
`FirResampler`), gated by a new `QualityConfig::legacy_resample` (default false; the C++ golden test
pins it true). Cutoffs at host Nyquist; relaxed upsampler (half=16·os) + steep decimator (half=64·os);
constant **80 host-sample** group delay. Wired into the plugin PDC: reported latency = `bridge_lat +
wdf_fir_lat`; dry delayed to it, WDF wet by `bridge_lat`, NN wet by `wdf_fir_lat` (`dsp.rs
resize_delays`). Offline `ba283_standalone` latency-compensates its WAV (trim+flush) so every eval
harness stays aligned; `--legacy-resample` for A/B.

Post-fix fr_probe (total numerical FULL−REF, dB):
- 48k: **0.00 dB through 20 kHz** (was −5.9), −0.14 @21k.
- 44.1k: 0.00 to 17k, −0.07 @19k, **−0.56 @20k** (was −9.1), −2.16 @21k (decimator transition, edge).

Verification (all green):
- `FirResampler` unit tests: unity reconstruction <2e-3 + DC unity + latency=80, os∈{2,3,4}.
- `fr_probe` regression gate: numerical rolloff <1.0 dB through 20 kHz at 48k & 44.1k.
- Golden C++ parity (`test_wdf`, legacy flag): 22/22 bit-exact. `process_host_sample` alloc-free.
- Full default-feature suite green (fixed `decimate_flag_changes_output` → legacy; 2 latency-align
  tests → `reported_latency`).
- Step A re-run on the **FIR path**: FR max err **+0.6 dB** vs SPICE, coherence 1.000, ESR −29.9 dB,
  lag=−1 (latency-compensation aligns) → circuit faithfulness unchanged.

## Fix direction (per approved decision tree)
1. Replace the os=2 linear-interp upsample with a proper polyphase/sinc reconstruction (or raise os).
   Biggest, SR-independent recovery (~4 dB @20k).
2. Steepen/raise the decimator cutoff toward host Nyquist (reuse the validated steep windowed-sinc
   from `headroom_probe.rs`), recovering ~2 dB @20k (48k) and the larger 44.1k loss — without
   re-aliasing (must re-check `alias_sweep`/`headroom_probe`).
Both reduce only NON-faithful rolloff; circuit FR (Zobel) stays. Re-run Step A (unchanged ±0.6 dB)
and fr_probe (flatter numerical cols) to verify.
```
Run fr_probe: cargo test --features measure --release --test fr_probe -- --nocapture
```
