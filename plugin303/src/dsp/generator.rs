//! Probabilistic Acid Euclidean Sequence Generator (`AcidGenerator`).

use crate::dsp::sequencer::{Pattern, PitchEntry, TimeEntry, MAX_STEPS};
use serde::{Deserialize, Serialize};

/// Supported musical scales for 303 sequence generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MusicalScale {
    #[default]
    MinorPentatonic,
    Dorian,
    Phrygian,
    Aeolian,
    HarmonicMinor,
    Chromatic,
}

impl MusicalScale {
    /// Return the semitone offsets for scale degrees relative to root note (0).
    pub fn intervals(&self) -> &'static [i8] {
        match self {
            MusicalScale::MinorPentatonic => &[0, 3, 5, 7, 10],
            MusicalScale::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            MusicalScale::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            MusicalScale::Aeolian => &[0, 2, 3, 5, 7, 8, 10],
            MusicalScale::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
            MusicalScale::Chromatic => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        }
    }
}

/// Parameters controlling the generative sequence synthesis.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeneratorParams {
    /// Root note offset: 0=C, 1=C#, ..., 11=B.
    pub root_note: u8,
    /// Musical scale for pitch quantization.
    pub scale: MusicalScale,
    /// Minimum allowed octave (-1, 0, +1).
    pub octave_min: i8,
    /// Maximum allowed octave (-1, 0, +1).
    pub octave_max: i8,
    /// Number of Euclidean pulses (k: 1..16).
    pub rhythm_pulses: u8,
    /// Total Euclidean rhythm steps (n: 1..16).
    pub rhythm_length: u8,
    /// Cyclic rhythm rotation shift (0..15).
    pub rhythm_shift: u8,
    /// Pitch sequence length for polyrhythmic decoupling (1..16).
    pub pitch_length: u8,
    /// Probability of rest step vs active note (0.0..1.0).
    pub rest_probability: f32,
    /// Probability of step accent (0.0..1.0).
    pub accent_probability: f32,
    /// Probability of step slide (0.0..1.0).
    pub slide_probability: f32,
    /// Probability of octave up shift (+1).
    pub octave_up_prob: f32,
    /// Probability of octave down shift (-1).
    pub octave_down_prob: f32,
    /// Preference for higher scale degrees on accented notes (0.0..1.0).
    pub accent_pitch_bias: f32,
    /// Allow consecutive slided steps.
    pub slide_chaining: bool,
    /// Probability of mutating each step when loop wraps (0.0..1.0).
    pub mutation_rate: f32,
    /// PRNG seed for deterministic generation.
    pub seed: u64,
}

impl Default for GeneratorParams {
    fn default() -> Self {
        Self {
            root_note: 0, // C
            scale: MusicalScale::MinorPentatonic,
            octave_min: -1,
            octave_max: 1,
            rhythm_pulses: 5,
            rhythm_length: 16,
            rhythm_shift: 0,
            pitch_length: 16,
            rest_probability: 0.2,
            accent_probability: 0.35,
            slide_probability: 0.25,
            octave_up_prob: 0.2,
            octave_down_prob: 0.1,
            accent_pitch_bias: 0.5,
            slide_chaining: false,
            mutation_rate: 0.15,
            seed: 42,
        }
    }
}

/// Lightweight deterministic PRNG (XorShift64) for generator and mutation.
#[derive(Clone, Debug, PartialEq)]
pub struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    pub fn new(seed: u64) -> Self {
        let state = if seed == 0 { 0x853c_49e6_748f_ea9b } else { seed };
        Self { state }
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    pub fn next_f32(&mut self) -> f32 {
        ((self.next_u64() >> 40) as f32) / 16777216.0
    }

    pub fn range_usize(&mut self, min: usize, max: usize) -> usize {
        if min >= max {
            return min;
        }
        let range = max - min + 1;
        min + (self.next_u64() as usize % range)
    }
}

/// Core engine for generating and mutating 303 acid patterns.
pub struct AcidGenerator;

impl AcidGenerator {
    /// Distribute `k` pulses evenly across `n` steps using Bjorklund algorithm.
    pub fn bjorklund(pulses: u8, length: u8, shift: u8) -> [bool; MAX_STEPS] {
        let n = (length.clamp(1, MAX_STEPS as u8)) as usize;
        let k = (pulses.clamp(0, n as u8)) as usize;
        let mut result = [false; MAX_STEPS];

        if k == 0 {
            return result;
        }
        if k >= n {
            for item in result.iter_mut().take(n) {
                *item = true;
            }
            return result;
        }

        let mut sequences: Vec<Vec<bool>> = (0..k).map(|_| vec![true]).collect();
        let mut rests: Vec<Vec<bool>> = (0..(n - k)).map(|_| vec![false]).collect();

        while rests.len() > 1 {
            let num_sub = sequences.len().min(rests.len());
            for i in 0..num_sub {
                if let Some(rest) = rests.pop() {
                    sequences[i].extend(rest);
                }
            }
        }
        sequences.extend(rests);

        let mut flat: Vec<bool> = sequences.into_iter().flatten().collect();
        if !flat.is_empty() {
            let shift_amt = (shift as usize) % flat.len();
            if shift_amt > 0 {
                flat.rotate_right(shift_amt);
            }
        }

        for (i, &pulse) in flat.iter().take(MAX_STEPS).enumerate() {
            result[i] = pulse;
        }

        result
    }

    /// Generate a 16-step 303 `Pattern` deterministically from `GeneratorParams`.
    pub fn generate(params: &GeneratorParams) -> Pattern {
        let mut rng = SimpleRng::new(params.seed);
        let pulse_mask = Self::bjorklund(params.rhythm_pulses, params.rhythm_length, params.rhythm_shift);

        let intervals = params.scale.intervals();
        let num_degrees = intervals.len();

        let mut pattern = Pattern {
            pitches: [PitchEntry::default(); MAX_STEPS],
            timing: [TimeEntry::Rest; MAX_STEPS],
            num_steps: params.rhythm_length.clamp(1, 16),
            decoupled: params.rhythm_length != params.pitch_length,
        };

        let mut prev_degree_idx: usize = 0;
        let mut prev_slide = false;
        let mut prev_pitch = PitchEntry::default();

        for i in 0..MAX_STEPS {
            let is_pulse = pulse_mask[i];
            let is_rest = !is_pulse || (rng.next_f32() < params.rest_probability);

            if is_rest {
                if prev_slide {
                    // Slide into rest -> Tie carrying forward pitch
                    pattern.timing[i] = TimeEntry::Tie;
                    pattern.pitches[i] = prev_pitch;
                    prev_slide = false;
                } else {
                    pattern.timing[i] = TimeEntry::Rest;
                    pattern.pitches[i] = PitchEntry::default();
                }
                continue;
            }

            // Note Step
            pattern.timing[i] = TimeEntry::Note;

            // Accent calculation
            let accent = rng.next_f32() < params.accent_probability;

            // Slide calculation with chaining rule
            let raw_slide = rng.next_f32() < params.slide_probability;
            let slide = if !params.slide_chaining && prev_slide {
                false
            } else {
                raw_slide
            };

            // Scale degree selection with optional accent pitch bias
            let degree_idx = if accent && params.accent_pitch_bias > 0.0 && rng.next_f32() < params.accent_pitch_bias {
                // Favor upper half of scale degrees
                let upper_min = num_degrees / 2;
                rng.range_usize(upper_min, num_degrees - 1)
            } else {
                // Walk around previous degree
                let step_change = rng.range_usize(0, 2); // 0, 1, or 2 steps shift
                if rng.next_f32() < 0.5 {
                    (prev_degree_idx + step_change) % num_degrees
                } else {
                    (prev_degree_idx + num_degrees - (step_change % num_degrees)) % num_degrees
                }
            };
            prev_degree_idx = degree_idx;

            let key = (params.root_note as i8 + intervals[degree_idx]).rem_euclid(12);

            // Octave selection
            let mut octave: i8 = 0;
            let oct_r = rng.next_f32();
            if oct_r < params.octave_up_prob {
                octave = 1;
            } else if oct_r < (params.octave_up_prob + params.octave_down_prob) {
                octave = -1;
            }
            octave = octave.clamp(params.octave_min, params.octave_max);

            let pitch_entry = PitchEntry {
                key,
                octave,
                accent,
                slide,
            };

            pattern.pitches[i] = pitch_entry;
            prev_slide = slide;
            prev_pitch = pitch_entry;
        }

        pattern
    }

    /// Mutate active steps in `pattern` based on `mutation_rate` and `loop_count`.
    pub fn mutate(pattern: &mut Pattern, params: &GeneratorParams, loop_count: u64) {
        if params.mutation_rate <= 0.0 {
            return;
        }
        // Deterministic PRNG seed derived from base seed + loop_count
        let seed = params.seed.wrapping_add(loop_count.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut rng = SimpleRng::new(seed);

        let intervals = params.scale.intervals();
        let num_degrees = intervals.len();

        for i in 0..(pattern.num_steps as usize) {
            if rng.next_f32() < params.mutation_rate {
                match rng.range_usize(0, 2) {
                    0 => {
                        // Toggle accent / slide
                        pattern.pitches[i].accent = !pattern.pitches[i].accent;
                        if rng.next_f32() < 0.5 {
                            pattern.pitches[i].slide = !pattern.pitches[i].slide;
                        }
                    }
                    1 => {
                        // Nudge pitch scale degree by +-1
                        let current_key = pattern.pitches[i].key;
                        let mut degree = 0;
                        for (d, &interval) in intervals.iter().enumerate() {
                            if (params.root_note as i8 + interval).rem_euclid(12) == current_key {
                                degree = d;
                                break;
                            }
                        }
                        let shift = if rng.next_f32() < 0.5 { 1 } else { num_degrees - 1 };
                        let new_degree = (degree + shift) % num_degrees;
                        pattern.pitches[i].key = (params.root_note as i8 + intervals[new_degree]).rem_euclid(12);
                    }
                    2 => {
                        // Flip note vs rest state (preserving slide ties)
                        if pattern.timing[i] == TimeEntry::Rest {
                            pattern.timing[i] = TimeEntry::Note;
                        } else if pattern.timing[i] == TimeEntry::Note {
                            pattern.timing[i] = TimeEntry::Rest;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
