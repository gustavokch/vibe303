# HANDOFF — Souped-up TB-303 SPICE/ACME model

**Branch:** `feat/souped-303-circuit` (worktree `~/Git/ba283-303-circuit`, repo of
record; synced copy lives in `ba283-vst3/tools/`). 6 commits, all pushed.
**Design plan:** `~/.claude/plans/brainstorming-lets-use-our-eventual-waffle.md`
**Source material:** `~/Git/vibe303` (schematics, docs, `netlist.txt`).

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

**VCF (step 1 DONE + validated — commit c5615ab):**
- `eval/303/VCF_MAP.md` — complete schematic read (the bridge; no netlist exists
  for the VCF). 4-pole ladder T34–T37 + .033/.018 caps + T38 diff input; gain
  stage T20/T22; output buffer T21; reso buffer T23; expo cutoff T7/T11.
- `circuit/vcf303.toml` — **NEW.** Bare ladder core only: 8 diodes (4 rungs × 2
  rails, 1N4148) + 4 shunt caps (.033/.033/.033/.018µF). Active stages deferred.
- `eval/303/run_vcf303_spice.py` — **ngspice AC: 4-pole LP ~−22..−24 dB/oct,
  cutoff tracks bias current** (8.4→31.7→119→399 Hz across 1/4/16/64 µA).
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
3. ⚠️ **WIP / BLOCKED (2eebc68 + pole-spread dig).** Resonance T23 + souped
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
   **NEXT = rebuild the readout AC-coupled per VCF_MAP §7** (add C21 ladder-top→gain,
   R114 100K load, C17 input, C15 output coupling caps; re-bias the gain stage off
   its own divider so the load no longer sets DC), then re-test resonance/self-osc
   with the `--reso` harness. Then step 4 (ACME parity), step 5 (tap-select poles).
4. **ACME-vs-SPICE parity** for the VCF (use `vco303_acme.jl` as the `@circuit`
   template + the generated `_vcf303_gen.jl`); target ≈ −30 dB ESR like BA283.
5. **Souped tap-select poles:** expose probe nodes after rungs T35/T36/T37.

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
