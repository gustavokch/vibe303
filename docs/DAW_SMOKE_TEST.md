# DAW Smoke-Test — plugin303 (303 fidelity tiers)

Manual, human-in-the-loop check that the plugin loads and sounds right in a host.
The automated gates cover spectra/level/pitch/no-alloc; this covers the things
only ears + a real host catch. Run after building the bundle.

## Build the bundle

```bash
cargo xtask bundle plugin303 --release
#   → target/bundled/TB303.vst3  and  target/bundled/TB303.clap
```

Verified building on this branch: both `TB303.vst3` and `TB303.clap` are produced.

## Checklist

- [ ] **Loads** in a VST3 host (Reaper / Bitwig / Carla) **and** a CLAP host
      without crashing or printing errors.
- [ ] **Note-on produces sound on all four tiers** (Draft / Low / High / Best). Switch
      the `Tier` param under a **held note** — the switch is **click-free** (F5.1,
      ~10 ms equal-power crossfade); listen for pops on every pair.
- [ ] **Cutoff sweep** audibly opens/closes the filter on Draft, Low, High, and Best.
- [ ] **Resonance** audibly peaks; on **Low/High/Best** the peak **tracks cutoff**
      (F6.3) — sweep cutoff with reso high and confirm the resonant whistle
      follows the cutoff up/down (vs Draft/TPT's parametric peak).
- [ ] **Pitch is accurate** across the keyboard (F6.6) — play an octave run on
      Best and check it is in tune (no low-note flatness).
- [ ] **Host PDC** (F5.2): Low/High/Best report added latency vs Draft. Verify in a
      host that shows plugin latency (Reaper: FX → "PDC"). Note: latency is now
      padded to a constant (Best's 4× OS group delay), so it should **not jump**
      when switching tiers (PR4 T5).
- [ ] **No denormal / CPU spikes** on sustained notes. Best is ~1.5 cores/voice
      (offline-grade — expected); Low ~0.4 core (RT-viable). Confirm Draft/Low run
      comfortably in real time; Best may glitch at low buffer sizes — expected.
      High must also make sound; its CPU was not profiled.
- [ ] **A/B the Best render by ear** against `eval/303/gold/voice_A2_mid.f32`
      (note 57 / cutoff 1002 Hz / reso 0.85). Same brightness / resonance
      character?
- [ ] **First-note-after-load / transport-restart** has no multi-second stall
      (the VCO pitch LUT is per-sr cached; the first construction at a new host sr
      builds it once — a brief one-time init, not per note/reset).

## Findings

_Record results here after running the checklist in a host._

| Host | VST3/CLAP | Loads | Tiers sound | Click-free switch | Reso tracks fc | PDC | Notes |
|------|-----------|-------|-------------|-------------------|----------------|-----|-------|
|      |           |       |             |                   |                |     |       |

## Bundling

`cargo xtask bundle plugin303 --release` is wired (the `xtask` workspace member
calls `nih_plug_xtask::main()`). Bundle name is `TB303` (set in the plugin's
nih-plug metadata), not `plugin303`.
