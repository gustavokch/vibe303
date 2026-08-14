# Accurately Modelling the TB-303 Sequencer in a Rust nih-plug Emulation

**A research report covering the original hardware's sequencer architecture and measured timing behaviour, the slide/accent/gate signal semantics, an analysis of the open-source jc303 (Open303) emulation, and a concrete design + reference implementation for a Rust/nih-plug model.**

---

## 1. Executive summary

The TB-303's sequencer is not an idealised 16-step, 16th-note grid. It is a 4-bit mask-ROM CPU (NEC µPD650C-133) driven by two unsynchronised clocks — a 24 PPQN DIN-sync tempo clock and a fixed ~1.8 ms interrupt oscillator — whose outputs (pitch CV latch, gate, accent, slide) show measurable latency, jitter, and duty-cycle drift that are part of the instrument's feel.[^1] Its slide and accent behaviour follow precise, well-documented digital rules that drive simple but characterful analog circuits (an RC slew for pitch, an accent envelope that builds up over consecutive accents).[^2][^7]

The open-source jc303 plugin is a JUCE port of Robin Schmidt's Open303 DSP engine. It contains a compact `AcidSequencer` that models step scheduling with sample-accurate drift compensation and the slide/gate lookahead logic, but it omits triplet mode, the CPU timing imperfections, and the hardware's decoupled pitch/timing memory; the jc303 plugin itself is MIDI-driven (accent = velocity ≥ 100, slide = legato overlap) and treats a faithful internal step sequencer as roadmap work.[^5][^8]

For your Rust nih-plug model, the recommended approach is:

1. **Separate the sequencer core from the DSP** as a pure, unit-testable module that consumes a clock and emits timestamped events (`Trigger`, `SlideTo`, `NoteOff`, accent state).
2. **Schedule sample-accurately** using an integer sample countdown with Bresenham-style fractional drift compensation (the same technique jc303 uses), or a beat-anchored absolute-position model when host-synced.
3. **Model the hardware semantics exactly**: 6 clocks/step with 3-on/3-off gate in 4/4 (5-on/3-off over 8 clocks in triplet mode), gate extension through slides, one-step lookahead, whole-step accent latching, slide active through ties and rests, ~60 ms constant-time exponential pitch slew.
4. **Optionally model the CPU timing imperfections** by quantising gate edges to a free-running 1.8 ms "interrupt" grid plus measured processing latencies — this reproduces the beating/jitter behaviour with two lines of code rather than a statistical fudge.
5. **Integrate with nih-plug** via `context.transport()` for host sync, `context.next_event()` for MIDI input, sub-block event splitting in `process()`, lock-free pattern transfer from the editor thread, and `Smoother` for DSP parameters.

Sections 2–4 give the research basis; section 5 is the nih-plug design; section 6 is a working Rust reference implementation; section 7 is a validation plan.

---

## 2. Hardware anatomy of the TB-303 sequencer

### 2.1 The digital section

| Component | Part | Role |
|---|---|---|
| CPU | NEC µPD650C-133 (mask-ROM, CMOS µCOM-43 family, max 440 kHz clock) | Runs the sequencer firmware; all gate/accent/slide signals originate here.[^1][^3] |
| Pattern RAM | 3 × NEC µPD444 (1K × 4-bit) = 3072 nibbles (1536 bytes) | Stores patterns: pitch, timing, accent/slide attributes across the 303's pattern memory.[^3] |
| Pitch DAC | Discrete R-2R resistor ladder | Converts the note value to pitch CV; the DAC input bits are **latched by the slide signal** through buffer ICs.[^1] |
| Accent latch | Flip-flop (IC13) | Latches the CPU's accent output so the accent signal spans exactly one whole step, synchronised to the gate.[^1] |
| Tempo clock | Internal oscillator or DIN-sync input (jack switch selects) | 24 PPQN (24 clocks per quarter note), per the DIN-sync specification also used by the TR-808.[^1] |
| Interrupt clock | External oscillator | Fires the CPU interrupt every **1.8 ms**; panel scanning and — critically — tempo-clock processing happen inside the ISR.[^1] |

Two facts in that table explain almost everything about the 303's timing character:

* **The tempo clock is processed inside the interrupt service routine.** The CPU only "sees" a clock edge at the next 1.8 ms interrupt tick, and then takes roughly 1 ms of ISR work to raise the gate. The two clocks (tempo clock and interrupt clock) are completely unsynchronised, so their relative phase drifts continuously — a beating pattern.[^1]
* **The note/accent data is latched by the slide line.** A new pitch or accent value cannot reach the analog section without a brief slide-signal pulse; even on non-slid notes the slide circuitry pulses for about **44 µs** at the start of each new note to latch the DAC. During slides spanning multiple different notes, the slide line dips low for **8.75 µs** at each new note to latch the next value.[^1] (These microsecond pulses are below audibility, but they document why the slide signal is high at the start of *every* note's CV update.)

### 2.2 Measured clock and gate timing (logic-analyzer data)

Julian Schmidt's 2017 analysis of an original µPD650C-133 (performed for the RE-303 replica CPU project) is the authoritative measurement set:[^1]

**Latencies**

| Measurement | Value |
|---|---|
| Gate-on delay after tempo-clock rising edge | 0.8–3.3 ms, mean **2.06 ms**, σ ≈ 0.74 ms |
| Gate-off delay after tempo-clock falling edge | 2.25–5.45 ms, mean **3.51 ms**, σ ≈ 0.75 ms |
| Gate-on delay after interrupt-clock edge | **0.917–1.062 ms** (very consistent) |
| Gate-off delay after interrupt-clock edge | **~2.3–2.4 ms** |

The much tighter distribution relative to the *interrupt* clock confirms the ISR model: gate edges are quantised to the interrupt grid, then delayed by roughly constant ISR processing times (~1 ms for gate-on, ~2.35 ms for gate-off). The variable latency relative to the tempo clock is simply the 0–1.8 ms wait for the next interrupt.

**Duty cycle**

* 4/4 mode: a step is 6 clock pulses at 24 PPQN; the nominal gate is **3 clocks on / 3 clocks off = 50% duty**. Measured duty cycles vary between **49.96% and 55.8%** because gate-on and gate-off latencies differ and both ride on the interrupt beating. At slow tempos the variation is ~50.25–50.5%; at the fastest tempos (step ≈ 50 ms) the fixed ~2.4 ms gate-off latency pushes duty to 55–56%, which is audible.[^1]
* Triplet mode: a step is **8 clock pulses; gate is 5 on / 3 off = 62.5% duty** — an asymmetric ratio that gives triplets a noticeably more legato feel.[^1]

An earlier (1999) measurement by Robin Whittle described the gate as going high at clock-pulse 0's rising edge and low halfway through pulse 3 (3.5 on / 2.5 off ≈ 58%).[^2] Schmidt's later logic-analyzer data (3/3 with interrupt-induced variation) is the more careful measurement and is what the RE-303 CPU implements; treat 50% as nominal and the 49.96–55.8% spread as the authentic variation envelope.

**Beating and tempo "sweet spots"**

Because the interrupt clock is fixed at 1.8 ms, the beating pattern between it and the tempo clock depends on tempo. When the step duration is an integer multiple of 1.8 ms there is *no* beating — Schmidt's example: 278 interrupt clocks × 1.8 ms per step ⇔ **119.90 BPM**.[^1] At other tempos a 16th-note pattern exhibits a slow, audible "beating" of note accentuation — a documented part of the hardware's feel.

### 2.3 Pattern memory organisation

The 303 stores up to 16 steps per pattern (shorter patterns possible), and — crucially for fidelity — stores **pitch and timing as separate, decoupled sequences**:[^2][^4]

* **Timing mode**: each step is entered as one of *note*, *tie*, or *rest*.
* **Pitch mode**: a sequence of pitches (key + octave up/down transposition), each carrying an **accent** and a **slide** attribute.

At playback, the sequencer only consumes the next entry from the *pitch* queue when the timing track says "note". Rests do not advance the pitch queue; ties extend the current pitch. Consequently a pitch sequence of C-D-E-F with timing Note-Note-Rest-Note plays C-D-∅-E, **not** C-D-∅-F, and accent/slide attributes travel with their pitch entry, not with a fixed step index.[^4] jc303's simplified per-step model (section 4) collapses these two tracks into one; section 5.3 shows how to keep them decoupled in Rust.

---

## 3. Sequencer semantics: gate, slide, accent

### 3.1 Step and gate model

* Internal resolution: 24 PPQN → 6 clocks per 16th step (4/4), 8 clocks per step (triplet).[^1]
* Gate rises at the start of clock 0 of the step; falls after 3 clocks (4/4) or 5 clocks (triplet), subject to the interrupt quantisation of section 2.2.[^1]
* A step programmed as a *rest* produces no gate. A *tie* continues the previous gate.

### 3.2 Slide semantics

Whittle's 1999 investigation (verified by Schmidt's measurements) gives the exact algorithm:[^1][^2]

1. A **slide attribute programmed on note B** ties B into the next note C: B's gate does **not** turn off at the end of B's step; the gate stays high into C's step.
2. At the start of C's step, the CV changes to C's value — but the **slide (slew) circuit is engaged**, so the VCO pitch glides from B to C rather than jumping. The slide signal goes high at the beginning of the slid-into step and stays high for that step's entire on/off cycle.
3. If the slid note is longer than one step, the slide line goes high one 16th after the slid note starts (no audible effect — the CV is already there) and remains active until the end of the following note.
4. Quirk: the slide signal is **also high during ties and the rests that follow them**, even with no slide programmed. Since no pitch change occurs, this has no audible effect, but a cycle-accurate model should reproduce it.[^1]
5. The rumour that the sequencer "anticipates" the next note and starts sliding early is false — the slide starts exactly at the slid-into note's start.[^2]

**Analog consequence (the part that actually matters sonically):** the slide circuit is an RC slew on the pitch CV — a **constant-time, exponential** glide (not constant-rate, not linear). Open303 documents the TB-303 slide time as **60 ms**, and models it with a one-pole leaky integrator whose time constant is τ = 0.2 × 60 ms = **12 ms** (i.e., the glide reaches ~63% of its interval in 12 ms and ~99.3% in 60 ms — the standard "5τ ≈ done" convention).[^5] Whether the gate is open during the tail of the slide matters: on an unaccented note the VCA envelope closes quickly and you don't hear the glide's tail; on an accented note the main envelope also drives the VCA, so more of the glide is audible.[^2]

### 3.3 Accent semantics

* The CPU's raw accent output is high for 2 of the 3 gate-on clock pulses and contains stray pulses; the IC13 flip-flop **latches** it, synchronising accent to a whole step: the accent signal is active for exactly one full gate on/off cycle, stays active across ties and consecutive accented notes, and drops at the start of the next unaccented note.[^1]
* Analog consequences (Whittle's analysis of the accent circuitry):[^7]
  1. On an accented note the **main (filter) envelope decay is shortened** — to a fixed ~200 ms on the stock 303 (Open303 uses exactly this value).
  2. The shortened filter envelope is **also routed to the VCA**, producing a two-segment amplitude shape (fast accent decay into the normal long decay) and a louder peak.
  3. An additional, gentler **"accent sweep"** envelope is summed into the filter cutoff, giving a slower re-attack/decay component.
  4. The accent sweep circuit **does not fully discharge between consecutive accents** — its level builds up over runs of accented steps, producing the rising "wow-wow-wow" sweep that is signature acid.[^4][^7]

Open303 approximates this with: accent gain scaling an extra smoothed path (`rc2`, ~3 ms smoothing) of the main envelope into the cutoff, plus `accentDecay = 200 ms` swapped in for the main envelope and a shorter amp-envelope release on accented notes. It does **not** model the charge-pump build-up of the real accent sweep capacitor — a known simplification and an opportunity for your model to be more faithful (§5.5).[^5]

### 3.4 Envelope/VCA behaviour the sequencer must drive

From Open303's stock-303 configuration (used by jc303 when "Switch Mod" is off):[^5]

* Main envelope (MEG → filter): attack ~3 ms, decay = front-panel Decay (200–2000 ms) or fixed 200 ms when accented.
* Volume envelope (VEG → VCA): very fast attack, long decay (stock ≈ 3–4 s; jc303 sets 1230 ms), short release — the "staccato" feel comes from the 50% gate duty, not from a short envelope.
* The gate stays high through slides, so neither envelope retriggers on a slid-into note; only the pitch slews (and the accent state may change).

---

## 4. Analysis of the jc303 / Open303 sequencer implementation

### 4.1 Architecture

jc303 embeds Robin Schmidt's Open303 engine, whose sequencer-relevant classes are:[^5]

```
AcidNote      { int key (0–12, 0=C); int octave; bool accent; bool slide; bool gate }
AcidPattern   { AcidNote notes[16]; int numSteps; double stepLength }
AcidSequencer { AcidPattern patterns[16]; sample-rate, bpm, countDown, step,
                driftError, running, mode ∈ {OFF, KEY_SYNC, HOST_SYNC} }
Open303       { owns AcidSequencer; consumes notes in getSample() }
```

Differences from hardware to note:

* Pitch and timing are merged: each of the 16 steps stores key+octave+accent+slide+gate directly (no separate pitch/timing queues, no tie concept — a tie would be expressed as a slide-like gate extension). This is a pragmatic simplification, not a faithful memory model.
* `stepLength` (gate duty) is a free parameter in units of a 16th note; default **0.5** (50%), matching the 4/4 hardware nominal. There is no triplet mode.
* `AcidSequencer` supports three modes, but in the jc303 plugin the sequencer modes are effectively unused — the plugin is MIDI-driven (see 4.4). The jc303 project explicitly lists a faithful internal step sequencer as future/roadmap work.[^8]

### 4.2 The scheduling algorithm (worth borrowing)

`AcidSequencer::getNote()` is called once per output sample and implements an integer countdown with **fractional drift compensation**:[^5]

```cpp
if( countDown > 0 ) { countDown--; return NULL; }
else {
  double samplesToNextStep = beatsToSeconds(0.25, bpm) * sampleRate; // 16th note
  countDown = roundToInt(samplesToNextStep);
  // compensate accumulated rounding error as soon as it exceeds half a sample
  driftError += countDown - samplesToNextStep;
  if( driftError < -0.5 )      { driftError += 1.0; countDown += 1; }
  else if( driftError >= 0.5 ) { driftError -= 1.0; countDown -= 1; }
  AcidNote* note = patterns[activePattern].getNote(step);
  step = (step+1) % patterns[activePattern].getNumSteps();
  return note;
}
```

This is a Bresenham-style integer scheduler: steps are always triggered on an exact sample index, and the long-term average step rate is exact to the double-precision tempo (no cumulative tempo drift — the classic bug of naive `countDown = round(samplesPerStep)` loops). It is also **block-size independent**, a property your Rust model must preserve.

### 4.3 Slide/gate state machine (the core logic to replicate)

In `Open303::getSample()`:[^5]

```cpp
noteOffCountDown--;
if( noteOffCountDown == 0 || sequencer.isRunning() == false )
  releaseNote(currentNote);

AcidNote *note = sequencer.getNote();
if( note != NULL && note->gate == true && currentNote != -1 ) {
  int key = note->key + 12*note->octave + currentNote;   // pattern is *relative* to held key
  if( !slideToNextNote ) triggerNote(key, note->accent);
  else                   slideToNote(key, note->accent);

  AcidNote* nextNote = sequencer.getNextScheduledNote();  // ONE-STEP LOOKAHEAD
  if( note->slide && nextNote->gate == true ) {
    noteOffCountDown = INT_MAX;          // gate extends into the next step
    slideToNextNote  = true;             // next note will be slid to, not retriggered
  } else {
    noteOffCountDown = sequencer.getStepLengthInSamples(); // 50% duty note-off
    slideToNextNote  = false;
  }
}
```

Three design elements to carry over:

1. **One-step lookahead.** Deciding whether the current gate may close requires knowing whether the next step is a note (`nextNote->gate == true`) when the current note has its slide attribute set. A slide into a rest does *not* extend the gate.
2. **Gate release is a separate countdown**, decoupled from step scheduling — the gate-off event happens mid-step (at 50% duty), while step events happen on the grid.
3. **The pattern is transposable**: stored keys are relative to a base note (`currentNote`, set by the last played key), exactly like the 303's transpose behaviour when played from MIDI.

`triggerNote()` vs `slideToNote()` differ exactly as the hardware dictates: trigger sets the oscillator frequency instantly (after resetting the slew limiter state so no glide occurs), retriggers MEG and VEG; slide only changes the target frequency (the leaky integrator slews the pitch) and swaps envelope parameters if the destination note is accented — envelopes are **not** retriggered.[^5]

### 4.4 MIDI mapping used by the jc303 plugin

Since the plugin's own UI sequencer is unfinished, jc303 maps MIDI to 303 articulations — a mapping your plugin should also support as an input mode:[^5]

* **Note-on with no note currently held** → trigger. **Note-on while a note is held (legato overlap)** → slide.
* **Velocity ≥ 100** → accented note; below → normal.
* Note-off releases only when no other note is held (a `std::list` note stack, most-recent-first; releasing back to a still-held note slides back to it).
* In the JC303 `processBlock`, audio is rendered in sub-blocks between MIDI event offsets — the same sample-accurate event-splitting pattern you'll use in nih-plug's `process()`.[^5]

### 4.5 Gap analysis — what jc303/Open303 does *not* model

| Feature | Hardware | jc303/Open303 |
|---|---|---|
| Step grid / gate duty | 6 clk/step, 3 on/3 off (~50%, measured 49.96–55.8%) | Exact 50% (parameterisable), no variation |
| Triplet mode | 8 clk/step, 5 on/3 off (62.5%) | Not implemented |
| Interrupt-clock beating/jitter | Gate edges quantised to 1.8 ms ISR grid + ~1/2.35 ms latencies | Not implemented |
| Decoupled pitch/timing memory | Yes (pitch queue advances only on note steps) | No (per-step merged data) |
| Slide-through-ties/rests quirk | Yes | Partially (slide only via explicit flags) |
| Slide slew | ~60 ms constant-time exponential | Modelled (τ = 12 ms leaky integrator) ✓ |
| Accent latch (whole step) | Yes | Effectively per-step, close enough at event level |
| Accent sweep build-up over consecutive accents | Yes (capacitor charge pump) | Not modelled (static smoothed path) |
| Accent decay 200 ms / MEG+VEG routing | Yes | Modelled ✓ |

---

## 5. Design for a Rust nih-plug model

### 5.1 Architecture overview

Keep three cleanly separated layers:

```
┌──────────────────────────────────────────────────────────────┐
│ nih-plug Plugin (audio thread, process())                    │
│  ┌───────────────┐   events (offset, SeqEvent)  ┌─────────┐  │
│  │ SequencerCore │ ───────────────────────────▶ │ DSP     │  │
│  │ (pure Rust,   │                              │ (osc,   │  │
│  │  no locks,    │ ◀──────── params ──────────  │ filter, │  │
│  │  no allocs)   │                              │ envs)   │  │
│  └──────┬────────┘                              └─────────┘  │
│         ▲ pattern snapshot (atomic swap / triple buffer)     │
└─────────┼────────────────────────────────────────────────────┘
          │
   GUI / editor thread: pattern editor, state (de)serialisation
```

* `SequencerCore` owns: pattern memory, clock state, the gate/slide/accent state machine, and the (optional) ISR-jitter model. It has **no knowledge of nih-plug** — it takes `(block_start_sample, num_samples, ClockInfo)` and writes events into a caller-provided buffer. This makes it unit-testable offline (§7).
* The DSP consumes `SeqEvent`s at exact sample offsets: `Trigger { midi_key, accent }`, `SlideTo { midi_key, accent }`, `GateOff`, `Accent(bool)`.

### 5.2 Clocking

Support two clock sources, matching the hardware's internal/external switch:

**Internal clock** (free-running, used standalone or when host transport is stopped): a persistent `f64` sample position for the next step boundary, advanced by `samples_per_step = sample_rate * 60.0 / (bpm * 4.0)` (16th notes). Keep the accumulator in `f64` beats or samples — never accumulate integer-rounded step lengths without drift compensation (jc303's `driftError` approach is the integer equivalent; an `f64` absolute anchor is simpler and equally exact over any realistic session length).

**Host-synced clock:** nih-plug's `ProcessContext` gives you `context.transport()` with `playing: bool`, `tempo: Option<f64>`, `pos_beats: Option<f64>` (position in quarter notes), and `pos_samples()`/`pos_seconds()` accessors.[^6] Derive step phase as:

```rust
let beats = transport.pos_beats().unwrap_or(0.0);     // quarter notes at block start
let step_pos = beats * 4.0;                            // 16th-note position
```

Anchor events to absolute beat positions (`event_offset = (event_step_pos - step_pos) * samples_per_beat / 4.0 …`) rather than accumulating, so loop points and transport jumps stay correct. **Caveat:** nih-plug's own docs note the sample position can be unreliable across host loop points (VST3 reports a continuous sample time that CLAP cannot reconstruct) — prefer `pos_beats` for musical anchoring.[^6] Fall back to the internal clock when `tempo`/`pos_beats` are `None` (some hosts don't report them).

**Triplet mode** changes the grid from 4 steps/beat to 8/3 steps per beat (a triplet step = 1/3 beat) and the gate duty from 3/6 to 5/8 — model it as clock-mode data, exactly like the hardware's timebase switch.

### 5.3 Data model: keep pitch and timing decoupled

To reproduce the 303's signature "pitch queue advances only on note steps" behaviour (§2.3):

```rust
pub const MAX_STEPS: usize = 16;

#[derive(Clone, Copy, Default)]
pub struct PitchEntry {
    pub key: i8,      // 0..=12 semitones above base, 0 and 12 both = C
    pub octave: i8,   // -1, 0, +1 (transpose buttons)
    pub accent: bool, // accent belongs to the PITCH entry in hardware
    pub slide: bool,  // slide ties this note INTO the next one
}

#[derive(Clone, Copy, PartialEq)]
pub enum TimeEntry { Note, Tie, Rest }

pub struct Pattern {
    pub pitches: [PitchEntry; MAX_STEPS], // consumed sequentially, only on Note steps
    pub timing: [TimeEntry; MAX_STEPS],
    pub num_steps: u8,                    // 1..=16
}
```

Playback maintains `step_idx` (timing track position) and `pitch_idx` (pitch track position, advanced only when `timing[step_idx] == Note`). This single decision reproduces the hardware's pattern semantics that trip up most emulations — including the accent/slide-travels-with-the-pitch behaviour.

If you prefer jc303's simpler per-step model as a UI compromise, you can *store* per-step data but *compile* it into the decoupled representation at edit time — the playback engine then stays faithful.

### 5.4 The state machine

Per step boundary (at clock tick 0 of the step), with one-step lookahead:

```
on_step_start(step_idx):
    match timing[step_idx]:
        Rest => { /* no gate event; gate already off or closes */ }
        Tie  => { /* gate continues; slide line state per §3.2 quirk */ }
        Note => {
            pitch_idx = advance(pitch_idx)
            p = pitches[pitch_idx]
            next_is_note = timing[(step_idx+1) % num_steps] == Note
            if prev_note.slide && prev_step_was_note_or_tie:
                emit SlideTo(p)          // envelopes NOT retriggered
            else:
                emit Trigger(p)          // MEG/VEG retrigger, pitch jumps
            accent_line = p.accent       // latched for the WHOLE step
            if p.slide && next_is_note:
                schedule no GateOff      // gate extends into next step
            else:
                schedule GateOff at step_start + gate_on_ticks
        }
```

Gate duration: `gate_on_ticks = 3` of 6 ticks (4/4) or `5` of 8 (triplet). Convert ticks to samples once per step (`samples_per_tick = samples_per_step / ticks_per_step`) and schedule the `GateOff` as an absolute sample target with the same drift-free accounting as step boundaries.

Slide handling in the DSP: maintain a pitch slew one-pole `y += (1-k)(target - y)`, `k = exp(-1/(τ·fs))` with **τ = 12 ms** (equivalent to the hardware's 60 ms constant-time slide). On `Trigger`, snap `y = target`; on `SlideTo`, only set `target`. This is precisely Open303's `LeakyIntegrator` pitch path.[^5]

### 5.5 Accent modelling

Event-level: latch the accent flag for the whole step (emit `Accent(true/false)` only at step starts; consecutive accents keep it high). DSP-level, per §3.3:

1. On accented notes, set the filter-envelope decay to a fixed **200 ms** (independent of the Decay knob).
2. Sum the (shortened) filter envelope into the VCA path with an accent-scaled gain — producing the two-segment amplitude shape.
3. Model the **accent sweep build-up** — a leaky capacitor charged on each accent onset: `sweep += (1 - sweep) * k_charge` at each accented trigger (diode charge), then `sweep *= k_leak` per sample with a slow leak (a few hundred ms); add `accent_amount * sweep` to cutoff CV. Runs of consecutive accents then build rising peaks, as measured on hardware.[^4][^7] This goes beyond Open303 and is a cheap, high-value fidelity win.

### 5.6 The CPU timing-imperfection model (optional but recommended)

Rather than white-noise jitter, emulate the mechanism — it automatically produces the correct statistics *and* the tempo-dependent beating:[^1]

```rust
// Free-running interrupt clock (hardware: 1.8 ms)
const ISR_PERIOD_S: f64 = 0.0018;
const GATE_ON_LATENCY_S: f64 = 0.0010;   // measured 0.917–1.062 ms
const GATE_OFF_LATENCY_S: f64 = 0.00235; // measured ~2.3–2.4 ms

fn quantise_to_isr(t_s: f64, isr_phase: &mut f64, latency: f64) -> f64 {
    // next interrupt tick strictly after t_s, plus ISR processing latency
    (t_s / ISR_PERIOD_S).ceil() * ISR_PERIOD_S + latency
}
```

Apply to gate-on and gate-off times only when a `hw_jitter` parameter is enabled; keep the ISR phase as a persistent free-running oscillator (do not reset it per step — the beating comes precisely from the phase drift). With jitter off, you get the mathematically exact grid. Validate against §2.2's measured distributions (gate-on mean 2.06 ms / σ 0.74 ms relative to the tempo clock; duty cycle 49.96–55.8%). Note the special case: tempos whose step length is a multiple of 1.8 ms (e.g. 119.90 BPM for 16ths) beat-free — a nice unit test.

### 5.7 nih-plug integration specifics

**Event-accurate processing.** Mirror JC303's `processBlock` structure:[^5] collect MIDI note events from `context.next_event()` (each `PluginNoteEvent` carries a `timing: u32` sample offset), merge them with internally generated sequencer events sorted by offset, and render sub-blocks between event offsets:

```rust
fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers,
           context: &mut impl ProcessContext<Self>) -> ProcessStatus {
    let transport = *context.transport();
    // 1. update clock info (tempo, pos_beats, playing) from transport
    // 2. pull note events (context.next_event()) into a small sorted list
    // 3. ask SequencerCore for events within this block
    // 4. merge-sort the two event streams by sample offset
    // 5. for each event: render audio up to event offset, apply event to DSP
    // 6. render remainder
    ProcessStatus::Normal
}
```

All allocation must happen outside the audio callback: use fixed-capacity arrays (`arrayvec`/`heapless`) or pre-sized `Vec`s for the per-block event lists.

**Pattern data flow (GUI ⇄ audio thread).** The pattern is not an automatable parameter set; treat it as document state:

* Keep the authoritative pattern in the plugin struct behind an `Arc` + snapshot mechanism: editor writes a new `Pattern`, publishes it via an atomic swap (e.g. `arc_swap::ArcSwap`) or a lock-free SPSC slot; the audio thread loads the snapshot at block start. Never lock a `Mutex` in `process()` if you can avoid it; if you do use one, only `try_lock` and fall back to the previous snapshot.
* Persist patterns through nih-plug's state system: implement `Plugin::state` save/load by serialising the pattern array (serde + a `String`/byte field in your `#[derive(Params)]`-adjacent state struct, or `serialize_field` in nih-plug's `State` API) so patterns survive DAW project reloads.

**Parameters.** Front-panel controls (Cutoff, Resonance, Env Mod, Decay, Accent, Volume, Tuning, Waveform, plus mod switches: triplet mode, hw-jitter amount, internal/host clock, pattern select) belong in `#[derive(Params)]` with `FloatParam`/`BoolParam`/`EnumParam`. Smooth DSP-facing values with nih-plug's `Smoother` (`SmoothingStyle::Linear`/`Exponential`, ~5–10 ms) — Open303's own `setCutoff` path is unsmoothed and clicks; your Rust port should not inherit that.[^5][^6]

**MIDI input modes** (alongside the internal sequencer): legato-overlap → slide, velocity ≥ 100 → accent, note stack with slide-back on release — exactly jc303's mapping (§4.4). Transport: start the internal sequencer on host `playing == true` (or on a note-on in "key sync" mode, like Open303's `KEY_SYNC`).

---

## 6. Reference implementation sketch (Rust)

A compact but functional core — pattern model, drift-free scheduler, slide/accent state machine, and ISR jitter — written to be dropped into a nih-plug plugin. (DSP envelope/filter details intentionally elided; the event contract is what matters.)

```rust
// ---------------- pattern data ----------------
pub const MAX_STEPS: usize = 16;

#[derive(Clone, Copy, Default)]
pub struct PitchEntry { pub key: i8, pub octave: i8, pub accent: bool, pub slide: bool }

#[derive(Clone, Copy, PartialEq)]
pub enum TimeEntry { Note, Tie, Rest }

#[derive(Clone)]
pub struct Pattern {
    pub pitches: [PitchEntry; MAX_STEPS],
    pub timing: [TimeEntry; MAX_STEPS],
    pub num_steps: u8,
}

// ---------------- events ----------------
#[derive(Clone, Copy, Debug)]
pub enum SeqEvent {
    Trigger { key: i8, octave: i8, accent: bool }, // retrigger envelopes, snap pitch
    SlideTo { key: i8, octave: i8, accent: bool }, // slew pitch, no retrigger
    GateOff,                                       // close gate (VEG release)
}

// ---------------- clock ----------------
#[derive(Clone, Copy, PartialEq)]
pub enum Timebase { Straight, Triplet } // 6 ticks/step (3 on) vs 8 ticks/step (5 on)

impl Timebase {
    fn ticks_per_step(self) -> f64 { match self { Self::Straight => 6.0, Self::Triplet => 8.0 } }
    fn gate_on_ticks(self)  -> f64 { match self { Self::Straight => 3.0, Self::Triplet => 5.0 } }
    fn steps_per_beat(self) -> f64 { match self { Self::Straight => 4.0, Self::Triplet => 8.0/3.0 } }
}

// ---------------- sequencer core ----------------
pub struct Sequencer {
    sample_rate: f64,
    bpm: f64,
    timebase: Timebase,

    // playback state
    running: bool,
    step_idx: usize,
    pitch_idx: usize,           // advances only on TimeEntry::Note
    next_step_sample: f64,      // absolute f64 anchor: drift-free
    gate_off_sample: Option<f64>,

    // slide bookkeeping (one-step lookahead state)
    prev_note_slide: bool,
    prev_was_sounding: bool,    // previous step was Note or Tie (gate may extend)

    // hardware jitter model (§5.6)
    hw_jitter: bool,
    isr_origin: f64,            // free-running ISR phase anchor (absolute samples)
}

const ISR_PERIOD_S: f64 = 0.0018;
const GATE_ON_LATENCY_S: f64 = 0.0010;
const GATE_OFF_LATENCY_S: f64 = 0.00235;

impl Sequencer {
    fn samples_per_step(&self) -> f64 {
        self.sample_rate * 60.0 / (self.bpm * self.timebase.steps_per_beat())
    }

    /// Quantise an absolute sample time to the next ISR tick + latency.
    fn isr_quantise(&self, t: f64, latency_s: f64) -> f64 {
        if !self.hw_jitter { return t; }
        let period = ISR_PERIOD_S * self.sample_rate;
        let rel = t - self.isr_origin;
        self.isr_origin + (rel / period).ceil() * period + latency_s * self.sample_rate
    }

    /// Fill `out` with (sample_offset, event) pairs for the block
    /// [block_start, block_start + num_samples). `out` must be empty.
    pub fn process_block(&mut self, pattern: &Pattern, block_start: u64, num_samples: u32,
                         out: &mut impl Extend<(u32, SeqEvent)>) {
        if !self.running { return; }
        let block_end = block_start as f64 + num_samples as f64;

        // 1. pending gate-off inside this block?
        if let Some(g) = self.gate_off_sample {
            if g >= block_start as f64 && g < block_end {
                let g = self.isr_quantise(g, GATE_OFF_LATENCY_S);
                if g < block_end {
                    out.extend([(g as u32 - block_start as u32, SeqEvent::GateOff)]);
                    self.gate_off_sample = None;
                }
            }
        }

        // 2. step boundaries
        while self.next_step_sample < block_end {
            let t = self.isr_quantise(self.next_step_sample, GATE_ON_LATENCY_S);
            self.advance_step(pattern, t.max(block_start as f64), block_start, out);
            self.next_step_sample += self.samples_per_step();
            self.step_idx = (self.step_idx + 1) % pattern.num_steps as usize;
        }
    }

    fn advance_step(&mut self, pat: &Pattern, t: f64, block_start: u64,
                    out: &mut impl Extend<(u32, SeqEvent)>) {
        let off = t as u32 - block_start as u32;
        match pat.timing[self.step_idx] {
            TimeEntry::Rest => { self.prev_was_sounding = false; }
            TimeEntry::Tie  => { /* gate continues; slide-line quirk is cosmetic */ }
            TimeEntry::Note => {
                self.pitch_idx = (self.pitch_idx + 1) % pat.num_steps as usize;
                let p = pat.pitches[self.pitch_idx];
                let next = pat.timing[(self.step_idx + 1) % pat.num_steps as usize];

                let ev = if self.prev_note_slide && self.prev_was_sounding {
                    SeqEvent::SlideTo { key: p.key, octave: p.octave, accent: p.accent }
                } else {
                    SeqEvent::Trigger { key: p.key, octave: p.octave, accent: p.accent }
                };
                out.extend([(off, ev)]);

                // gate-off: only if this note is not slid into a following note
                if !(p.slide && next == TimeEntry::Note) {
                    let spt = self.samples_per_step() / self.timebase.ticks_per_step();
                    self.gate_off_sample =
                        Some(self.next_step_sample + self.timebase.gate_on_ticks() * spt);
                } else {
                    self.gate_off_sample = None; // gate extends through the next step
                }
                self.prev_note_slide = p.slide;
                self.prev_was_sounding = true;
            }
        }
    }
}
```

And the DSP-side contract (Open303-equivalent behaviour):

```rust
struct Voice {
    pitch_slew: OnePoleSlew,  // tau = 12 ms  ->  60 ms constant-time slide
    target_key: f32,          // semitones; convert to Hz after slew
    meg: DecayEnvelope,       // filter env: decay = knob OR 200 ms on accent
    veg: AmpEnvelope,         // VCA env: fast attack, ~1.2–4 s decay, short release
    accent_sweep: f32,        // charge-pump capacitor for accent build-up
}

impl Voice {
    fn on_event(&mut self, ev: SeqEvent, accent_amount: f32, decay_knob_ms: f32) {
        match ev {
            SeqEvent::Trigger { key, octave, accent } => {
                self.target_key = key as f32 + 12.0 * octave as f32;
                self.pitch_slew.snap(self.target_key);           // no glide
                self.meg.set_decay(if accent { 200.0 } else { decay_knob_ms });
                self.meg.trigger(); self.veg.trigger(accent);
                if accent { self.accent_sweep += (1.0 - self.accent_sweep) * 0.8; }
            }
            SeqEvent::SlideTo { key, octave, accent } => {
                self.target_key = key as f32 + 12.0 * octave as f32; // slew only
                self.meg.set_decay(if accent { 200.0 } else { decay_knob_ms });
                if accent { self.accent_sweep += (1.0 - self.accent_sweep) * 0.8; }
                // NO envelope retrigger — gate stayed high
            }
            SeqEvent::GateOff => self.veg.release(),
        }
    }
}
```

---

## 7. Validation and testing plan

Because `SequencerCore` is pure Rust, everything below can run in `cargo test` and CI:

**Deterministic-grid tests (jitter off)**

1. 16th-note pattern at 120 BPM, 48 kHz: assert step interval averages exactly 22050 samples over ≥ 10⁴ steps with max deviation ≤ 1 sample (drift-free scheduler).
2. Gate duty: exactly 50% of step in 4/4; 62.5% in triplet mode (5/8).
3. Slide semantics: for pattern [Note(slide), Note], assert no `GateOff` between steps and that the second event is `SlideTo`; for [Note(slide), Rest], assert `GateOff` *is* emitted (slide into a rest does not extend the gate).
4. Pitch-queue decoupling: pitches C-D-E-F + timing Note-Note-Rest-Note ⇒ emitted keys C, D, E (rest must not consume a pitch).
5. Accent latch: consecutive accented notes produce one continuous accent state; accent drops at the first unaccented note.

**Jitter-model tests (jitter on)**

6. Statistical match to Schmidt's measurements: over ≥ 10³ steps, gate-on delay relative to ideal grid ≈ 2.06 ± 0.74 ms; duty cycle distribution within 49.96–55.8%.
7. Beating null: at 119.90 BPM (step = 278 × 1.8 ms), assert near-zero variance of gate timing; at 120.00 BPM, assert the characteristic slow beating (variance ≈ ISR-quantisation bound).
8. Block-size independence: render the same 8 bars with buffer sizes 32/128/512/2048 and require bit-identical output event streams.

**DSP/golden tests**

9. Pitch-track a rendered slide (YIN/autocorrelation) and fit an exponential: recovered glide should be constant-time ≈ 60 ms regardless of interval (1 semitone vs 1 octave) — distinguishing constant-time from constant-rate portamento.
10. Accent build-up: render 8 consecutive accented notes; assert monotonically rising cutoff-envelope peaks (the charge-pump behaviour).
11. If you have access to a real 303/x0xb0x recording: render the same pattern and compare gate-event onset histograms and slide time constants.

---

## 8. Fidelity checklist

**Must-have (defines the 303 sequencer):**
- [ ] 24 PPQN-derived grid, 6 ticks/step 4/4 with 50% gate duty
- [ ] Slide = gate extension + destination-step CV slew, one-step lookahead, no envelope retrigger
- [ ] ~60 ms constant-time exponential pitch slew (τ ≈ 12 ms one-pole)
- [ ] Accent latched per whole step; accent ⇒ 200 ms filter-env decay + VCA boost
- [ ] Decoupled pitch/timing memory (pitch advances only on note steps; ties; rests)
- [ ] Sample-accurate, drift-free, block-size-independent scheduling

**Should-have (audible character):**
- [ ] Triplet mode (8 ticks/step, 5-on/3-off)
- [ ] Interrupt-clock jitter/beating model (§5.6) with tempo sweet spots (119.90 BPM…)
- [ ] Accent sweep build-up over consecutive accents
- [ ] MIDI input mode: legato = slide, velocity ≥ 100 = accent

**Optional (documented but inaudible):**
- [ ] 44 µs DAC latch pulse on non-slid notes; 8.75 µs latch dip mid-slide
- [ ] Slide-line-high-during-ties/rests quirk
- [ ] Duty-cycle tempo dependence (fixed latencies vs. step length)

---

## 9. References

[^1^]: Julian Schmidt, *Analysis of the µPD650C-133 CPU timing*, Sonic Potions / RE-303 project, 2017. http://sonic-potions.com/Documentation/Analysis_of_the_D650C-133_CPU_timing.pdf
[^2^]: Robin Whittle, *Investigating the Slide function of the TB-303*, Devil Fish, 1999. https://www.firstpr.com.au/rwi/dfish/303-slide.html
[^3^]: Zoe Blade, *TB-303 notebook* (CPU, RAM, timebase notes). https://notebook.zoeblade.com/TB-303.html
[^4^]: Andrew Olney, *Computational Thinking through Modular Sound Synthesis, Ch. 13: TB-303* (sequencer structure, accent sweep build-up). https://olney.ai/ct-modular-book/tb-303.html
[^5^]: midilab, *jc303* — JUCE port of Robin Schmidt's Open303 DSP engine (source: `src/dsp/open303/rosic_AcidSequencer.*`, `rosic_AcidPattern.*`, `rosic_Open303.*`, `rosic_LeakyIntegrator.*`, `src/JC303.cpp`). https://github.com/midilab/jc303
[^6^]: Robbert van der Helm, *nih-plug* — `ProcessContext`, `Transport`, note events (`src/context/process.rs`). https://github.com/robbert-vdh/nih-plug
[^7^]: Robin Whittle's accent-circuit analysis as summarised in *Patch Ideas I Learned While Studying the TB-303* (Learning Modular). https://www.patreon.com/LearningModular/posts/patch-ideas-i-tb-41917616
[^8^]: *midilab releases JC-303 free bass synthesizer* (step-sequencer fidelity as project roadmap). https://rekkerd.org/midilab-releases-jc-303-free-bass-synthesizer-for-windows-mac-linux/
