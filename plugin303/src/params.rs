use crate::dsp::filter::Tier;
use nih_plug::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Enum)]
pub enum CompTolerance {
    #[default]
    #[name = "0%"]
    Zero,
    #[name = "0.5%"]
    PointFive,
    #[name = "2%"]
    Two,
    #[name = "5%"]
    Five,
    #[name = "10%"]
    Ten,
}

impl CompTolerance {
    pub fn as_f64(&self) -> f64 {
        match self {
            CompTolerance::Zero => 0.0,
            CompTolerance::PointFive => 0.005,
            CompTolerance::Two => 0.02,
            CompTolerance::Five => 0.05,
            CompTolerance::Ten => 0.10,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Enum)]
pub enum CircuitTopology {
    #[name = "Accurate"]
    Accurate,
    #[default]
    #[name = "Fun"]
    Fun,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Enum)]
pub enum OscWave {
    #[default]
    #[name = "Saw"]
    Saw,
    #[name = "Square"]
    Square,
    #[name = "Sweep"]
    Sweep,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Enum)]
pub enum VcfTaps {
    #[name = "2-pole (12 dB/oct)"]
    TwoPole,
    #[name = "3-pole (18 dB/oct)"]
    ThreePole,
    #[default]
    #[name = "4-pole (24 dB/oct)"]
    FourPole,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Enum)]
pub enum VcaEnvMode {
    #[name = "Gate"]
    Gate,
    #[default]
    #[name = "Envelope"]
    Envelope,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Enum)]
pub enum ClipperOversampling {
    #[default]
    #[name = "2x"]
    X2,
    #[name = "4x"]
    X4,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Enum)]
pub enum SequencerMode {
    #[default]
    #[name = "Direct Grid"]
    DirectGrid,
    #[name = "Decoupled (Roland 303)"]
    DecoupledHardware,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Enum)]
pub enum Timebase {
    #[default]
    #[name = "16th (Straight)"]
    Straight,
    #[name = "12th (Triplet)"]
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

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum StepGate {
    Rest,
    Note,
    Tie,
}

impl Default for StepGate {
    fn default() -> Self {
        Self::Note
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SequencerStep {
    pub semitone: i8, // 0..12
    pub octave: i8,   // -2..2
    pub gate: StepGate,
    pub accent: bool,
    pub slide: bool,
    pub gate_len: f32, // 0.0..1.0
    pub velocity: f32, // 0.0..1.0
}

impl Default for SequencerStep {
    fn default() -> Self {
        Self {
            semitone: 0,
            octave: 0,
            gate: StepGate::Note,
            accent: false,
            slide: false,
            gate_len: 0.5,
            velocity: 0.7,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SequencerPattern {
    pub steps: [SequencerStep; 16],
}

impl Default for SequencerPattern {
    fn default() -> Self {
        let mut steps = [SequencerStep::default(); 16];
        // Classic acid baseline initial pattern with NOTE, TIE, REST variation
        steps[0].accent = true;
        steps[1].gate = StepGate::Tie;
        steps[2].semitone = 3; // D#
        steps[2].slide = true;
        steps[3].gate = StepGate::Rest;
        steps[4].accent = true;
        steps[5].gate = StepGate::Tie;
        steps[6].semitone = 7; // G
        steps[6].slide = true;
        steps[7].gate = StepGate::Rest;
        steps[8].semitone = 10; // A#
        steps[8].accent = true;
        steps[9].gate = StepGate::Tie;
        steps[10].semitone = 12; // C4
        steps[10].slide = true;
        steps[11].gate = StepGate::Rest;
        steps[12].accent = true;
        steps[13].gate = StepGate::Tie;
        steps[14].semitone = 7; // G
        steps[14].slide = true;
        steps[15].gate = StepGate::Rest;

        Self { steps }
    }
}

impl SequencerPattern {
    pub fn compile(&self, num_steps: u8, mode: SequencerMode) -> crate::dsp::sequencer::Pattern {
        let num_steps = num_steps.clamp(1, 16);
        let mut pitches = [crate::dsp::sequencer::PitchEntry::default(); 16];
        let mut timing = [crate::dsp::sequencer::TimeEntry::Rest; 16];

        match mode {
            SequencerMode::DirectGrid => {
                for i in 0..16 {
                    timing[i] = match self.steps[i].gate {
                        StepGate::Note => crate::dsp::sequencer::TimeEntry::Note,
                        StepGate::Tie => crate::dsp::sequencer::TimeEntry::Tie,
                        StepGate::Rest => crate::dsp::sequencer::TimeEntry::Rest,
                    };
                    pitches[i] = crate::dsp::sequencer::PitchEntry {
                        key: self.steps[i].semitone,
                        octave: self.steps[i].octave,
                        accent: self.steps[i].accent,
                        slide: self.steps[i].slide,
                    };
                }
            }
            SequencerMode::DecoupledHardware => {
                let mut note_count = 0;
                for i in 0..num_steps as usize {
                    timing[i] = match self.steps[i].gate {
                        StepGate::Note => {
                            pitches[note_count] = crate::dsp::sequencer::PitchEntry {
                                key: self.steps[i].semitone,
                                octave: self.steps[i].octave,
                                accent: self.steps[i].accent,
                                slide: self.steps[i].slide,
                            };
                            note_count += 1;
                            crate::dsp::sequencer::TimeEntry::Note
                        }
                        StepGate::Tie => crate::dsp::sequencer::TimeEntry::Tie,
                        StepGate::Rest => crate::dsp::sequencer::TimeEntry::Rest,
                    };
                }
            }
        }

        let decoupled = match mode {
            SequencerMode::DirectGrid => false,
            SequencerMode::DecoupledHardware => true,
        };

        crate::dsp::sequencer::Pattern {
            pitches,
            timing,
            num_steps,
            decoupled,
        }
    }
}

#[derive(Params)]
pub struct Plugin303Params {
    #[id = "tier"]
    pub tier: EnumParam<Tier>,

    #[id = "topology"]
    pub topology: EnumParam<CircuitTopology>,

    // --- Classic Front Panel ---
    #[id = "tuning"]
    pub tuning: FloatParam,
    #[id = "cutoff"]
    pub cutoff: FloatParam,
    #[id = "resonance"]
    pub resonance: FloatParam,
    #[id = "env_mod"]
    pub env_mod: FloatParam,
    #[id = "decay"]
    pub decay: FloatParam,
    #[id = "accent"]
    pub accent: FloatParam,
    #[id = "waveform"]
    pub waveform: EnumParam<OscWave>,
    #[id = "volume"]
    pub volume: FloatParam,

    // --- Sequencer ---
    #[id = "seq_run"]
    pub seq_run: BoolParam,
    #[id = "seq_sync"]
    pub seq_sync: BoolParam,
    #[id = "seq_tempo"]
    pub seq_tempo: FloatParam,
    #[id = "seq_length"]
    pub seq_length: IntParam,
    #[id = "seq_mode"]
    pub seq_mode: EnumParam<SequencerMode>,
    #[id = "seq_timebase"]
    pub seq_timebase: EnumParam<Timebase>,
    #[id = "seq_jitter"]
    pub seq_jitter: BoolParam,

    // --- Souped-up ---
    #[id = "osc1_pitch"]
    pub osc1_pitch: FloatParam,
    #[id = "osc2_pitch"]
    pub osc2_pitch: FloatParam,
    #[id = "osc2_vol"]
    pub osc2_vol: FloatParam,
    #[id = "osc1_pw"]
    pub osc1_pw: FloatParam,
    #[id = "sub_vol"]
    pub sub_vol: FloatParam,
    #[id = "sweep_pos"]
    pub sweep_pos: FloatParam,
    #[id = "vcf_taps"]
    pub vcf_taps: EnumParam<VcfTaps>,
    #[id = "vcf_drive"]
    pub vcf_drive: FloatParam,
    #[id = "reso_comp"]
    pub reso_comp: FloatParam,
    #[id = "glide"]
    pub glide: FloatParam,
    #[id = "advanced_env"]
    pub advanced_env: BoolParam,

    // MEG ADSR
    #[id = "meg_attack"]
    pub meg_attack: FloatParam,
    #[id = "meg_decay_adj"]
    pub meg_decay_adj: FloatParam,
    #[id = "meg_sustain"]
    pub meg_sustain: FloatParam,
    #[id = "meg_release"]
    pub meg_release: FloatParam,

    // VEG ADSR
    #[id = "veg_attack"]
    pub veg_attack: FloatParam,
    #[id = "veg_decay_adj"]
    pub veg_decay_adj: FloatParam,
    #[id = "veg_sustain"]
    pub veg_sustain: FloatParam,
    #[id = "veg_release"]
    pub veg_release: FloatParam,

    // VCA Env Mode
    #[id = "vca_env_mode"]
    pub vca_env_mode: EnumParam<VcaEnvMode>,

    // Analog
    #[id = "comp_age"]
    pub comp_age: IntParam,
    #[id = "comp_drift"]
    pub comp_drift: FloatParam,
    #[id = "recalibration_scale"]
    pub recalibration_scale: FloatParam,
    #[id = "tolerance"]
    pub tolerance: EnumParam<CompTolerance>,
    #[id = "thermal_noise"]
    pub thermal_noise: IntParam,

    #[id = "comp_seed"]
    pub comp_seed: IntParam,

    // Global WARMCLIPPER output stage; intentionally active in both topologies.
    #[id = "clipper_bypass"]
    pub clipper_bypass: BoolParam,
    #[id = "clipper_os"]
    pub clipper_os: EnumParam<ClipperOversampling>,
    #[id = "clipper_drive"]
    pub clipper_drive: FloatParam,
    #[id = "clipper_threshold"]
    pub clipper_threshold: FloatParam,
    #[id = "clipper_bass"]
    pub clipper_bass: FloatParam,
    #[id = "clipper_tone"]
    pub clipper_tone: FloatParam,
    #[id = "clipper_asym"]
    pub clipper_asym: BoolParam,
    #[id = "clipper_mix"]
    pub clipper_mix: FloatParam,
    #[id = "clipper_in_gain"]
    pub clipper_in_gain: FloatParam,
    #[id = "clipper_out_gain"]
    pub clipper_out_gain: FloatParam,
    #[id = "clipper_gain_link"]
    pub clipper_gain_link: BoolParam,
    #[id = "clipper_tilt"]
    pub clipper_tilt: FloatParam,
    #[id = "clipper_dynamics"]
    pub clipper_dynamics: FloatParam,
    // Sequencer pattern persistence
    #[persist = "pattern"]
    pub pattern: std::sync::RwLock<SequencerPattern>,
}

impl Plugin303Params {
    pub fn is_param_active(&self, param_id: &str) -> bool {
        match self.topology.value() {
            CircuitTopology::Accurate => match param_id {
                "osc1_pitch" | "osc2_pitch" | "osc2_vol" | "osc1_pw" | "sub_vol" | "sweep_pos"
                | "vcf_taps" | "vcf_drive" | "advanced_env" | "meg_attack" | "meg_decay_adj"
                | "meg_sustain" | "meg_release" | "veg_attack" | "veg_decay_adj"
                | "veg_sustain" | "veg_release" | "vca_env_mode" => false,
                _ => true,
            },
            CircuitTopology::Fun => match param_id {
                "waveform" => false,
                _ => true,
            },
        }
    }
}

impl Default for Plugin303Params {
    fn default() -> Self {
        let default_topology = if let Ok(s) = std::env::var("VIBE303_QA_STATE") {
            if s == "accurate" { CircuitTopology::Accurate } else { CircuitTopology::Fun }
        } else {
            CircuitTopology::Fun
        };

        let default_seq_run = if let Ok(s) = std::env::var("VIBE303_QA_STATE") {
            s == "running"
        } else {
            false
        };

        Self {
            tier: EnumParam::new("Tier", Tier::Low),
            topology: EnumParam::new("Circuit Topology", default_topology),

            tuning: FloatParam::new(
                "Tuning",
                0.0,
                FloatRange::Linear {
                    min: -100.0,
                    max: 100.0,
                },
            )
            .with_unit(" cents"),

            cutoff: FloatParam::new("Cutoff", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 }),

            resonance: FloatParam::new("Resonance", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 }),

            env_mod: FloatParam::new("Env Mod", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 }),

            decay: FloatParam::new("Decay", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 }),

            accent: FloatParam::new("Accent", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 }),

            waveform: EnumParam::new("Waveform", OscWave::Saw),

            volume: FloatParam::new(
                "Volume",
                0.0,
                FloatRange::SymmetricalSkewed {
                    min: -90.0,
                    max: 6.0,
                    factor: FloatRange::skew_factor(-2.0),
                    center: 0.0,
                },
            )
            .with_unit(" dB"),

            seq_run: BoolParam::new("Seq Run", default_seq_run),
            seq_sync: BoolParam::new("Seq Sync", true),

            seq_tempo: FloatParam::new(
                "Seq Tempo",
                120.0,
                FloatRange::Linear {
                    min: 40.0,
                    max: 300.0,
                },
            )
            .with_unit(" BPM"),

            seq_length: IntParam::new("Seq Length", 16, IntRange::Linear { min: 1, max: 16 }),

            seq_mode: EnumParam::new("Seq Mode", SequencerMode::DirectGrid),
            seq_timebase: EnumParam::new("Seq Timebase", Timebase::Straight),
            seq_jitter: BoolParam::new("Seq Jitter", false),

            osc1_pitch: FloatParam::new(
                "Osc 1 Pitch",
                0.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            )
            .with_unit(" st"),

            osc2_pitch: FloatParam::new(
                "Osc 2 Pitch",
                0.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            )
            .with_unit(" st"),

            osc2_vol: FloatParam::new(
                "Osc 2 Volume",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            ),

            osc1_pw: FloatParam::new("Osc 1 PW", 0.5, FloatRange::Linear { min: 0.1, max: 0.9 }),

            sub_vol: FloatParam::new("Sub Vol", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 }),

            sweep_pos: FloatParam::new(
                "Sweep Morph",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            ),

            vcf_taps: EnumParam::new("VCF Taps", VcfTaps::FourPole),

            vcf_drive: FloatParam::new(
                "VCF Drive",
                0.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 24.0,
                },
            )
            .with_unit(" dB"),

            reso_comp: FloatParam::new(
                "Reso Gain Comp",
                0.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 1.0,
                },
            ),

            glide: FloatParam::new("Glide", 0.4, FloatRange::Linear { min: 0.0, max: 1.0 }),

            advanced_env: BoolParam::new("Advanced Env", false),

            meg_attack: FloatParam::new(
                "MEG Attack",
                3.0,
                FloatRange::Skewed {
                    min: 0.1,
                    max: 1000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms"),

            meg_decay_adj: FloatParam::new(
                "MEG Decay Adj",
                200.0,
                FloatRange::Skewed {
                    min: 10.0,
                    max: 3000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms"),

            meg_sustain: FloatParam::new(
                "MEG Sustain",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            ),

            meg_release: FloatParam::new(
                "MEG Release",
                200.0,
                FloatRange::Skewed {
                    min: 10.0,
                    max: 3000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms"),

            veg_attack: FloatParam::new(
                "VEG Attack",
                3.0,
                FloatRange::Skewed {
                    min: 0.1,
                    max: 1000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms"),

            veg_decay_adj: FloatParam::new(
                "VEG Decay Adj",
                200.0,
                FloatRange::Skewed {
                    min: 10.0,
                    max: 3000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms"),

            veg_sustain: FloatParam::new(
                "VEG Sustain",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            ),

            veg_release: FloatParam::new(
                "VEG Release",
                200.0,
                FloatRange::Skewed {
                    min: 10.0,
                    max: 3000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms"),

            vca_env_mode: EnumParam::new("VCA Env Mode", VcaEnvMode::Envelope),

            comp_age: IntParam::new(
                "Comp Age",
                0,
                IntRange::Linear {
                    min: (0),
                    max: (100),
                },
            )
            .with_value_to_string(std::sync::Arc::new(|v| format!("{:.1} yrs", v as f32 * 0.45))),

            comp_drift: FloatParam::new(
                "Comp Drift",
                0.1,
                FloatRange::Skewed {
                    min: 0.0,
                    max: 1.0,
                    factor: FloatRange::skew_factor(-1.2),
                },
            )
            .with_value_to_string(std::sync::Arc::new(|v| format!("{:.0} %", v * 100.0))),

            recalibration_scale: FloatParam::new(
                "Recalibration Scale",
                1.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(std::sync::Arc::new(|v| format!("{:.0} %", v * 100.0))),

            tolerance: EnumParam::new("Tolerance", CompTolerance::Zero),

            thermal_noise: IntParam::new("Thermal Noise", 0, IntRange::Linear { min: 0, max: 100 })
                .with_value_to_string(std::sync::Arc::new(|v| {
                    if v == 0 {
                        "Off".to_string()
                    } else {
                        format!("{:.1}°C", 20.0 + v as f32 * 0.25)
                    }
                })),

            comp_seed: IntParam::new(
                "Component Seed",
                1337,
                IntRange::Linear {
                    min: 0,
                    max: 999_999,
                },
            ),

            clipper_bypass: BoolParam::new("Clipper Bypass", false),
            clipper_os: EnumParam::new("Clipper OS", ClipperOversampling::X2),
            clipper_drive: FloatParam::new("Clipper Drive", 47_000.0, FloatRange::Skewed { min: 22_000.0, max: 100_000.0, factor: FloatRange::skew_factor(-1.5) }).with_unit(" ohm"),
            clipper_threshold: FloatParam::new("Clipper Threshold", 4_700.0, FloatRange::Skewed { min: 2_200.0, max: 22_000.0, factor: FloatRange::skew_factor(-1.5) }).with_unit(" ohm"),
            clipper_bass: FloatParam::new("Clipper Bass", 8_200.0, FloatRange::Skewed { min: 4_700.0, max: 22_000.0, factor: FloatRange::skew_factor(-1.5) }).with_unit(" ohm"),
            clipper_tone: FloatParam::new("Clipper Tone", 1e-9, FloatRange::Skewed { min: 470e-12, max: 4.7e-9, factor: FloatRange::skew_factor(-1.5) }).with_value_to_string(std::sync::Arc::new(|v| format!("{:.2} nF", v * 1e9))),
            clipper_asym: BoolParam::new("Clipper Asym", false),
            clipper_mix: FloatParam::new("Clipper Mix", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(std::sync::Arc::new(|v| format!("{:.0} %", v * 100.0))),
            clipper_in_gain: FloatParam::new("Clipper In", 0.0, FloatRange::Linear { min: -18.0, max: 18.0 })
                .with_unit(" dB"),
            clipper_out_gain: FloatParam::new("Clipper Out", 0.0, FloatRange::Linear { min: -18.0, max: 18.0 })
                .with_unit(" dB"),
            clipper_gain_link: BoolParam::new("Clipper Gain Link", true),
            clipper_tilt: FloatParam::new("Clipper Tilt", 0.0, FloatRange::Linear { min: -6.0, max: 6.0 })
                .with_unit(" dB"),
            clipper_dynamics: FloatParam::new("Clipper Dynamics", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(std::sync::Arc::new(|v| format!("{:.0} %", v * 100.0))),

            pattern: std::sync::RwLock::new(SequencerPattern::default()),
        }
    }
}
