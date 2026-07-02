//! 16-step sequencer engine.

use crate::params::{SequencerStep, StepGate};

/// A 16-step sequencer tracking step playback position, timing phases,
/// and generating frequency/trigger/gate/accent modulation signals.
pub struct Sequencer {
    /// Active playing step index.
    step_idx: usize,
    /// Counter tracking sample ticks inside the current step (for internal clock).
    sample_counter: f64,
    /// Previously processed step index (used to detect step boundary crossings).
    prev_step_idx: usize,
    /// Previously computed gate state.
    prev_gate: bool,
    /// Tracks if host was playing in the previous block.
    was_playing: bool,
    
    // Pitch slew state for slide/glide
    /// Current slewed pitch frequency.
    current_freq: f32,
    /// target pitch frequency of the active step.
    target_freq: f32,
    
    // Output control values
    /// Output slewed note frequency.
    pub freq: f32,
    /// Output envelope gate state.
    pub gate: bool,
    /// Output step accent modifier.
    pub accent: bool,
    /// Output step velocity.
    pub velocity: f32,
    /// Output envelope note trigger indicator.
    pub trigger: bool,
}

/// Host transport and synchronization data passed to the sequencer.
pub struct TransportInfo {
    /// True if the host transport is currently playing.
    pub playing: bool,
    /// Current host playback position in beats (PPQ).
    pub pos_beats: Option<f64>,
    /// Current host tempo in BPM.
    pub tempo: Option<f64>,
}

impl Sequencer {
    pub fn new() -> Self {
        Self {
            step_idx: 0,
            sample_counter: 0.0,
            prev_step_idx: 99, // dummy initial value
            prev_gate: false,
            was_playing: false,
            current_freq: 440.0,
            target_freq: 440.0,
            freq: 440.0,
            gate: false,
            accent: false,
            velocity: 0.0,
            trigger: false,
        }
    }

    pub fn reset(&mut self) {
        self.step_idx = 0;
        self.sample_counter = 0.0;
        self.prev_step_idx = 99;
        self.prev_gate = false;
        self.was_playing = false;
        self.gate = false;
        self.trigger = false;
    }

    /// Advance the sequencer by one sample. Returns (freq, gate, accent, velocity, trigger)
    pub fn tick(
        &mut self,
        sr: f32,
        transport: &TransportInfo,
        steps: &[SequencerStep; 16],
        length: usize,
        tempo_param: f32,
        sync: bool,
        glide_param: f32,
    ) {
        if !transport.playing {
            self.gate = false;
            self.trigger = false;
            self.was_playing = false;
            return;
        }

        if !self.was_playing {
            // Reset playback pointers when starting transport
            self.reset();
            self.was_playing = true;
        }

        let length = length.clamp(1, 16);
        let tempo = transport.tempo.unwrap_or(tempo_param as f64) as f32;
        let step_duration_samples = (sr * 60.0) / (tempo * 4.0); // 16th note length in samples

        let (step_idx, phase, step_changed) = if sync && transport.pos_beats.is_some() {
            let ppq = transport.pos_beats.unwrap();
            let step_float = ppq * 4.0; // 4 steps per beat (16th notes)
            let raw_idx = step_float.floor() as i64;
            let idx = (raw_idx.rem_euclid(length as i64)) as usize;
            let phase = step_float.fract() as f32;
            
            let changed = idx != self.prev_step_idx;
            (idx, phase, changed)
        } else {
            // Free-running internal clock
            self.sample_counter += 1.0;
            let mut changed = false;
            if self.sample_counter >= step_duration_samples as f64 {
                self.sample_counter -= step_duration_samples as f64;
                self.step_idx = (self.step_idx + 1) % length;
                changed = true;
            }
            let phase = (self.sample_counter / step_duration_samples as f64) as f32;
            (self.step_idx, phase, changed)
        };

        if step_changed {
            self.prev_step_idx = step_idx;
        }

        let step = steps[step_idx];
        
        // Calculate raw step frequency (pitch + octave offset)
        // Midi base 36 (C1) as the 0-reference for sequencer pitch tracking.
        let base_note = 36 + step.semitone + step.octave * 12;
        let step_freq = 440.0 * 2.0f32.powf((base_note as f32 - 69.0) / 12.0);
        self.target_freq = step_freq;

        // Slide / Glide logic:
        // Glide slew coefficient. 0.0 = instant, 1.0 = extremely slow.
        // We configure it so 0.5 is a classic 303 slide speed (~80 ms).
        let slide_time_ms = 10.0 + glide_param * 400.0; // 10 ms to 410 ms slide
        let glide_alpha = 1.0 - (-1.0 / (slide_time_ms * 0.001 * sr)).exp();
        
        // Slew the pitch if slide is active or if we are sliding from the previous step
        let prev_step = steps[(step_idx + length - 1) % length];
        let sliding = prev_step.slide && prev_step.gate != StepGate::Rest;
        if sliding {
            self.current_freq += glide_alpha * (self.target_freq - self.current_freq);
        } else {
            self.current_freq = self.target_freq;
        }
        self.freq = self.current_freq;

        // Gate state:
        let is_note_step = step.gate != StepGate::Rest;
        let gate_on = if is_note_step {
            if step.slide {
                true // slide keeps gate open for the whole step duration
            } else {
                phase < step.gate_len
            }
        } else {
            false
        };

        // Note Trigger logic:
        // We trigger a note when:
        // 1. We just entered a note step, AND
        // 2. Either the previous gate was off, OR (the previous step did not slide AND current step is not a Tie)
        let mut trigger = false;
        if step_changed && is_note_step {
            let prev_was_slide = prev_step.slide && prev_step.gate != StepGate::Rest;
            let is_tie = step.gate == StepGate::Tie;
            if !self.prev_gate || (!prev_was_slide && !is_tie) {
                trigger = true;
            }
        }

        self.gate = gate_on;
        self.trigger = trigger;
        self.accent = step.accent && gate_on;
        self.velocity = step.velocity;
        self.prev_gate = gate_on;
    }
}

impl Default for Sequencer {
    fn default() -> Self {
        Self::new()
    }
}
