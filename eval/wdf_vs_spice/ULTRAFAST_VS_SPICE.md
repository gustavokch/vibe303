# ultra-fast tier vs SPICE (worst-case validation)

Validates the `ultra-fast` WDF tier (`os=1` + output-decimation bypass) against
on-the-fly SPICE, and compares it head-to-head against the `realtime` baseline
(`os=2` + decimation biquads). Time-aligned raw-volts ESR, 0.5 s onset-skip,
`|lag| ≤ 64`. Grid: worst cases — gains `{0.0, 0.5, 1.0}` × `{pink, sweep, music3}`.

Reproduce:

```bash
uv run python -m eval.wdf_vs_spice.compare --quality ultra-fast --gains 0.0,0.5,1.0 \
    --no-build --out eval/wdf_vs_spice/out_ultrafast
uv run python -m eval.wdf_vs_spice.compare --quality realtime   --gains 0.0,0.5,1.0 \
    --no-build --out eval/wdf_vs_spice/out_realtime
```

## Result — ultra-fast tracks SPICE *better* than realtime at every point

| stim   | gain | realtime ESR | ultra-fast ESR | Δ (ultra better) |
|--------|------|-------------:|---------------:|-----------------:|
| pink   | 0.00 |     −25.2 dB |       −28.6 dB |          +3.4 dB |
| pink   | 0.50 |     −25.6 dB |       −30.5 dB |          +4.9 dB |
| pink   | 1.00 |     −25.2 dB |       −31.2 dB |          +6.0 dB |
| sweep  | 0.00 |     −24.4 dB |       −35.0 dB |         +10.6 dB |
| sweep  | 0.50 |     −24.8 dB |       −36.0 dB |         +11.2 dB |
| sweep  | 1.00 |     −26.2 dB |       −33.2 dB |          +7.0 dB |
| music3 | 0.00 |     −38.6 dB |       −47.6 dB |          +9.0 dB |
| music3 | 0.50 |     −39.0 dB |       −51.0 dB |         +12.0 dB |
| music3 | 1.00 |     −39.4 dB |       −51.8 dB |         +12.4 dB |

- ESR **median**: ultra-fast **−35.0 dB** vs realtime −25.6 dB → **+9.4 dB**.
- ESR **worst**:  ultra-fast **−28.6 dB** vs realtime −24.4 dB.

## Why faster *and* more accurate

Counter-intuitive but expected from the lever design:

- realtime runs `os=2`, so the **output-decimation Butterworth biquads** are
  mandatory antialiasing. They impose group delay (every realtime row reports
  `lag=−1`, a sub-sample shift the aligner cannot fully remove) and a HF
  magnitude/phase roll-off that **deviates from the circuit's true response**.
- ultra-fast runs `os=1`: there is nothing to alias, so the biquads are bypassed
  (`--no-decimate`). Zero added delay (`lag=+0`), no synthetic HF coloration —
  it solves the same MNA/Newton system the realtime tier does, minus the
  decimation artifact.

The decimation filter was the dominant realtime-vs-SPICE error term on clean
broadband/sweep material (phase-sensitive ESR). Dropping it both halves the
solve rate (~3.4× over realtime end-to-end) and removes the artifact.

**Gate vs SPICE, never vs realtime.** "ultra-fast" names the speed lever, not a
fidelity downgrade — against the ground-truth circuit it is the most accurate
real-time-capable tier.
