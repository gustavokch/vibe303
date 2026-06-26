# HANDOFF — Souped-up TB-303 SPICE/ACME model

**Repo:** `~/Git/vibe303` — standalone 303 project (remote `gustavokch/vibe303`). The
303 work was relocated here out of the `ba283-vst3` worktree, and the BA283 preamp
product was removed (git history retains it; pre-removal ba283 code is at commit
`af74b60`). Phase-1 circuit commits (HEAD `a071dd0`) carried over.
**Design plan:** `~/.claude/plans/brainstorming-lets-use-our-eventual-waffle.md`
**Phase-2 design + plan:** `docs/plans/2026-06-26-phase2-303-voice-design.md` +
`docs/plans/2026-06-26-phase2-303-voice-implementation-plan.md`.

**STATUS: Phase 1 (SPICE/ACME circuit model) COMPLETE.** VCO core + VCF both built &
ngspice-validated; ACME parity = NO-GO for both (ngspice is ground truth). Next = Phase 2
(nih-plug DSP voice) — design + implementation plan written (see above); build per the
plan (M1 vertical slice first).

## Goal
Take the Roland TB-303 architecture "to the next level" via the `circuitgen`
circuit-IR pipeline (TOML → SPICE / ACME / WDF, ground-truthed by ngspice — the
same pipeline that drives the BA283).

Locked scope/decisions:
- **Analog circuit models:** the saw/pulse **VCO core** and the diode-ladder
  **VCF**. Everything else (2nd osc, sub-osc, multi-waveform, sequencer w/ per-step
  gate+velocity, envelopes, accent, VCA) is **DSP**, Phase 2.
- **Phased:** Phase 1 = SPICE/ACME model + validation. Phase 2 = nih-plug synth.
- **Tool gap closed:** the IR only had R/C/BJT/transformer; added **diode + JFET**
  primitives, emit to **SPICE + ACME** (Rust-WDF deferred). ACME is the nonlinear
  validator; ngspice is ground truth.
- **VCF cutoff** = parameterize the ladder bias current (signal path exact, expo
  converter replaced by a controllable current, like BA283's R_KJ). **Ladder rungs
  = `[[diode]]`** (Stinchcombe diode-ladder analysis).
- **Souped levers (circuit-level):** 2/3/4-pole = probe tap after rung T35/T36/T37;
  self-oscillation = `param=` on the T23 resonance feedback leg.

## Current progress
**Tooling (done, BA283 stays byte-identical — `circuitgen --check` green, 7/7
drift tests):**
- `tools/circuitgen/ir.py` — `[[diode]]`/`[[jfet]]` dataclasses + parse/validate;
  transformer/params/models now optional.
- `tools/circuitgen/emit_spice.py` — transformer-optional; generic `param`→kwarg
  signature via `_PARAM_ALIASES` (BA283 byte-identical); diode/JFET stamping;
  BJT terminals routed through `_spice_node` (ground → `circuit.gnd`).
- `tools/circuitgen/emit_acme.py` — module/header from `meta.name`; native
  `diode()` for `d`; **depletion-NMOS surrogate** `mosfet(:n; vt,α,λ)` for `njf`.
- `tools/circuitgen/targets.py` + `__main__.py` — `CIRCUITS` registry +
  `render_circuits()`; BA283 = rust+spice+acme, `vco303`/`vcf303` = spice+acme
  under `eval/303/` (auto-enabled when their TOMLs exist).
- `tools/schemagen/spice_import.py` — **NEW** deterministic SPICE-deck→draft-IR
  parser (the osc front-end; the SINA image path is image-only).
- `tools/schemagen/schema_spec.py` — taught the VLM diode/jfet; **fixed a real bug**
  (compact `key=v; key=v` examples were invalid TOML → every VLM draft failed to
  parse; now one-key-per-line).

**VCO core (DONE + validated):**
- `circuit/vco303.toml` — imported from `vibe303/netlist.txt` (21 nodes,
  15R/5C/6Q/1D/1J). `cv` marked a driven input.
- `eval/303/run_vco303_spice.py` — **ngspice: OSCILLATING, 1 V/oct verified**
  (CV 4/5/6 → 231/477/1000 Hz).
- `eval/303/vco303_acme.jl` — ACME model (spike artifact + VCF template).

**VCF (ALL STEPS 1–5 DONE + validated — see the numbered "Next steps" list below
for per-step detail + commits; circuit-level feature-complete):**
- Steps: 1 ladder core (c5615ab) → 2a/2b active stages → 3 fc-tracking resonance
  (active Q9 feedback, c70ba53) → 4 ACME parity NO-GO (cd809ac) → 5 tap-select
  2/3/4-pole (a071dd0). The numbered list at the bottom has the full story per step.
- `eval/303/VCF_MAP.md` — complete schematic read + §1–§13 findings log (the bridge;
  no netlist exists for the VCF). 4-pole ladder T34–T37 + .033/.018 caps + T38 diff
  input; gain stage T20/T22; output buffer T21; **active Q9 resonance amp**; expo
  cutoff T7/T11. §12 = ACME NO-GO; §13 = tap-select poles.
- `circuit/vcf303.toml` — **NEW.** Bare ladder core only: 8 diodes (4 rungs × 2
  rails, 1N4148) + 4 shunt caps (.033/.033/.033/.018µF). Active stages deferred.
- `circuit/vcf303.toml` — **full active VCF** (ladder + T38 diff input + T20/T22 gain
  + T21 buffer + active Q9 resonance amp; bias networks abstracted per VCF_MAP item 3).
- `eval/303/run_vcf303_spice.py` — the ngspice harness + 4 gates: default = 4-pole LP
  cutoff-tracks-bias; `--reso` = fc-tracking resonance; `--poles` = tap-select 2/3/4;
  `--op` = DC sanity. PySpice stamps, ngspice runs (PySpice can't drive ngspice-46).
- `eval/303/vcf303_acme.jl` + `compare_vcf303.py` — ACME NO-GO rig + reproducer (step 4).
- `circuit/vcf303.draft.toml` — gpt-4o SINA skeleton; superseded, kept as ref.
- ngspice-46 gotchas baked into the harness (see What didn't work).

## What worked
- **Generalizing circuitgen incrementally** with the drift-guard (`--check` + 7
  pytest) as a regression net — BA283 output never changed.
- **Importing the osc from `netlist.txt`** (deterministic) beat running vision on it.
- **ngspice convergence for the relaxation osc:** ramp the supply rails 0→V over
  1 ms via PWL sources (self-start), `sim.options(method="gear", rshunt=1e12,
  gmin=1e-10, reltol=1e-3, abstol=1e-9, vntol=1e-6, itl4=1000, trtol=7)`.
- **Reading dense schematic regions:** PIL crop + LANCZOS 3–4× upscale a region of
  the 2787×2647 PNG, then Read the crop. Made the VCF fully legible.
- **MOSFET as the JFET surrogate** in ACME (`mosfet(:n; vt=vto, α=beta, λ=lambda)`)
  — same source-follower topology + square-law clipping.

## What didn't work (don't repeat)
- **ACME for the VCO core = NO-GO.** ACME's fixed-step DK-method does NOT reproduce
  the saw-core relaxation oscillation (drifts to quasi-static, 0 cycles, convergence
  warnings; crashes at 384 kHz). Root cause: the regenerative SCR switching has
  sub-µs edges only ngspice's *adaptive* timestepping resolves. ⇒ **osc validation
  is ngspice-only**; ACME-vs-SPICE parity is reserved for the **VCF** (a filter).
- **`uic` + `.ic` for the osc** in ngspice → JFET starts hard-on (Vgs huge) →
  "timestep too small" collapse at t≈0. Use the ramped-supply self-start instead.
- **Plain `.op` start for the osc** → settles flat at the unstable equilibrium.
- **gpt-4o on the dense VCF block** → only a crude skeleton (ladder→4 diodes, reso/
  CV/diff-input dropped). It is a seed, not ground truth — human review required.
- **numpy 2.0** removed `ndarray.ptp()` → use `np.ptp(x)`.
- **PySpice + ngspice 46 = all-zero vectors** (PySpice doesn't support v46; prints
  "Unsupported Ngspice version 46"). ⇒ netlist with PySpice (`str(circuit)`), but
  RUN ngspice as a subprocess + parse `wrdata`. Also PySpice unit suffixes
  (`16e-6 @ u_A` → `"1.6e-05A"`) parse as **atto** in ngspice → ~zero bias → dead
  circuit; emit harness sources via `raw_spice` with **plain numbers**. And ngspice
  eats the **first deck line as the title** — start raw decks with a `* comment`.
- **VCF ladder AC rig:** the bottom sink must be an **ideal current sink** (open at
  AC), NOT a resistor — a low-Z resistive sink shorts the differential signal to
  ground and the cutoff stops tracking the bias current. Terminate the top with a
  differential `Rtop` (readout stand-in) to complete the AC loop.

## Next steps (VCF faithful build — incremental, sim at each step)
1. ✅ **DONE (c5615ab).** Ladder core `circuit/vcf303.toml` + `run_vcf303_spice.py`:
   4-pole LP confirmed, cutoff tracks bias current.
2. ✅ **DONE (2a f34aa4f + 2b 3b8e90b).** Active stages:
   - **2a:** T38 differential input stage (DMMT3904W = matched 2N3904) drives the
     ladder bottom; collectors → dl/dr, emitters via R164/R161 2.2K → tail sink =
     parameterised cutoff. Top tl/tr biased from +8V via R95 10K/rail.
   - **2b:** T20/T22 gain stage + T21 output buffer. T20 reads the ladder top
     DC-coupled (bases = tl,tr), R83/R84 22K → +8V, T22 tail sink → 3V3 "A" rail,
     T21 emitter-follower → `out`. **`A` net = 3V3 mod-offset rail (confirmed).**
   - Signal path transcribed faithfully; the gain-stage bias divider
     (R120/R122/R123, not legibly readable) is **abstracted** to a harness T22-base
     voltage `Vm2=3.9`, like the cutoff-expo abstraction. `.op` converges; `.ac` at
     `out` = 4-pole LP, cutoff tracks bias (f_-60dB 329→1633→6479→20716 Hz over
     2/8/32/128 µA tail). Notes: passband gain scales with bias (T38 gm, inherent);
     stopband has an HF feedthrough floor (diode junction caps). A closed-loop read
     (AC-coupled C79 input + R121 output→2nd-base feedback) was tried + reverted
     (mis-biased). T20 runs as a low-headroom diff buffer (ladder top sits ~+8V).
3. ✅ **RESOLVED (2026-06-26) — see the bottom of this item + VCF_MAP §11.** The
   "model can't resonate" verdict below is SUPERSEDED: the active Q9 feedback gives
   an fc-tracking resonance peak. History retained for context.
   ⚠️ ~~WIP / BLOCKED (2eebc68 + pole-spread dig).~~ Resonance T23 + souped
   self-osc lever: T23 buffers output (base ← out via R97), high-pass-couples
   (C64) through the resonance leg `Rreso` (build-param `rreso`) into in_r; input
   restructured to summing nodes (sig→R27→in_l, in_r biased bref→R_br + sums
   feedback). Harness `--reso` sweep mode added. **Lever works mechanically but
   musical resonance does NOT emerge.**

   **POLE-SPREAD DIG (2026-06-25 — OVERTURNS the earlier "make poles coincident"
   framing; full diagnostics in scratch `probe_poles.py` --r95/--itop/--phase/
   --loop/--difftest, findings now also in `run_vcf303_spice.py` --reso):**
   - The passive diode ladder's 4 poles are **inherently spread** (f60/f3 ~42–260×
     vs a coincident 4-pole's 5.6×) and **cannot be clustered by any termination** —
     high-Z current-source top makes it WORSE (260×), low-Z best (~42×). Coincidence
     is a *Moog* (buffered-stage) property; a diode ladder is passive, so spread is
     expected, **not** a bug to tune out. Rungs DO carry equal current (r equal).
   - Dominant pole pinned ~47 Hz @32µA, saturates ~hundreds Hz (R95 10K caps the
     top-section pole ~241 Hz; f3 scales ~I^0.5 not the I^1 of a clean ladder pole).
   - **Forward sig→out phase NEVER reaches −180° at a frequency with gain** (−90°
     only at ~3 kHz where |H|=−40 dB). So global feedback can only close the loop
     sub-50 Hz — confirmed: in_l/in_r, every C64 & Rreso, differential readout
     (tl−tr same 74× spread), and differential *current* feedback ALL peak at DC
     (1 Hz) or ~50 Hz, **never at fc**.
   - ⇒ Blocker is **NOT pole-placement** (impossible for this topology) and **NOT
     feedback-network tuning** (all variants fail identically). The modeled
     **open-loop ladder** cannot support fc resonance. The real TB-303 DOES
     resonate ⇒ the model diverges from the circuit.
   **Step 2b stays green with the reso leg open (default).**

   **SCHEMATIC RE-READ DONE (2026-06-25, VCF_MAP §7/§8).** Re-read the VCF off the
   ORIGINAL Roland sheet (`schem-5.gif`, center) — the TD3 PNG was the wrong source
   (different designators). Confirmed: (a) the feedback IS AC-coupled (`C23` 1µF) via
   `R97` ~10K into the second base of the bottom diff-input pair `Q12` = structurally
   IDENTICAL to the modelled `Rreso`+`C64`→in_r — so the feedback topology is NOT the
   bug; (b) the ladder rungs are diode-connected 2SC1583 pairs = the 1N4148 model is
   right; (c) ROOT CAUSE = the real readout gain stage is **AC-coupled** (`C21`
   electrolytic ladder-top→gain, `C17` input, `C15` output) with a **100K** load
   (`R114`), which decouples DC bias from AC gain so it runs the high loop gain
   self-osc needs WITHOUT saturating. The model's DC-coupled 22K-load readout
   structurally can't (sim: raising the DC-coupled load to 100K just shifts the
   op-point and KILLS the peak). Passive-ladder pole spread is inherent, NOT the
   lever; resonance comes from the high-gain AC-coupled loop.
   **AC-COUPLED READOUT REBUILT + MEASURED → §8 HYPOTHESIS FALSIFIED (2026-06-26).**
   Built the §7 fix in `vcf303.toml`: T20 bases AC-coupled off the ladder top via
   `C21L/C21R` (1µF), biased to an independent divider node `gbias`, loads raised to
   `R114` 100K. `--check` stayed green; `.op` converged; the readout gain rose to
   +28dB passband (vs ~+8dB DC-coupled). **But resonance did NOT emerge:** `--reso`
   still peaked at ~3–4 Hz, barely responding to feedback (open 1.48dB → 1k 1.74dB).
   More forward gain ≠ resonance — the blocker is the forward-path POLE SPREAD, not
   readout coupling. (Also regressed the step-2b automated gate: +28dB passband pushes
   the −60dB/slope metrics out of range.) **Changes REVERTED** — repo back to step-2b
   green. The §7-faithful AC-coupled readout is correct-as-topology but is NOT the
   resonance lever; re-apply only alongside a real fix (+ retune the gate metric).
   Scratch: `probe_readout.py` (readout works fine, +28dB), `sweep_rtop.py`,
   `test_isrctop.py` (under the `…-ba283-303-circuit/…/scratchpad`).

   **TOP-TERMINATION DIG (2026-06-26) — the real anomaly is the DOMINANT-POLE
   DEPRESSION, not the top termination.** The Roland sheet (`laddertop.png`) shows the
   ladder top fed by the **2SC2291 current source** (high-Z cascode), NOT a 10K-to-rail
   — so the model's `R95` 10K top is unfaithful and was the suspect. BUT: sweeping R95
   10K→1M LOWERS the dominant pole (40→2.3 Hz) and worsens spread; a faithful current
   source top + bleeder pins the dominant pole at ~1.6 Hz (bleeder×C). Resonance peaks
   always ride the dominant pole (1–47 Hz), never fc. ⇒ Top termination is NOT the
   lever. **THE CORE BUG:** the ladder's dominant pole sits ~47 Hz when the diode
   small-signal r·C predicts ~kHz (r=Vt/I≈1.6k @16µA, C=.033µ → ~3kHz) — ~**60× too
   low** — and the 4 poles spread 42–260× up to ~20kHz. A real 303 resonates, so the
   model's ladder SECTION structure diverges from the circuit. Prime suspects for the
   60×: (a) the differential cap-between-rails modelling vs the real per-rail shunt;
   (b) diode current far below i_tail/2; (c) 1N4148 vs the real diode-connected
   2SC1583 r/C; (d) cascaded-unbuffered-RC dominant-pole math (lowest pole ≈ 0.1/RC).
   **RESO/GAIN RE-READ DONE (2026-06-26, VCF_MAP §10).** Re-read the full Roland VCF:
   topology is faithfully transcribed (2SC2291 current-source top, AC-coupled C21
   readout, R114 100K, rail-bridging caps, Q12 input pair) — NO hidden inter-section
   buffer, it IS a passive ladder so the spread is faithful. Tested every faithful
   termination×coupling combo (resistive 10K–1M, current-source top, DC/AC readout,
   combined): ALL reproduce the spread; in every case a fixed RC (R95 / DC bleeder /
   the C21+Rg ≈1.6 Hz coupling corner) forms a dominant pole BELOW the bias-dependent
   ladder poles → no tweak yields high-Q fc-tracking resonance. **THE ONE SIMPLIFIED
   ELEMENT = the resonance feedback:** real = Q9/Q10/Q11 (3 transistors) + R64/R65 10K +
   R71 2.2K, driven by R97 10K + C23 1µF, feeding back around Q12 — almost certainly a
   DIFFERENTIAL regenerative Q-multiplier, not the model's single-ended T23→Rreso→in_r.
   A Q-multiplier synthesises resonance at fc DESPITE spread poles (no pole coincidence
   needed).

   **★ RESONANCE SOLVED (2026-06-26, VCF_MAP §11).** Pin-level re-read (`q9_pins.png`):
   the real feedback is the **active, INVERTING Q9 common-emitter amp** (emitter degen
   `R71` 2.2K → GND, base from output via `R64` 10K, collector load `Rc9` → +8V), NOT a
   differential Q-multiplier and NOT the old unity emitter-follower T23. The unity buffer
   was WHY it never resonated (no gain, no inversion → loop only closes at DC). Rebuilt
   `vcf303.toml` step-3: `out` →(C23 1µF)→ Q9 CE amp (gain ≈ Rc9/R71 ≈ 12×) →(C_out)→
   `Rreso` leg → **`in_r`**. Inject = in_r (Q9 inverts ⇒ loop −ve at DC, +ve near fc once
   the ladder rotates the phase; in_l = wideband neg-fb, just flattens). **Result: fc-
   TRACKING resonance up to ~20 dB**, peak follows cutoff — 532 Hz @16µA → 1.0 kHz @32µA
   → 1.7 kHz @64µA → 2.7 kHz @128µA (`run_vcf303_spice.py --reso`). The §8–§10 pole-spread
   "blocker" was a red herring (loop closes at ladder-phase + amp-180° = regen, near fc).
   `Rreso=RRESO_OFF` opens the leg ⇒ step-2b gate bit-exact; `--check` + 7/7 drift green;
   BA283 byte-identical. Stock loop self-limits (Q optimum ~Rreso 47k) — no self-osc
   without a higher-gain `Rc9`/`R71` "Devil-Fish" regime (optional souped follow-up).
   Scratch: `q9_pins.py`/`shape.py`/`track.py`/`selfosc.py` (under `…ba283-303-circuit/
   …/scratchpad`). **NEXT = step 5 (souped tap-select poles, ngspice-validated).**
4. ✅ **DONE (2026-06-26) — NO-GO. See VCF_MAP §12.** ACME-vs-SPICE parity for the
   VCF is **NOT achievable** (diode ladder *or* full active circuit). The 303 VCF
   sets cutoff with an IDEAL BIAS CURRENT; ACME's DC homotopy (no SPICE gmin) routes
   that current through the singularity-breaking reference leaks instead of the
   devices → a NONPHYSICAL all-devices-OFF branch (probed: ladder `tl=−64 kV`, every
   diode reverse-biased OFF; full circuit T38 OFF → bias-independent dead filter).
   Without leaks ACME aborts ("Failed to find initial solution"); with them it
   converges to the wrong branch. Also needed `decompose_nonlinearity=false` to build
   at all (decompose explodes: 8 diodes=16s, +6-BJT=801s). ngspice's Newton+gmin/
   source-stepping finds the physical branch; ACME's homotopy does not — **extends the
   VCO ACME NO-GO** (solver-robustness, not a model bug). ⇒ **ngspice stays the VCF
   ground truth** (steps 1–3, gated). Artifacts: `eval/303/vcf303_acme.jl` (bare-
   ladder rig, header = the NO-GO), `eval/303/compare_vcf303.py` (reproducer: SPICE
   ladder correct 86 dB 4-pole bias-tracking, ACME output collapses ~1e-8 V vs ~2 mV
   → `RESULT: NO-GO confirmed`, exit 0). Scratch probes under `…/scratchpad/acme_*.jl`.
5. ✅ **DONE (2026-06-26) — see VCF_MAP §13.** Souped tap-select 2/3/4-pole is a pure
   READOUT-TAP choice (no switch element, no IR change): signal injects at the ladder
   bottom (dl,dr), reading a lower differential tap crosses fewer shunt caps → fewer
   poles. `run_vcf303_spice.py --poles` (I_tail=32µA, atten @5kHz): **tl=−70.9dB (4-pole),
   bl=−57.2dB (3-pole), cl=−42.8dB (2-pole)**, monotone ~14dB/pole; `al≡tl` (rung-1
   diode adds no pole). Map: **4-pole=tl, 3-pole=bl, 2-pole=cl** (the plugin muxes which
   node the gain stage reads). Slopes compressed below 24/18/12 dB/oct by pole spread +
   HF feedthrough (§9), so the demo confirms the mechanism, not integer slopes. Default
   step-2b gate stays green. **⇒ Phase-1 VCF feature-complete at circuit level.**

## Phase-1 status: COMPLETE (VCO + VCF, ngspice-validated)
VCO core: oscillates + 1 V/oct (ngspice). VCF: 4-pole LP + bias-tracking cutoff (step
2) + fc-tracking resonance via active Q9 feedback (step 3) + tap-select 2/3/4-pole (step
5). ACME parity = NO-GO for both (current-source bias / fixed-step switching defeat
ACME's solver — ngspice is ground truth). **NEXT = Phase 2** (nih-plug monosynth voice;
gets its own brainstorm/plan per the design doc): osc (validated vco303 core + DSP
extras: 2nd osc, sub-osc, multi-waveform), VCF (diode-ladder WDF or Huovilainen/TPT
fitted to vcf303 + tap-select + self-osc), sequencer (per-step gate+velocity),
envelopes/accent/VCA. Cytomic VA papers digested as the realtime-filter reference
(design plan, Phase-2 section).

## Commands
```bash
cd ~/Git/ba283-303-circuit
# regenerate all backends / drift-guard
uv run --no-project python -m tools.circuitgen [--check]
uv run --no-project --with pytest python -m pytest tests/circuitgen/ -q
# import a SPICE deck -> draft IR
uv run --no-project python -m tools.schemagen.spice_import <deck> --name <n> --out circuit/<n>.draft.toml
# SINA VLM on a schematic (key at ~/Git/vibe303/.env)
set -a; . ~/Git/vibe303/.env; set +a
uv run --no-project python -m tools.schemagen <image> --out circuit/<n>.draft.toml --no-sim --hint "..."
# ngspice osc render
uv run --no-project --with PySpice --with numpy python eval/303/run_vco303_spice.py 5
# ACME (project-scoped: ACME 0.10)
julia --project=eval/acme_vs_spice eval/303/vco303_acme.jl 5 48000 0.15
```
Memory: mem0 (`user=gustavo`, `app_id=gustavokch-ba283-vst3`) holds all decisions +
the ACME/ngspice findings. `MEMORY.md` writes are hook-blocked → use mem0.
