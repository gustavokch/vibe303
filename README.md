# vibe303 — Souped-up TB-303

Circuit-grounded emulation of the Roland TB-303, taken "to the next level": a
validated SPICE/ACME model of the analog core (Phase 1) and a nih-plug VST3/CLAP
monosynth voice built on top of it (Phase 2).

- **Version:** 0.1.0
- **Vendor:** desmen
- **License:** TBD

> This repo began as a fork of the BA283 preamp project (shared `circuitgen`
> circuit-IR pipeline + ngspice/ACME validation harness). The BA283 product code
> was removed; what remains is the 303 work plus the reusable pipeline.

---

## Status

**Phase 1 — analog circuit model: COMPLETE.** VCO core + diode-ladder VCF modelled as
circuit IR (`circuit/vco303.toml`, `circuit/vcf303.toml`), emitted to SPICE + ACME via
`tools/circuitgen`, and ngspice-validated (`eval/303/`). VCO oscillates with 1 V/oct;
VCF gives a 4-pole LP with bias-tracking cutoff, active-Q9 fc-tracking resonance, and
tap-select 2/3/4-pole. ACME parity = NO-GO for both (ngspice is ground truth). See
`HANDOFF.md` and `eval/303/VCF_MAP.md`.

**Phase 2 — nih-plug voice: PLANNED.** Monophonic VST3/CLAP synth (`plugin303`): fitted
antialiased VCO, TPT diode-ladder VCF, MEG/VEG envelopes, accent, 16-step sequencer +
live MIDI, souped levers (2nd osc, sub, saw↔square sweep, self-osc). Design + plan:
`docs/plans/2026-06-26-phase2-303-voice-design.md` and
`docs/plans/2026-06-26-phase2-303-voice-implementation-plan.md`.

## Layout

- `circuit/` — circuit-IR definitions (`vco303.toml`, `vcf303.toml`).
- `tools/circuitgen/` — IR → SPICE/ACME emitters + drift guard.
- `tools/schemagen/` — SPICE-deck / schematic → draft-IR importers.
- `eval/303/` — ngspice + ACME validation harnesses (the ground truth).
- `eval/acme_vs_spice/` — the shared Julia ACME project environment.
- `tests/circuitgen/` — drift-guard + IR-validation tests.
- `xtask/` — nih-plug bundler runner (for the Phase-2 plugin).

## Commands

```bash
# regenerate all backends from the IR / drift-guard
uv run --no-project python -m tools.circuitgen [--check]
uv run --no-project --with pytest python -m pytest tests/circuitgen/ -q
# ngspice 303 renders
uv run --no-project --with PySpice --with numpy python eval/303/run_vco303_spice.py 5
uv run --no-project --with PySpice --with numpy python eval/303/run_vcf303_spice.py --reso
```
