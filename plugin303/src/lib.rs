use nih_plug::prelude::*;
use std::num::NonZeroU32;
use std::sync::Arc;

pub mod dsp;
pub mod params;
use dsp::sequencer::{Sequencer, TransportInfo};
use dsp::voice::Voice;
use params::Plugin303Params;
/// The main plugin structure implementing the nih-plug VST3/CLAP synth interface.
pub struct Plugin303 {
    /// Shared parameters object.
    params: Arc<Plugin303Params>,
    /// Active sample rate.
    sample_rate: f32,
    /// Monophonic synth voice.
    voice: Voice,
    /// Last latency reported to the host (samples). Tracked so we only notify on
    /// change — Ultra/High oversampling adds group delay the host must PDC.
    reported_latency: u32,
    /// Built-in 16-step sequencer.
    sequencer: Sequencer,
    /// Tracked active MIDI note to manage note priority.
    active_midi_note: Option<u8>,
}

impl Default for Plugin303 {
    fn default() -> Self {
        let sample_rate = 48_000.0;
        Self {
            params: Arc::new(Plugin303Params::default()),
            sample_rate,
            voice: Voice::new(sample_rate),
            reported_latency: 0,
            sequencer: Sequencer::new(),
            active_midi_note: None,
        }
    }
}

const fn str_eq(a: &str, b: &str) -> bool {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    if ab.len() != bb.len() {
        return false;
    }
    let mut i = 0;
    while i < ab.len() {
        if ab[i] != bb[i] {
            return false;
        }
        i += 1;
    }
    true
}

const fn hash_to_16_bytes(s: &str) -> [u8; 16] {
    let bytes = s.as_bytes();
    let mut hash1 = 0xcbf29ce484222325u64;
    let mut hash2 = 0x811c9dc5u64;
    let mut i = 0;
    while i < bytes.len() {
        hash1 ^= bytes[i] as u64;
        hash1 = hash1.wrapping_mul(0x100000001b3u64);

        hash2 ^= bytes[i] as u64;
        hash2 = hash2.wrapping_mul(0x100000001b3u64);
        i += 1;
    }
    let h1_bytes = hash1.to_ne_bytes();
    let h2_bytes = hash2.to_ne_bytes();
    let mut arr = [0u8; 16];
    let mut j = 0;
    while j < 8 {
        arr[j] = h1_bytes[j];
        arr[j + 8] = h2_bytes[j];
        j += 1;
    }
    arr
}

const fn parse_vst3_class_id(name: &str, custom_id: Option<&str>) -> [u8; 16] {
    match custom_id {
        Some(cid) => {
            let bytes = cid.as_bytes();
            if bytes.len() == 16 {
                let mut arr = [0u8; 16];
                let mut i = 0;
                while i < 16 {
                    arr[i] = bytes[i];
                    i += 1;
                }
                arr
            } else {
                hash_to_16_bytes(cid)
            }
        }
        None => {
            if str_eq(name, "TB303") {
                *b"desmen-tb303-vs3"
            } else {
                hash_to_16_bytes(name)
            }
        }
    }
}

const PLUGIN_NAME: &'static str = match option_env!("VIBE303_NAME") {
    Some(name) => name,
    None => "TB303",
};

const CLAP_ID: &'static str = match option_env!("VIBE303_CLAP_ID") {
    Some(id) => id,
    None => "com.desmen.tb303",
};

const VST3_CLASS_ID: [u8; 16] =
    parse_vst3_class_id(PLUGIN_NAME, option_env!("VIBE303_VST3_CLASS_ID"));

impl Plugin for Plugin303 {
    const NAME: &'static str = PLUGIN_NAME;
    const VENDOR: &'static str = "desmen";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _layout: &AudioIOLayout,
        cfg: &BufferConfig,
        ctx: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = cfg.sample_rate;
        self.voice = Voice::new(self.sample_rate);
        self.sequencer = Sequencer::new();
        self.active_midi_note = None;
        // Apply the persisted tier (no fade — this is the initial state, not a
        // user change) and report its latency so the host can PDC the
        // oversampling group delay from the first block.
        self.voice.set_tier_immediate(self.params.tier.value());
        self.reported_latency = self.voice.latency_samples() as u32;
        ctx.set_latency_samples(self.reported_latency);
        true
    }

    fn reset(&mut self) {
        // Clear the voice tail so transport stop / loop-jump doesn't bleed. Reset
        // is in place (not `Voice::new`): nih-plug may call this from the audio
        // thread, where allocating / rebuilding the VCO pitch LUT is illegal —
        // see `Voice::reset`. The LUT cache is warmed once in `initialize`.
        self.voice.reset();
        self.sequencer = Sequencer::new();
        self.active_midi_note = None;
        // Re-apply the persisted tier immediately (no fade) — otherwise the voice
        // would silently revert to Low and the next block would crossfade up to
        // the user's tier on every transport start / loop-jump.
        self.voice.set_tier_immediate(self.params.tier.value());
    }
    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Expose parameters block-rate
        self.voice.update_params(
            self.params.tuning.value(),
            self.params.osc1_pitch.value(),
            self.params.osc2_pitch.value(),
            self.params.osc2_vol.value(),
            self.params.waveform.value(),
            self.params.osc1_pw.value(),
            self.params.sub_vol.value(),
            self.params.cutoff.value(),
            self.params.resonance.value(),
            self.params.env_mod.value(),
            self.params.decay.value(),
            self.params.accent.value(),
            self.params.volume.value(),
            self.params.vcf_taps.value(),
            self.params.vcf_drive.value(),
            self.params.advanced_env.value(),
            self.params.meg_attack.value(),
            self.params.meg_decay_adj.value(),
            self.params.meg_sustain.value(),
            self.params.meg_release.value(),
            self.params.veg_attack.value(),
            self.params.veg_decay_adj.value(),
            self.params.veg_sustain.value(),
            self.params.veg_release.value(),
            self.params.vca_env_mode.value(),
            self.params.comp_age.value(),
            self.params.comp_drift.value(),
            self.params.tolerance.value(),
            self.params.thermal_noise.value(),
            self.params.topology.value(),
            self.params.sweep_pos.value(),
            self.params.comp_seed.value(),
            self.params.glide.value(),
        );

        // Block-rate read of the fidelity tier (starts a crossfade on change).
        self.voice.set_tier(self.params.tier.value());

        // Report PDC only when the tier's latency actually changes, so the host
        // isn't spammed with redundant restarts every block.
        let latency = self.voice.latency_samples() as u32;
        if latency != self.reported_latency {
            context.set_latency_samples(latency);
            self.reported_latency = latency;
        }

        let seq_run = self.params.seq_run.value();
        let pattern_steps = if seq_run {
            self.params.pattern.read().ok().map(|p| p.steps)
        } else {
            None
        };
        let seq_length = self.params.seq_length.value() as usize;
        let seq_tempo = self.params.seq_tempo.value();
        let seq_sync = self.params.seq_sync.value();
        let glide_val = self.params.glide.value();

        let mut next_event = context.next_event();
        let mut active_midi_note = self.active_midi_note;

        for (sample_idx, mut frame) in buffer.iter_samples().enumerate() {
            // Apply every event scheduled for this sample (sample-accurate).
            while let Some(event) = next_event {
                if event.timing() != sample_idx as u32 {
                    break;
                }
                match event {
                    NoteEvent::NoteOn { note, velocity, .. } => {
                        let is_legato = active_midi_note.is_some();
                        active_midi_note = Some(note);
                        let slide = is_legato && glide_val > 0.0;
                        let trigger = !is_legato;
                        self.voice.set_gate(true, trigger, velocity >= 0.78, slide);
                        self.voice.note_on(note);
                    }
                    NoteEvent::NoteOff { note, .. } => {
                        if active_midi_note == Some(note) {
                            active_midi_note = None;
                            self.voice.set_gate(false, false, false, false);
                        }
                    }
                    _ => {}
                }
                next_event = context.next_event();
            }
            self.active_midi_note = active_midi_note;

            // If no MIDI note is held, run the sequencer
            if active_midi_note.is_none() && seq_run {
                if let Some(steps) = &pattern_steps {
                    let transport = context.transport();
                    let transport_info = TransportInfo {
                        playing: transport.playing,
                        pos_beats: transport.pos_beats(),
                        tempo: transport.tempo,
                    };
                    self.sequencer.tick(
                        self.sample_rate,
                        &transport_info,
                        steps,
                        seq_length,
                        seq_tempo,
                        seq_sync,
                        glide_val,
                    );
                    self.voice.set_gate(
                        self.sequencer.gate,
                        self.sequencer.trigger,
                        self.sequencer.accent,
                        self.sequencer.slide,
                    );
                    self.voice.set_freq(self.sequencer.freq);
                }
            }

            let out = self.voice.tick();
            for s in frame.iter_mut() {
                *s = out;
            }
        }
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for Plugin303 {
    const VST3_CLASS_ID: [u8; 16] = VST3_CLASS_ID;
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

impl ClapPlugin for Plugin303 {
    const CLAP_ID: &'static str = CLAP_ID;
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Souped-up TB-303 voice");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] =
        &[ClapFeature::Instrument, ClapFeature::Synthesizer];
}

nih_export_clap!(Plugin303);
nih_export_vst3!(Plugin303);
