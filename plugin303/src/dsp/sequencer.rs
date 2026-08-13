//! 16-step sequencer engine.

pub const MAX_STEPS: usize = 16;

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct PitchEntry {
    pub key: i8,
    pub octave: i8,
    pub accent: bool,
    pub slide: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TimeEntry {
    #[default]
    Note,
    Tie,
    Rest,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pattern {
    pub pitches: [PitchEntry; MAX_STEPS],
    pub timing: [TimeEntry; MAX_STEPS],
    pub num_steps: u8,
}

impl Default for Pattern {
    fn default() -> Self {
        Self {
            pitches: [PitchEntry::default(); MAX_STEPS],
            timing: [TimeEntry::Note; MAX_STEPS],
            num_steps: 16,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SeqEvent {
    Trigger { key: i8, octave: i8, accent: bool },
    SlideTo { key: i8, octave: i8, accent: bool },
    GateOff,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Timebase {
    #[default]
    Straight,
    Triplet,
}

impl Timebase {
    pub fn ticks_per_step(self) -> f64 {
        match self {
            Self::Straight => 6.0,
            Self::Triplet => 8.0,
        }
    }
    pub fn gate_on_ticks(self) -> f64 {
        match self {
            Self::Straight => 3.0,
            Self::Triplet => 5.0,
        }
    }
    pub fn steps_per_beat(self) -> f64 {
        match self {
            Self::Straight => 4.0,
            Self::Triplet => 8.0 / 3.0,
        }
    }
}

pub struct Sequencer {
    sample_rate: f64,
    running: bool,
    step_idx: usize,
    pitch_idx: usize,
    next_step_sample: f64,
    gate_off_sample: Option<f64>,
    prev_note_slide: bool,
    prev_was_sounding: bool,
    isr_origin: f64,
}

const ISR_PERIOD_S: f64 = 0.0018;
const GATE_ON_LATENCY_S: f64 = 0.0010;
const GATE_OFF_LATENCY_S: f64 = 0.00235;

impl Sequencer {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            running: false,
            step_idx: 0,
            pitch_idx: 0,
            next_step_sample: 0.0,
            gate_off_sample: None,
            prev_note_slide: false,
            prev_was_sounding: false,
            isr_origin: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    pub fn set_running(&mut self, running: bool) {
        if running && !self.running {
            self.reset();
        }
        self.running = running;
    }

    pub fn reset(&mut self) {
        self.step_idx = 0;
        self.pitch_idx = 0;
        self.next_step_sample = 0.0;
        self.gate_off_sample = None;
        self.prev_note_slide = false;
        self.prev_was_sounding = false;
    }

    pub fn reset_transport(&mut self, _pos_beats: f64) {
        self.next_step_sample = 0.0;
    }

    fn samples_per_step(&self, bpm: f64, timebase: Timebase) -> f64 {
        self.sample_rate * 60.0 / (bpm * timebase.steps_per_beat())
    }

    fn isr_quantise(&self, t: f64, latency_s: f64, hw_jitter: bool) -> f64 {
        if !hw_jitter {
            return t;
        }
        let period = ISR_PERIOD_S * self.sample_rate;
        let rel = t - self.isr_origin;
        self.isr_origin + (rel / period).ceil() * period + latency_s * self.sample_rate
    }

    pub fn process_block(
        &mut self,
        pattern: &Pattern,
        block_start: u64,
        num_samples: u32,
        out_events: &mut [(u32, SeqEvent); 64],
        out_count: &mut usize,
        bpm: f64,
        timebase: Timebase,
        hw_jitter: bool,
    ) {
        if !self.running {
            return;
        }
        let block_end = block_start as f64 + num_samples as f64;
        let num_steps = (pattern.num_steps as usize).clamp(1, 16);

        // 1. Pending GateOff from previous block?
        if let Some(g) = self.gate_off_sample {
            let g_quant = self.isr_quantise(g, GATE_OFF_LATENCY_S, hw_jitter);
            if g_quant >= block_start as f64 && g_quant < block_end {
                let offset = (g_quant as u32).saturating_sub(block_start as u32);
                if *out_count < 64 {
                    out_events[*out_count] = (offset, SeqEvent::GateOff);
                    *out_count += 1;
                }
                self.gate_off_sample = None;
            }
        }

        // 2. Step boundaries
        while self.next_step_sample < block_end {
            let t = self.isr_quantise(self.next_step_sample, GATE_ON_LATENCY_S, hw_jitter);
            let offset = (t as u32).saturating_sub(block_start as u32);

            match pattern.timing[self.step_idx] {
                TimeEntry::Rest => {
                    self.prev_was_sounding = false;
                }
                TimeEntry::Tie => {
                    // Gate continues sounding
                }
                TimeEntry::Note => {
                    let p = pattern.pitches[self.pitch_idx];
                    let next_timing = pattern.timing[(self.step_idx + 1) % num_steps];

                    let ev = if self.prev_note_slide && self.prev_was_sounding {
                        SeqEvent::SlideTo {
                            key: p.key,
                            octave: p.octave,
                            accent: p.accent,
                        }
                    } else {
                        SeqEvent::Trigger {
                            key: p.key,
                            octave: p.octave,
                            accent: p.accent,
                        }
                    };

                    if *out_count < 64 {
                        out_events[*out_count] = (offset, ev);
                        *out_count += 1;
                    }

                    // Schedule gate-off unless this note slides into an active sounding step (Note or Tie)
                    if !(p.slide && next_timing != TimeEntry::Rest) {
                        let spt = self.samples_per_step(bpm, timebase) / timebase.ticks_per_step();
                        let gate_off_time = self.next_step_sample + timebase.gate_on_ticks() * spt;
                        let g_quant = self.isr_quantise(gate_off_time, GATE_OFF_LATENCY_S, hw_jitter);

                        if g_quant < block_end {
                            let off_offset = (g_quant as u32).saturating_sub(block_start as u32);
                            if *out_count < 64 {
                                out_events[*out_count] = (off_offset, SeqEvent::GateOff);
                                *out_count += 1;
                            }
                            self.gate_off_sample = None;
                        } else {
                            self.gate_off_sample = Some(gate_off_time);
                        }
                    } else {
                        self.gate_off_sample = None;
                    }

                    self.prev_note_slide = p.slide;
                    self.prev_was_sounding = true;
                    self.pitch_idx = (self.pitch_idx + 1) % num_steps;
                }
            }

            self.next_step_sample += self.samples_per_step(bpm, timebase);
            self.step_idx = (self.step_idx + 1) % num_steps;
        }
    }
}

impl Default for Sequencer {
    fn default() -> Self {
        Self::new(48000.0)
    }
}
