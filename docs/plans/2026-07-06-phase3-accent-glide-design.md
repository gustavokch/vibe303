# Phase 3 Design Spec: Accent, Glide, and Decay systems

**Date:** 2026-07-06
**Status:** Spec Proposal

---

## 1. Goal

Implement the signature Roland TB-303 dynamic movement features:
1. **Accent:** A leaky-integrator accumulator representing the charge on the accent capacitor, which stacks on consecutive accents and dynamically modulates filter cutoff, VCF envelope decay, and VCA output level.
2. **Glide / Slide:** A one-pole pitch slew in log-frequency (semitone) space built directly into the `Voice` struct, driven by both the sequencer's slide events and MIDI legato transitions.
3. **Decay:** Scaled envelope decay times for VCF/VCA, integrated with dynamic accent decay shortening.

All DSP modifications must be zero-allocation, thread-safe, and compatible with the three fidelity tiers (Low, High, Ultra) without breaking existing tests.

---

## 2. Mathematical Models

### 2.1 Accent Accumulator & Modulations
The accent envelope behaves as a continuous capacitor charging/discharging circuit:
- **Continuous Decay:** At each sample, the accumulator `accent_acc` decays exponentially towards `0.0` with a time constant $\tau_{\text{accent}} \approx 200$ ms:
  $$c_{\text{decay}} = \exp\left(-\frac{1}{0.2 \cdot f_s}\right)$$
  $$\text{accent\_acc} \leftarrow \text{accent\_acc} \cdot c_{\text{decay}}$$
- **Trigger Stacking:** When an accented step/note triggers:
  $$\text{accent\_acc} \leftarrow (\text{accent\_acc} + 1.0).\min(1.8)$$
  This provides the stacking boost on consecutive accents.
- **Modulation Routing:**
  - **Cutoff (`+cutoff`):** Adds a direct offset to the filter cutoff frequency in Hz:
    $$\text{cutoff\_hz} = \text{base\_cutoff\_hz} + \text{meg\_val} \cdot \text{env\_mod} \cdot 4000.0 + \text{accent\_knob} \cdot \text{accent\_acc} \cdot 3000.0$$
    The final value is clamped to $[20.0, 0.45 \cdot f_s]$.
  - **MEG Decay (`-MEG decay`):** Accent dynamically shortens the filter envelope's decay time constant:
    $$\text{decay\_ms} = \text{lerp}(D_{\text{normal}}, 200.0, (\text{accent\_knob} \cdot \text{accent\_acc}).\min(1.0))$$
    where $D_{\text{normal}}$ is the normal decay time set by the `decay_knob` ($200 \text{ ms} + \text{decay\_knob} \cdot 1800 \text{ ms}$). This shrinks decay down to $200 \text{ ms}$.
  - **VCA gain (`+VCA`):** Boosts the output volume:
    $$\text{vca\_gain} = \text{vca\_level} \cdot (1.0 + \text{accent\_knob} \cdot \text{accent\_acc} \cdot 0.5)$$

### 2.2 Glide / Slide (Pitch Slew)
Slewing in pitch (semitone) space matches the physical 303 CV slide circuit:
- **Glide Time Constant:** The glide time ranges from 10 ms to 410 ms:
  $$\tau_{\text{glide}} = 10.0 + \text{glide\_knob} \cdot 400.0 \text{ ms}$$
  $$c_{\text{glide}} = 1.0 - \exp\left(-\frac{1}{\tau_{\text{glide}} \cdot 0.001 \cdot f_s}\right)$$
- **Pitch Tracking State:** `Voice` manages `current_pitch_st` and `target_pitch_st` in semitones (where $69.0 \equiv 440$ Hz).
- **Pitch Updates:**
  - On `note_on(note)`:
    $$\text{target\_pitch\_st} = \text{note as f32}$$
    If `slide_active` is false:
    $$\text{current\_pitch\_st} = \text{target\_pitch\_st}$$
  - On `set_freq(hz)`:
    $$\text{target\_pitch\_st} = 12.0 \cdot \log_2\left(\frac{\text{hz}}{440.0}\right) + 69.0$$
    If `slide_active` is false:
    $$\text{current\_pitch\_st} = \text{target\_pitch\_st}$$
- **Sample Tick:**
  $$\text{current\_pitch\_st} \leftarrow \text{current\_pitch\_st} + c_{\text{glide}} \cdot (\text{target\_pitch\_st} - \text{current\_pitch\_st})$$
  $$\text{base\_hz} = 440.0 \cdot 2^{\frac{\text{current\_pitch\_st} - 69.0}{12.0}}$$
- **Legato Detection:**
  In `lib.rs`, if a new note is triggered while the previous note's gate is still open, `slide_active` is automatically enabled if the glide knob is above 0.

---

## 3. Structural Modifications

### 3.1 `Voice` Struct changes
- Add fields:
  - `slide_active: bool`
  - `current_pitch_st: f32`
  - `target_pitch_st: f32`
  - `glide_knob: f32`
- Update `update_params` to accept `glide_knob`.
- Update `set_gate` to accept a `slide` parameter:
  `pub fn set_gate(&mut self, gate: bool, trigger: bool, accent: bool, slide: bool)`

### 3.2 `Sequencer` Struct changes
- The sequencer no longer slews pitch itself; it reports the raw target frequency (`self.freq = target_freq`) and whether a slide is active.
- `sequencer.rs` will be cleaned up to pass the raw frequency and slide flags, delegating the slew to `Voice`.

---

## 4. TDD Testing Strategy

We will write targeted unit and integration tests to verify the new behaviors:
1. **Accent Stacking Test:** Play a sequence of accented notes and verify `accent_acc` increases beyond 1.0 (clamping at 1.8).
2. **Accent Cutoff & VCA Modulation Test:** Verify Cutoff frequency and VCA gain are boosted proportionally to `accent_acc`.
3. **Accent Decay Shortening Test:** Verify that accented notes result in shorter decay times compared to unaccented notes for the same `decay_knob` setting.
4. **Glide Slew Test:** Verify that when `slide` is active, the pitch slews exponentially in semitone space over the correct time constant.
5. **MIDI Legato Glide Test:** Verify that overlapping MIDI notes trigger slide/glide automatically.
