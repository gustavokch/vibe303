# AGENTS.md — vibe303

## Project Overview

vibe303 is a circuit-grounded emulation of the Roland TB-303 synthesizer, built as a **nih-plug VST3/CLAP monosynth** (Rust) backed by a validated SPICE circuit model (Python + ngspice). The project has two phases:

- **Phase 1 (COMPLETE):** Analog circuit model — VCO and VCF modelled as circuit IR (`circuit/*.toml`), emitted to SPICE + ACME via `tools/circuitgen`, and ngspice-validated under `eval/303/`.
- **Phase 2 (IN PROGRESS):** nih-plug voice — a 3-tier fidelity axis (Low / High / Ultra) plugin crate (`plugin303`), with the fidelity tiers complete and gated. Remaining: DAW smoke-test, accent/glide/decay levers, GUI.

---

## Repository Layout

```
circuit/              Circuit IR definitions (*.toml = source of truth)
plugin303/            Rust nih-plug crate (VST3/CLAP synth, the product)
  src/lib.rs            Plugin entry point (Plugin303 impl)
  src/params.rs         nih-plug parameter declarations
  src/dsp/              DSP modules (voice, filters, oscillators, etc.)
  src/bin/render303.rs  Headless raw-f32 renderer for offline evaluation
  tests/                Rust integration tests (spectral gates, no-alloc, etc.)
tools/circuitgen/     Python — IR → SPICE / ACME emitters + drift guard
tools/schemagen/      Python — schematic image → draft IR (experimental)
tools/gen_fir_coeffs.py  FIR coefficient generator for the oversampler
eval/303/             Python + ngspice validation harnesses (ground truth)
eval/acme_vs_spice/   Julia ACME.jl project environment (NO-GO, historical)
tests/circuitgen/     Python drift-guard tests (IR ↔ generated backends)
docs/plans/           Design docs and implementation plans
docs/DAW_SMOKE_TEST.md  Manual smoke-test checklist for the plugin
scripts/              Platform build + install scripts
xtask/                nih-plug bundler runner
.github/workflows/    CI: manual-dispatch plugin builds + GitHub Release
.cargo/config.toml    Cargo aliases + per-target RUSTFLAGS
```

---

## Language & Toolchain

| Domain | Language | Toolchain |
|--------|----------|-----------|
| Plugin / DSP | Rust (edition 2021) | `cargo` / `rustup`; nih-plug pinned to a specific git rev |
| Circuit IR pipeline | Python 3.12 | `uv` (package manager); stdlib `tomllib` for the IR; no heavy deps for circuitgen |
| Evaluation harness | Python 3.12 | `uv run --no-project` with `--with PySpice --with numpy`; ngspice-46 required |
| ACME parity (historical) | Julia | ACME.jl; verdict = NO-GO for both 303 circuits |
| CI | GitHub Actions | Manual dispatch; self-hosted linux x64 runners |
| Linting | `ruff` (Python), `cargo clippy` (Rust) | ruff config in `pyproject.toml`; line-length 100 |

### Key Commands

```bash
# Rust — plugin tests (the primary gate)
cargo test -p plugin303

# Rust — bundle the VST3/CLAP plugin
cargo xtask bundle plugin303 --release

# Rust — headless render (for offline eval / gold generation)
cargo run --bin render303 -p plugin303 -- --note 57 --sr 48000 --secs 0.5 \
    --cutoff 1000 --reso 85 --tier 2 --out /tmp/render.f32

# Python — regenerate all backends from circuit IR (drift guard)
uv run --no-project python -m tools.circuitgen [--check]

# Python — drift-guard + IR-validation tests
uv run --no-project --with pytest python -m pytest tests/circuitgen/ -q

# Python — ngspice 303 evaluation renders
uv run --no-project --with PySpice --with numpy python eval/303/run_vco303_spice.py 5
uv run --no-project --with PySpice --with numpy python eval/303/run_vcf303_spice.py --reso

# Python — whole-voice comparison (ESR table, ~3 min)
uv run --no-project --with PySpice --with numpy python eval/303/compare_voice.py

# macOS local build + install
bash scripts/build-macos-arm64.sh
```

---

## Architecture

### Circuit IR Pipeline (Phase 1)

The **single source of truth** for each circuit is its `circuit/*.toml` file. The `tools/circuitgen` package loads, validates, and emits each backend:

```
circuit/vco303.toml ──┐
circuit/vcf303.toml ──┤
                      ├── tools/circuitgen ──▶ eval/303/_*_netlist_gen.py  (SPICE)
                      │                    ──▶ eval/303/_*_gen.jl          (ACME)
                      └── (future)         ──▶ plugin303/src/dsp/...      (Rust WDF)
```

- **Never hand-edit generated files** (they carry a `DO NOT EDIT` marker). Edit the `.toml` IR, then run `python -m tools.circuitgen` to regenerate.
- The **drift guard** (`tests/circuitgen/test_no_drift.py`) catches stale generated files — it regenerates in memory and diffs against the committed copies.

### Plugin Architecture (Phase 2)

`plugin303` is a **nih-plug** VST3/CLAP instrument crate:

- **`lib.rs`** — `Plugin303` struct, implements `Plugin` / `Vst3Plugin` / `ClapPlugin`. Reads params block-rate, processes MIDI sample-accurately.
- **`params.rs`** — `Plugin303Params` with `#[derive(Params)]`. Currently only `tier: EnumParam<Tier>`.
- **`dsp/voice.rs`** — `Voice` struct: the monophonic synth voice. Owns per-tier oscillators, filters, oversamplers, and envelope. Tier-switching uses equal-power crossfade.
- **`dsp/filter.rs`** — `Filter303` trait + `Tier` enum (Low/High/Ultra).
- **`dsp/ladder.rs`** — `TptLadder`: zero-delay-feedback TPT 4-pole ladder (Low tier).
- **`dsp/ladder_wdf.rs`** — `LadderVcf`: nodal MNA diode-ladder VCF with Newton solver, active Q9 resonance (High + Ultra tiers).
- **`dsp/osc.rs`** — `Saw`: polyBLEP sawtooth oscillator (Low + High tiers).
- **`dsp/vco.rs`** — `Vco303`: 21-node nodal relaxation oscillator modeling the circuit VCO (Ultra tier). Per-note pitch LUT with process-global per-sr cache.
- **`dsp/oversample.rs`** — `Os2x` / `Os4x` / `Os8x`: polyphase FIR (Kaiser-sinc L=65), zero-alloc.
- **`dsp/bjt.rs`** — Gummel-Poon NPN/PNP BJT model.
- **`dsp/diode.rs`** — 1N4148 diode model with batched stamp.
- **`dsp/jfet.rs`** — MPF104 JFET model.
- **`dsp/expmath.rs`** — Fast `exp`/`log`/`pow` for the Newton solver hot path.
- **`dsp/linalg.rs` / `linalg_static.rs`** — Generic + static-size LU decomposition.
- **`dsp/adaa.rs`** — First-order ADAA (antiderivative antialiasing) via `ln(cosh)`.
- **`dsp/env.rs`** — Linear AD (attack-decay) envelope.
- **`bin/render303.rs`** — Headless CLI renderer for offline eval / gold generation.

### Fidelity Tiers

| Tier | VCO | VCF | OS | Cost | Status |
|------|-----|-----|----|------|--------|
| **Low** | polyBLEP saw | TPT ladder | None | Cheapest | RT-viable, ships |
| **High** | polyBLEP saw | Nodal diode-ladder (`LadderVcf`) | 2× | ~42% core | RT-viable |
| **Ultra** | Circuit VCO (`Vco303`) | Nodal diode-ladder (`LadderVcf`) | 4× | ~147% core | Offline-grade per voice |

- Oversampling is **tier-intrinsic** (not a separate param). Each tier's ladder is calibrated to its OS rate.
- Tier switches use a **10 ms equal-power crossfade** (click-free). Mid-fade changes are queued (no clobbering).
- **PDC** is constant across tiers — every tier is padded to the max OS group delay (Ultra's 4×), so the host never needs to re-init its graph.

---

## Coding Conventions

### Rust (plugin303)

- **Zero-alloc audio path.** All DSP in `process()` must be allocation-free. The `assert_no_alloc` crate gates this in tests (`tests/no_alloc.rs`). Never allocate on the audio thread.
- **`reset()` must not allocate.** nih-plug may call `reset()` from the audio thread. Use in-place state clearing, never `Voice::new()` (which would rebuild the VCO pitch LUT).
- **`initialize()` is non-RT.** Expensive one-time work (e.g. VCO pitch LUT build) goes in `initialize`, never in `process` or `reset`.
- **Block-rate param reads.** Read nih-plug params once per block in `process()`, not per sample.
- **Sample-accurate MIDI.** Process MIDI events at their exact sample offset (the event loop in `process()`).
- **`f32` for audio, `f64` for the Newton solver.** The nodal ladder/VCO solvers run `f64` internally for convergence stability; the audio path is `f32`.
- **Inline hot functions.** Use `#[inline]` on per-sample DSP functions (`tick`, `stamp`, fast math).
- **`nih_plug::prelude::Enum` derive.** Use `use nih_plug::prelude::Enum;` + `#[derive(Enum)]`. The path-form derive does NOT emit the impl.
- **Edition 2021**, resolver 2. Release profile: `lto = "fat"`, `codegen-units = 1`, `opt-level = 3`.
- **x86-64-v3 baseline** for shipped x86_64 builds (AVX2+FMA, Haswell 2013+). Set in `.cargo/config.toml`, scoped to `cfg(target_arch = "x86_64")` so aarch64 is unaffected.
- **Profiling profiles** (`profiling`, `profiling-thin`) with debuginfo + frame pointers for performance work.

### Python (tools + eval)

- **`uv run --no-project`** is the standard invocation — avoids resolving the heavy project deps for stdlib-only tools like circuitgen.
- **circuitgen is stdlib-only** (`tomllib`). Eval scripts use `--with PySpice --with numpy`.
- **ruff** for linting: `line-length = 100`, `target-version = "py312"`, select `E,F,I,B,UP,SIM,RUF`.
- **pytest** with `--strict-markers`, `-ra`, `pythonpath = ["."]`.

### Circuit IR (`circuit/*.toml`)

- **Structural validation** is strict — `ir.py:validate()` rejects malformed IRs at load time, not silently downstream.
- The **`wdf_drop` flag** must be set on any two-terminal element where both nodes are "known" (both terminals on known/supply nodes). This prevents silent SPICE↔WDF drift: the WDF topology omits such elements, SPICE/ACME keep them.
- **`meta.num_nodes`** in the TOML must match the actual node count.
- **GND** is the implicit reference — never list it as a node.

---

## Testing Strategy

### Rust (`cargo test -p plugin303`)

The test suite is the primary quality gate. ~65 tests pass, ~8 ignored (diagnostics).

| Test file | Purpose |
|-----------|---------|
| `ultra_spectral.rs` | **Binding spectral gate** — Welch LSD ≤ 4.0, corr ≥ 0.95 vs SPICE gold |
| `high_spectral.rs` | High-tier spectral gate — LSD ≤ 29, corr ≥ 0.95 |
| `voice_esr.rs` | Low-tier ESR regression guard (budget 3.5 dB) |
| `tier_levels.rs` | Per-tier level parity |
| `tier_switch.rs` | Click-free crossfade: switch-window step ≤ 2.5× steady |
| `vcf_resonance.rs` | Active Q9 reso peak frequency tracking vs ngspice fixture |
| `vco_pitch.rs` | VCO per-note pitch accuracy (< 0.3% at 110/220/440 Hz) |
| `no_alloc.rs` | Zero-allocation gate on the audio path (all tiers) |
| `ultra_stability.rs` | NaN/bounded sweep guard for Ultra |
| `newton_iters.rs` | Newton convergence budget for the nodal solver |
| `ladder_alias.rs` | Oversampled ladder anti-aliasing verification |
| `vcf303_fit.rs` | TPT ladder fit to ngspice gold |
| `params_tier.rs` | Tier enum nih-plug param wiring |
| `ultra_esr.rs` | **(#[ignore])** Informational — sample-exact ESR vs ngspice is NOT the gate |

- Tests starting with `#[ignore]` are **diagnostics**, not gates. They exist for investigative sweeps and are not expected to pass as acceptance criteria.
- **Gold files** live in `eval/303/gold/`. Regenerate with `eval/303/gen_gold.py`.
- Spectral test helpers are shared in `tests/common/mod.rs`.

### Python (`tests/circuitgen/`)

- **`test_no_drift.py`** — drift guard: committed generated backends must match what the IR would regenerate. Fix with `python -m tools.circuitgen`.
- Every generated file must carry the `DO NOT EDIT` marker.
- The registry must be non-empty (guards against accidental wipe).
- Each IR file must pass structural validation.

### Python (`eval/303/test_*.py`)

- `test_esr.py` — ESR metric unit tests.
- `test_voice_ref.py` — SPICE voice reference pipeline tests.
- `test_reso_gold.py` — ngspice reso-peak fixture tests.

---

## Critical Gotchas (Don't Repeat These)

### DSP / Plugin

1. **VCF corner has a ~600 Hz structural ceiling at 48 kHz** (passive ladder only). Raising `LadderVcf::set_cal` past ~1.0 distorts the spectral envelope (LSD optimum is at 1.0). Don't chase corner via bias; it's the active stages that clear it.

2. **VCO pitch cal `CV_FIT` is fit to THIS solver** (not SPICE). Re-run `characterize_freq_cv` if the VCO solver changes.

3. **Sample-exact ESR vs ngspice is NOT the acceptance gate** for Ultra. It's structurally unreachable for a real-time bespoke chain vs ngspice's adaptive solver. The gate is **spectral** (Welch LSD + dB-spectrum correlation).

4. **Calibrating Low/TPT cutoff/reso to SPICE is a wash** — the gap is topological (polyBLEP + TPT vs circuit saw + diode ladder), not parametric. Low's gate is a regression guard, not a fidelity target.

5. **Ultra is ~147% of one core** (offline-grade per voice). High is ~42% (RT-viable). If Ultra-at-RT is ever required, the active stages could ship High-only.

6. **ADAA "at 2×" test ≈ 0 dB** (the 2× FIR already removes that alias). Test ADAA at 1× near Nyquist/2 (f0=11 kHz).

7. **Oversampler FIR: L≡1 mod 4** for integer per-stage latency. Cascade stages process sequentially (single inner `Os2x`). Decimator reads even phase (`hist[k+1]`).

### Python / ngspice

8. **PySpice `.simulator` DOES drive ngspice-46 for `.tran`** (VCO self-starts with ramped rails). VCF transient uses subprocess ngspice + inline PWL drive.

9. **PySpice can't drive ngspice-46 for AC analysis** — must use subprocess + stamp + parse `wrdata`.

10. **ngspice unit suffixes** parse as atto (e.g. `100m` = `100e-18`). Use plain numbers.

11. **First SPICE deck line is eaten as a title** — always start with `* comment`.

12. **`np.ptp(x)` not `ndarray.ptp()`** — the method form is deprecated/removed in NumPy 2.

13. **RMS-normalise both signals before ESR** — `metrics()` has no gain normalization.

14. **Secant `solve_cv`** must match the Rust `midi_to_freq` pitch before ESR lag-align — pitch mismatch cannot be fixed by ESR lag-align alone.

### Build / Architecture

15. The design's **"WDF" label is wrong** — the nodal filter is MNA + Newton, not wave-digital. Historical naming in docs/tiers uses "WDF" but the code is nodal MNA.

16. **ACME parity = NO-GO** for both 303 circuits. ngspice is the ground truth. ACME's homotopy solver cannot handle the current-source-biased topologies.

---

## Build & Deploy

### Local (macOS Apple Silicon)

```bash
bash scripts/build-macos-arm64.sh
# Builds, bundles, installs to ~/Library/Audio/Plug-Ins/{VST3,CLAP}/
# Ad-hoc codesigns for Gatekeeper
```

### Local (Linux x86_64)

```bash
bash scripts/build-linux-x86_64-v3.sh
# Builds with -C target-cpu=x86-64-v3 (AVX2), installs to ~/.vst3/ and ~/.clap/
```

### CI (GitHub Actions)

Two manual-dispatch workflows in `.github/workflows/`:

- **`build.yml`** — linux-x86_64-v3 (native) + macos-arm64 (cross via zig/cargo-zigbuild), optional GitHub Release.
- **`build-macos.yml`** — macOS arm64 cross-compile only (macOS SDK fetch + cargo shim for nih_plug_xtask).

Both use self-hosted linux x64 runners. Release tag defaults to `build-<sha8>`.

### Plugin Bundle

```bash
cargo xtask bundle plugin303 --release
# Output: target/bundled/TB303.vst3 and target/bundled/TB303.clap
```

Bundle name is **`TB303`** (set in `bundler.toml` and the plugin metadata), not `plugin303`.

---

## Design Documents

All design and implementation plans live in `docs/plans/`:

- `2026-06-26-phase2-303-voice-design.md` — Phase 2 voice design.
- `2026-06-26-phase2-303-voice-implementation-plan.md` — Phase 2 implementation plan (M1–M8 milestones).
- `2026-06-27-303-fidelity-tiers-design.md` — Fidelity tiers design (supersedes M-milestones).
- `2026-06-27-303-fidelity-tiers-implementation.md` — F1–F5 implementation plan.
- `2026-06-27-F6-ultra-active-stages.md` — F6 Ultra active-stage headroom plan.

The handoff document is `HANDOFF.md` (root) — the definitive status snapshot. Use `superpowers:executing-plans` to continue.

---

## Current Status & Next Steps

**F1–F6 DONE.** The three-tier fidelity axis is complete, gated, click-free, PDC-reported, with active Q9 resonance + per-note VCO pitch.

Remaining work:
1. **F6.9 DAW smoke-test** — manual, human-in-the-loop (`docs/DAW_SMOKE_TEST.md`).
2. **Accent / glide / decay levers** (M4/M6, a separate axis from fidelity tiers).
3. **GUI** (nih_plug_egui, behind a `gui` feature gate — not yet started).
4. **Ship** — finish-branch, PR, iterate.
