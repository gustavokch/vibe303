# feat(voicing): implement TB-303 accent, pitch glide, and MIDI legato slide

## Overview
This PR implements **Phase 3: Sequencer-Driven Voicing Features** (specifically the Accent, Glide/Slide, and Decay systems) for the Roland TB-303 voice emulation. It also includes critical optimization changes to the VCO LUT caching and calculation to prevent testing bottlenecks.

## Changes
- **Accent Accumulator and Modulation:**
  - Implemented the leaky integrator accent accumulator that decays with a 200 ms time constant and charges on trigged accents, stacking up to a maximum of 1.8.
  - Guided modulation routes for cutoff boost (`+3000 Hz * accent_acc * accent_knob`), VCF envelope decay reduction (clamping to a minimum of 200 ms), and VCA boost (implemented in the voice engine).
- **Pitch Glide Slew:**
  - Moved the glide slew logic into the `Voice` struct (log-frequency semitone space).
  - Modeled the slew using an exponential decay curve with a time constant mapping the 0.0-1.0 knob range to 10 ms - 410 ms.
- **Sequencer/MIDI Integration:**
  - Sequencer slide triggers are delegated directly to the voice engine using the `slide` parameter of the steps.
  - Implemented sample-accurate MIDI legato slide detection (overlapping MIDI NoteOn events trigger pitch slide without retriggering envelopes).
- **VCO LUT Caching & Deferral Optimizations:**
  - Fixed a massive test suite bottleneck where `Voice::new` synchronously built the 1,078-entry pitch lookup table twice on every single test process.
  - Optimized the cache retrieval and deferred VCO LUT recalculations until the voice is actually switched to `Tier::Ultra` (the only tier that uses the circuit-grounded `Vco303`).
  - Reduced test suite execution times from **10+ minutes to ~2 minutes**.
