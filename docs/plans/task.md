# TODO List: Expose 303 Panel Parameters & Sequencer

- [x] Define all classic and souped-up parameters in `params.rs`
- [x] Add `set_poles` to `Filter303` trait and implement it for `TptLadder` and `LadderVcf`
- [x] Implement PolyBLEP Square and Sweep morphing in `osc.rs` (`BlepOsc`)
- [ ] Update `Vco303` to support native circuit Pulse/Square output and Sweep morphing
- [ ] Create `DriftGenerator` in `dsp/drift.rs`
- [ ] Create/Implement the 16-step sequencer in `dsp/sequencer.rs`
- [ ] Update `Voice` in `dsp/voice.rs` (dual oscillators, sub-oscillator, drift/age modulations, drive, accent, and glide)
- [ ] Wire block-rate parameter updates in `lib.rs`
- [ ] Write unit/integration tests and verify they pass
