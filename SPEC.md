# SPEC — Probabilistic Acid Euclidean Sequence Generator

## §G
Generative 303 sequence engine with Euclidean rhythm, scale walk, acid decoration, loop mutation, GUI panel, CLI tool.

## §C
- Thread safety: no audio thread locking or race condition on GUI pattern update; use lock-free/atomic swap payload.
- Zero audio dropouts during sequence generation or live mutation.
- Strict 303 gate semantics: tie-into-rest extends gate duration without envelope retrigger.
- Deterministic output: fixed seed must produce identical pattern.
- Pure Rust DSP implementation inside `plugin303`.

## §I
- `plugin303::dsp::generator`: `MusicalScale`, `GeneratorParams`, `AcidGenerator`.
- `plugin303::dsp::sequencer::Sequencer`: `set_pattern` / `mutate_loop` integration hooks.
- `plugin303::gui::panels::generator_panel`: GUI panel with knobs, sliders, action buttons.
- CLI binary: `plugin303/src/bin/gen_acid.rs` (`cargo run --package plugin303 --bin gen_acid`).

## §V
- V1: Bjorklund algorithm distributes `k` pulses evenly across `n` steps ($1 \le k \le n \le 16$) with `rhythm_shift` rotation.
- V2: `AcidGenerator::generate` output is 100% deterministic for given `GeneratorParams` seed.
- V3: `MusicalScale` note quantization strictly enforces selected scale intervals and `[octave_min, octave_max]` bounds.
- V4: `accent_pitch_bias > 0.0` increases scale degree or octave leap probability on accented steps.
- V5: `slide_chaining == false` suppresses slide flag if immediately preceding step slided.
- V6: Slide into rest entry constructs `TimeEntry::Tie` extending previous step gate without retriggering envelope generator.
- V7: Loop wrap step 0 mutation applies `mutation_rate` probability with deterministic PRNG step state (`base_seed + loop_count`).
- V8: GUI pattern updates to audio thread sequencer use lock-free payload exchange without audio glitch or data race.

## §T
|id|status|desc|cites|
|---|---|---|---|
|T1|x|impl `MusicalScale`, `GeneratorParams`, Bjorklund Euclidean pulse solver in `plugin303/src/dsp/generator.rs`|V1,I|
|T2|x|impl weighted scale walk, octave bounds, accent/slide acid decorator stage in `AcidGenerator::generate`|V2,V3,V4,V5,V6,I|
|T3|x|impl loop mutation logic `AcidGenerator::mutate` with deterministic seed stepping|V7,I|
|T4|x|integrate lock-free pattern swap and loop wrap mutation hook in `Sequencer`|V6,V7,V8,I|
|T5|x|impl GUI panel `plugin303/src/gui/panels/generator_panel.rs` with parameters, buttons, wire into GUI root|V8,I|
|T6|x|impl CLI binary `plugin303/src/bin/gen_acid.rs` for ASCII grid display, JSON pattern dump|I|
|T7|x|add unit, determinism, property, integration tests in `plugin303/tests/generator.rs`|V1,V2,V3,V4,V5,V6,V7,V8|

## §B
|id|date|cause|fix|
|---|---|---|---|
