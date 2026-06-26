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
1. ✅ Trace the `+8V` rail vs `A` net vs GND: top rail = +8V; `A` = 3V3 mod-offset.
2. ✅ Each `Tnn` pair = **diode-connected** 2SC1583 dual (B-C shorted, 2-terminal) —
   confirmed on the Roland sheet; the 1N4148 diode-ladder model is structurally right.
3. ✅ Ladder bias = parameterized cutoff current (abstracted), matches BA283 R_KJ.
4. ✅ `.op` converges; AC = 4-pole LP, cutoff tracks bias (step 2b). Resonance ✅ —
   active Q9 feedback gives fc-tracking peak, see §11 (supersedes the §8–§10 ✗).

## 7. Designator cross-reference (Roland sheet ↔ this map / TOML)
The VCF block was re-read from the **original Roland TB-303** sheet
(`schematics/roland.TB-303.schem-5.gif`, main board, center) — the TD3/Behringer
PNG uses different numbers (T43/T46/IC11…) and was the wrong source for the ladder.
Real Roland VCF designators:
- **Ladder** (diode-connected 2SC1583 pairs, top→bottom): top device **2SC2291**
  (cutoff-current source / cascode into the ladder top), then rungs **Q16/Q23**,
  **Q14/Q17**, **Q13/Q15**, bottom pair **Q12** (the differential INPUT pair).
- **Shunt caps**: `C26`/`C24`/`C19` = .033µF (upper three), `C18` = .018µF (bottom).
- **Gain / readout** (off the ladder top): **Q19/Q20** + `R114` **100K** load, `R96`
  10K, `R109`/`R108`; **AC-coupled via `C21`** (electrolytic) from the ladder top.
- **Input**: signal → `C17` (1µF/50) → `Q12` base. Emitter degen `R68`/`R69` 2.2K.
- **Resonance buffer**: **Q9/Q10/Q11** → feedback **AC-coupled via `C23` (1µF/50)
  + `R97` ~10K into the SECOND base of `Q12`** (the input diff pair). Output also
  AC-coupled via `C15` (1µF/50).
- **Cutoff CV summing** (the expo front-end, abstracted): `R73` 100K, `R63` 470K,
  `R72` 220K, `R71` 2.2K → expo → ladder-top current.

## 8. Resonance failure — root cause (pole-spread + AC-coupling dig, 2026-06-25)
The reconstructed feedback topology (`Rreso`+`C64` → in_r base) and the diode-ladder
model are BOTH faithful (confirmed above). The no-resonance blocker is the **readout
gain stage being DC-coupled with low (22K) loads** in the model, vs the real circuit's
**AC-coupled (`C21`/`C17`/`C15`) high-gain (`R114` 100K) stage**. AC-coupling
decouples DC bias from AC gain → the real stage runs the high loop gain self-osc needs
WITHOUT saturating; the DC-coupled model can't (sim: 22K→100K load just shifts the DC
op-point and kills the peak). Passive diode-ladder poles are inherently spread (not a
bug — see `run_vcf303_spice.py --reso`); resonance comes from the high-gain AC-coupled
loop, not pole coincidence. ~~FIX = rebuild the readout AC-coupled per §7~~ —
**FALSIFIED 2026-06-26** (see §9).

## 9. AC-coupled readout FALSIFIED + the real anomaly (2026-06-26)
The §8 fix (AC-couple T20 via `C21`, bias to `gbias`, `R114` 100K loads) was BUILT and
MEASURED: readout gain rose to +28dB but **resonance did not emerge** — `--reso` still
peaked at ~3–4 Hz (the `C64`/`Rreso` high-pass corner), unmoved by feedback. More
forward gain ≠ resonance. The §7 AC-coupled/100K readout is faithful-as-topology but
NOT the resonance lever; the change was reverted (it also regressed the step-2b gate).

**Top-termination dig:** `laddertop.png` confirms the real ladder top = the **2SC2291
current source** (high-Z), not `R95` 10K-to-rail. But neither a high resistive top
(R95→1M lowers the dominant pole to ~2 Hz) nor a faithful current-source top (pins it
~1.6 Hz via the DC bleeder×C) clusters the poles or lands resonance at fc.

**The core bug = DOMINANT-POLE DEPRESSION.** The ladder's first pole sits ~47 Hz where
the diode r·C predicts ~kHz (r=Vt/I≈1.6k @16µA × C=.033µ → ~3kHz) — ~60× too low — and
the 4 poles spread 42–260× up to ~20kHz. Feedback always regenerates at that low
dominant pole, never at fc. The real 303 resonates ⇒ the modelled ladder SECTION
structure diverges. Suspects: differential-cap-between-rails vs real per-rail shunt;
diode current ≪ i_tail/2; 1N4148 vs diode-connected 2SC1583 r/C; cascaded-unbuffered-RC
pole math. NEXT = localise the 60× (per-section pole instrument) OR confirm the real
resonance mechanism OR defer self-osc to Phase-2 DSP and proceed to ACME parity.

**Rung-device test (2026-06-26) — NEGATIVE.** Swapped the 8 ladder diodes for BC-tied
2N3904 (= the real diode-connected 2SC1583, n=1 vs 1N4148's n=1.7) in the ngspice deck
(`test_2sc.py`). Dominant pole rose only ~1.4× (47→55 Hz @32µA, the n ratio) and the
spread WORSENED (74→101×, upper poles rise more than the dominant). The rung device is
NOT the 60× cause. ⇒ the dominant-pole depression + spread is STRUCTURAL to the passive
cascaded-RC ladder (unbuffered cascade → lowest pole ≈0.1/RC, poles inherently spread —
the opposite of a buffered Moog). Not committed (negative + complicates ACME). Remaining
fork: structurally hunt the 60× / pin down the real-303 resonance mechanism (is stock
res genuinely low-Q?) / defer self-osc to Phase-2 DSP and proceed to step 4 ACME parity.

## 10. Reso/gain schematic RE-READ (2026-06-26) — the unfaithful spot = the Q9/Q10/Q11 feedback
Re-read the full Roland VCF (`vcf_roland.png`, `gain.png`, `reso_tight.png`). Confirmed
faithfully transcribed: 2SC2291 current-source top; AC-coupled readout via `C21`;
`R114` 100K gain load (Q19/Q20 = 2SC1583); rail-bridging caps C26/C24/C19=.033, C18=.018;
Q12 = bottom 2SC1583 input pair, emitter degen R68/R69 2.2K. **No hidden inter-section
buffer/bootstrap — it IS a passive ladder, so the pole spread is faithful.**

Tested every faithful termination×coupling combo (resistive top 10K–1M, current-source
top, DC vs AC-coupled readout, all combined) — ALL reproduce the inherent passive-ladder
spread, and in EVERY case some fixed RC (R95, the DC bleeder, or the C21/Rg ≈1.6 Hz
coupling corner) forms a dominant pole BELOW the bias-dependent ladder poles. So no
termination/coupling tweak yields high-Q fc-tracking resonance.

**THE ONE THING THE MODEL SIMPLIFIES = the resonance feedback.** Real = **Q9/Q10/Q11**
(3 transistors) + `R64`/`R65` 10K + `R71` 2.2K, driven by `R97` 10K + `C23` 1µF from the
output, feeding back around Q12 — almost certainly a DIFFERENTIAL regenerative "Q-
multiplier", not the single-ended `T23`→`Rreso`→in_r in the model. A differential Q-
multiplier can synthesise resonance at fc DESPITE spread poles (it doesn't need pole
coincidence). ⇒ NEXT = transcribe the Q9/Q10/Q11 feedback faithfully (trace whether it
modulates the Q12 tail current or injects push-pull into both bases) and re-test `--reso`.
This is the prime remaining candidate for "real 303 resonates, model doesn't".

## 11. RESONANCE SOLVED (2026-06-26) — the active Q9 feedback was the missing piece
The §8–§10 "model can't resonate at fc" verdict is **OVERTURNED**. Root cause of the
failure: the reconstructed feedback used a **unity emitter-follower** (old `T23`) — no
gain, no inversion — so the loop could only close at DC (every Rreso peaked at ~1 Hz).

**Pin-level re-read of the Roland sheet** (`q9_pins.png`, `reso_cluster.png`): the real
feedback is the **active, inverting Q9/Q10/Q11 stage**. `Q9` = a common-emitter amp
(emitter degenerated to GND via `R71` 2.2K, base from the output via `R64`/`R65` 10K,
collector loaded to +8V), buffered by `Q10`/`Q11`. It supplies the regenerative VOLTAGE
GAIN the unity buffer lacked.

**Faithful build** (`vcf303.toml` step-3, rework): `Q9` transcribed as the single CE
gain stage (gain ≈ `Rc9`/`R71` ≈ 27k/2.2k ≈ 12×; `Q10`/`Q11` output-buffer folded into
the existing `out` follower; base bias divider abstracted per VCF_MAP item 3). Output
AC-coupled (`C23` 1µF) → CE amp → AC-coupled (`C_out`) → resonance leg `Rreso` (param,
= the RESONANCE pot VR4) → **`in_r`** (the OTHER diff input). The inject node is the key:
because `Q9` inverts, feeding `in_r` makes the loop **negative at DC** (stable, flat) but
**positive near fc** once the ladder's own phase lag rotates the sign — the textbook way
a multipole filter resonates. (Injecting `in_l` instead = wideband negative feedback,
just flattens — measured.)

**Result** (`run_vcf303_spice.py --reso`): an **fc-TRACKING** resonance peak up to ~20 dB.
Peak frequency follows the bias current (cutoff): **532 Hz @16µA → 1.0 kHz @32µA → 1.7
kHz @64µA → 2.7 kHz @128µA**. `Rreso` sweeps Q to an optimum ~47k; the stock loop then
self-limits (no self-oscillation — that's Devil-Fish territory, an optional souped
higher-gain `Rc9`/`R71` regime). `Rreso = RRESO_OFF` (1e12) opens the leg → **step-2b
gate stays bit-exact** (4-pole LP, cutoff tracks bias; gate green). The passive-ladder
pole spread (§9) was a red herring — the loop closes where ladder phase + the amp's 180°
sum to regeneration, near fc, regardless of where the open-loop dominant pole sits.
NEXT = step 4 (ACME-vs-SPICE VCF parity).
