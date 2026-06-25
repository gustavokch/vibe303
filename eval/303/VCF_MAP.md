# TB-303 / TD-3 VCF — circuit map (for vcf303.toml reconstruction)

Read from `vibe303/schematics/TD3_schematics.png` (VCF block, center-right) via
crop+4× upscale. No netlist exists for the VCF anywhere — this map is the bridge
to a faithful `circuit/vcf303.toml`. The gpt-4o SINA draft was only a crude
skeleton; this is the human-read ground truth.

Devices: `6× DMMT3904W` (dual matched NPN) for the ladder + gain stage; `BC847PN`
(NPN+PNP dual) for the expo converter. Rails: `+8V` analog, `3V3` offset rail,
GND. (Net label `A` recurs — needs wire-trace to confirm +rail vs bias node.)

## 1. Diode ladder (4-pole core) — `T34–T37` + `T38` input
Two rails (left/right); each `Tnn` box = a matched NPN pair = the two ladder
"diodes" of that rung (diode-connected). Shunt caps bridge the rails between rungs:
- `T34` (top rung) — top fed from the diff gain stage / ladder bias current.
  Left bias: `R95` 10K + `C87` 10µF/16V to the top node.
- `C98` .033µF  → `T35` rung
- `C99` .033µF  → `T36` rung
- `C100` .033µF → `T37` rung
- `C101` .018µF → `T38` = **differential input stage** (bottom; pins 1/2 collectors
  into ladder bottom rails, 4/5 emitters).
Input/bias on the bottom-left rails: `R27` 100k + `C19` 1µ (signal in from X12),
`R23` 20k, `R24` 200k, `C41` 1µF/50.

## 2. Differential gain stage (ladder readout) — `T20` + tail `T22`
- `T20` matched pair reads the ladder top (T34 collectors).
- Collector loads to +rail: `R83` 22K, `R84` 22K.
- Network: `R120` 100K, `R121` 100K, `R122` 10K, `R123` 10K, `R85` 10K, `R96` 2.2K.
- Comp caps `C79` 120n, `C80` 120n, `C81` 1µ.
- `T22` = tail current sink for the T20 pair (emitter via R96 2.2K).

## 3. Output buffer — `T21`
- Emitter-follower: base from T20 collector / R84 node, collector to +rail.
- `C61` 47µF/16 + `R66` 100 (to +8V test point), `C63` 1µF/50 + `R85` 10K,
  `R124` 10K → `X12-1` (output), `X12-4`/TP13 tap.

## 4. Resonance buffer / feedback — `T23`
- Single NPN; takes the filter output, buffers it back to the input (resonance).
- `R97` 2.2K, `R98` 100K, `R99` 100, `R163` 10K, `C75` 1µ, `C64` 1µF/50.
- **Souped-up lever:** this feedback gain sets resonance; extend its range past
  unity loop gain → self-oscillation (Devil-Fish style).

## 5. Cutoff CV → exponential current — `T7` (expo pair) + `T11`
- `T7` "VCF expo." matched pair (BC847PN) + `T11` convert summed CV to the
  exponential ladder bias current (= cutoff). `R56`/`R57` 10K, `R46` 2.2K,
  `3V3 mod. offset rail`.
- CV summing inputs: `R45` 100K (VCF CV **lin** input), `VR1` 470K (VCF OFFSET),
  `R47` 220K + `R59` 100K (VCF CV **log** inputs).
- **Cutoff lever:** the ladder bias current. In the model this is the parameterized
  cutoff control (like BA283's R_KJ gain leg).

## 6. Output to VCA
`C62` 1µF, `R39` 47K, `D9` → "to VCA".

## Souped-up plan (circuit-level, per the design)
- **2/3/4-pole tap select:** probe after rung T35 (2-pole) / T36 (3-pole) /
  T37/T38 (4-pole) — extra `listening`/probe nodes, no switch element.
- **Self-osc resonance:** `param=` on the T23 feedback leg; range pushes loop gain
  > 1.

## Open items before a converging netlist
1. Trace the `+8V` rail vs the `A` net vs GND across stages (diff-pair tail polarity).
2. Confirm each `Tnn` pair's internal diode-connection wiring (B-C vs B-E short).
3. Bias the ladder current source (T7/T38) — likely the parameterized cutoff input,
   not a literal expo-converter transcription (matches how BA283 parameterizes gain).
4. DC `.op` convergence, then AC sweep to confirm 4-pole LP + resonance peak.
