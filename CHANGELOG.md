# Changelog

All notable changes to vibe303 are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - Unreleased

### Added
- Windows `x86_64` VST3 + CLAP build targets. New `build-plugins-windows`
  GitHub Actions workflow builds on a hosted `windows-latest` runner in two CPU
  variants (`x86-64-v3` AVX2, `x86-64-v2` compatibility), and
  `scripts/build-windows-x86_64.ps1` builds and installs the same bundles
  locally. Windows bundles ship with the GUI but without the standalone binary.
- Phase 1 — TB-303 analog circuit model: `circuit/vco303.toml`, `circuit/vcf303.toml`,
  emitted to SPICE + ACME via `tools/circuitgen` and ngspice-validated under `eval/303/`
  (VCO 1 V/oct; VCF 4-pole LP, bias-tracking cutoff, active-Q9 resonance, tap-select poles).
- Phase 2 — TB-303 nih-plug voice design + implementation plan under `docs/plans/`.

### Changed
- Quality tiers renamed in fidelity order: Draft (was Low), Low (was High),
  High (was Ultra 2x), Best (was Ultra). The GUI QUALITY switch, the host
  parameter value names, `render303 --tier` and `profile_voice --tier` use the
  new names; `--tier` takes names only (`draft|low|high|best`), not indices.
  The QA state `quality_high` now selects the High tier; the capture suite uses
  `quality_low` for what `quality_high` captured before. Sessions saved before
  this change: a saved Ultra (index 2) now loads as High; a saved Ultra 2x
  (index 3) now loads as Best. DAW automation lanes that stored index 2 or 3
  change the same way.
- `eval/303/calibrate_vcf.py` fits the Draft and Low control maps against Best
  and prints the `DRAFT_*_POLY` / `LOW_*_POLY` constants for `voice.rs`. Its
  probe binary `vcf_sweep_helper` is restored and builds each tier's filter the
  way the voice does (oversampling factor and brightness calibration).
- The Draft and Low tiers run their filter at 4x oversampling, like Best
  (was 1x and 2x). Draft moves from 46.39 to 41.58 dB LSD against the SPICE
  gold. Low's brightness is re-tuned for 4x from 0.50 to 0.45, which moves
  Low from 28.62 to 12.47 dB LSD; the Low gate budget tightens from 29 to
  16 dB. Draft and Low match Best's level through a resonance-dependent
  output trim, applied before the output clipper (median 0.0 dB vs Best at
  every resonance point). Draft costs about 1.5% and Low about 8% of one core
  per voice at 48 kHz (was 0.9% and 4.5%).
- Draft's cutoff and resonance maps (`DRAFT_FC_POLY`, `DRAFT_RESO_POLY`) are
  re-fitted against Best at its calibrated brightness (0.70) and at Draft's 4x
  filter rate; they were fitted against Best at brightness 1.0 and Draft at
  1x. Draft-to-Best parity moves from 25.87 (Draft at 4x) to 12.37 dB LSD (corr
  0.965 to 0.982), and Draft against the SPICE gold from 41.58 to 33.62 dB (corr 0.935
  to 0.959). `DRAFT_LEVEL_TRIM` is re-derived, so Draft's median level again
  matches Best at every resonance point; the spread around the median grows
  (41 of 80 settings within ±1.5 dB of Best before the output clipper, was
  57). The maps are evaluated inside the fitted cutoff range (200–4000 Hz) and
  extended by octaves above and below it, so high cutoffs do not drive Draft into
  self-oscillation. `eval/303/calibrate_vcf.py` takes `--tiers` to fit one tier.
- Editor (round-20 GUI audit): on/off controls (RUN, SYNC, MIDI, SYNTH,
  JITTER, ADV ENV, LINK, ON, SLD CHAIN) are raised tiles with a small LED that
  lights orange; the tile no longer fills orange, and RUN, SYNC, MIDI and SYNTH
  keep one label in both states. GENERATE is a raised action, not a lit one.
  The playhead plate shows only while the sequencer runs, and the step LED is
  4 pt tall. CLEAR asks for a second click within 3 s before it wipes the
  pattern. Knobs show their parameter's own value text, the same string the
  host shows; CUTOFF reads the base cutoff and GLIDE the glide time the voice
  uses. The host shows DECAY in ms/s, the voice levels in dB and the 0-1
  amounts in percent. Disabled controls keep readable text and say on hover
  why they are disabled (ACCURATE, ADV ENV off, clipper off); abbreviated
  controls in SOUPED-UP ENGINE, OUTPUT CLIPPER, ANALOG TWEAKS and the
  generator have tooltips. In ACCURATE, ADV ENV and both envelope banks are
  disabled, ANALOG TWEAKS shows `FUN MODE ONLY` and hides CALIBRATE.
- ANALOG TWEAKS CALIBRATE is a manual trim. AGE, TOL and CMP SEED now change
  the components at once in every tier, and the circuit VCOs keep the pitch
  trim of the last CALIBRATE, so the component pitch error is audible (and the
  status line reads `SYNC PENDING`) until you press CALIBRATE. CALIBRATE
  computes the new trim in the background (`CALIBRATING`, about 20-40 s at
  48 kHz) and RECAL sets the share of the error it removes. The trim is saved
  with the project. Before, every tweak started the trim on its own and the
  change was silent in High and Best until it finished. Sessions saved before
  this change load with the factory trim: press CALIBRATE once to trim worn
  components again.
- The analog tweaks act in FUN only, as the editor says: ACCURATE plays
  nominal components with no drift and no added heat.
- AGE darkens the Draft tier like the ladder tiers (Draft scaled its cutoff up
  with age). RECAL no longer changes the filter: it only scales the VCO pitch
  trim.
- NOISE sets the ambient temperature at once (20 °C at 0 to 40 °C at 100);
  only gate self-heating is slow. The host shows that temperature (was 20 to
  45 °C) and shows AGE in the years the editor shows (was 0.45 x the value).

### Fixed
- The High tier was padded for Best's 4x oversampling latency instead of its
  own 2x latency, so its output came out 15 samples early (about 0.3 ms at
  48 kHz) and a High crossfade blended misaligned paths. High now has its own
  compensation delay, and every tier reaches the output at the reported
  latency.
- A crossfade between tiers that shared DSP state ticked that state twice per
  sample for the 10 ms fade. High <-> Best ran the circuit VCO at double rate
  (an audible pitch jump, then a phase offset); Low <-> High and the circuit
  tiers also shared the 2x oversampler and the sub oscillator. Each tier now
  owns its oversampler and nodal ladder, and the circuit VCOs and sub
  oscillator tick once per sample, also during a High <-> Best fade.
- Switching back to a tier resumed the filter, oversampler and delay state it
  held when it last ran, often from another note. With resonance up, the
  nodal ladder took about a second to forget it, so the returning tier sounded
  wrong for that time. The incoming tier now starts from a cleared state on
  every switch. At resonance 0.3 its error 100 ms after the fade drops from
  0.67 to 0.09 relative RMS.
- The High and Best tiers played their VCF at brightness 1.0 instead of the
  calibrated 0.70, and the Low tier lost its 0.50 brightness on the plugin's
  first block: analog wear (`LadderVcf::set_analog_wear`) replaced the tier
  brightness instead of scaling it. Wear now scales it. Against the SPICE gold,
  Best's brightness-only render moves from 9.59 to 3.15 dB LSD; the full default
  patch moves from 29.61 to 22.24 dB. High's full default patch moves from 28.98
  to 21.58 dB. The rest of the default patch's distance is its Env Mod (0.5),
  which sweeps the cutoff that the gold holds still; with Env Mod at 0 the
  default patch measures 3.50 dB for Best. Low, High and Best sound darker than
  before (in the plugin, Low's VCF brightness goes from 1.0 to 0.50 and
  High/Best from 1.0 to 0.70); sessions keep their settings. Draft-to-Best
  parity moves apart (20.41 to 24.81 dB LSD), because Best is darker.
- Sequencer notes ignored the DECAY curve that MIDI notes use. The sequencer
  mapped the knob linearly to 0-3000 ms for the classic filter envelope, so
  DECAY at 0.5 gave 1500 ms while the knob read 400 ms. Unaccented sequencer
  notes now use the MIDI curve (100 ms to 2.1 s, the value the DECAY readout
  shows). Above DECAY 0.04 the classic filter envelope of sequencer patterns
  is shorter than before (0.2: 200 ms, was 600 ms; 0.5: 400 ms, was 1500 ms;
  1.0: 2.1 s, was 3 s); below it, it is longer (0.01: 105 ms, was 30 ms).
  Accented notes keep their fixed 200 ms.
- The macOS standalone binary (`cargo run --release --bin standalone`) panicked
  on startup when the output device's nominal sample rate was below 48 kHz
  (e.g. 44.1 kHz on built-in MacBook speakers: `Received 558 samples, while the
  configured buffer size is 512`). `cpal`'s CoreAudio output backend does not
  switch the hardware sample rate, so CoreAudio resampled 512 device frames at
  44.1 kHz into ceil(512 * 48000 / 44100) = 558 callback frames,
  exceeding nih-plug's preallocated `period_size` buffer. `standalone` now
  passes the output device's current sample rate (the `--output-device`
  device, or the system default) when the command line does not set
  `-r`/`--sample-rate`.
- CALIBRATE locked up the plugin. It set a host parameter that nothing reset,
  and `process()` ran the full VCO pitch-LUT solve (about 30 s) on the audio
  thread on every block while it stayed set. The `calibrate_sync` parameter is
  removed: CALIBRATE only stores the trim settings, and the LUT worker computes
  the trim off the audio thread. The first block in FUN also ran a 20 s LUT
  build on the audio thread when the FUN pitch LUT was not cached yet;
  `initialize` now builds it.
- DRIFT was nearly inaudible: its random walk had an RMS of 0.02 at 48 kHz
  (0.013 at 96 kHz) instead of about 0.33, so DRIFT at 100 % moved the pitch by
  less than 0.4 cent. The walk is now normalized at every sample rate: at
  100 % the pitch wanders about 4 cents RMS (peaks near 12 cents, 8 cents
  peak-to-peak in 3 s), the cutoff about 3 % RMS and the resonance 1 % RMS.
- High and Best detuned with temperature (about 5 cents per 1.2 °C: full gate
  self-heating moved the pitch about 17 cents, and NOISE 100 about 100 cents)
  because the nominal pitch LUT ignored the device temperature. Their LUTs now
  trim the device temperature out, and every tier gets the same small modelled
  thermal detune (-8 cents at NOISE 100).
- The LUT worker allocated on the audio thread (a boxed callback per request).
  Requests are now plain data and allocation-free.

### Removed
- The editor's ZOOM control and the `zoom75` QA capture state. nih-plug cannot
  resize the plugin window from the editor, so a smaller zoom left an empty
  band; keyboard zoom is disabled for the same reason.
- The BA283 Neve-preamp product (plugin crate, WaveNet/LSTM model, WDF engine, training
  and data-generation pipelines, colab notebooks, datasets, ba283 eval suites and docs).
  The repo is now the standalone 303 project; the shared `circuitgen` circuit-IR pipeline
  and ngspice/ACME validation harness were kept. Removed code remains in git history.
