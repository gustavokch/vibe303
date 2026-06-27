use nih_plug::prelude::*;
use std::num::NonZeroU32;
use std::sync::Arc;

pub mod dsp;
pub mod params;
use dsp::voice::Voice;
use params::Plugin303Params;

pub struct Plugin303 {
    params: Arc<Plugin303Params>,
    sample_rate: f32,
    voice: Voice,
    /// Last latency reported to the host (samples). Tracked so we only notify on
    /// change — Ultra/High oversampling adds group delay the host must PDC.
    reported_latency: u32,
}

impl Default for Plugin303 {
    fn default() -> Self {
        let sample_rate = 48_000.0;
        Self {
            params: Arc::new(Plugin303Params::default()),
            sample_rate,
            voice: Voice::new(sample_rate),
            reported_latency: 0,
        }
    }
}

impl Plugin for Plugin303 {
    const NAME: &'static str = "TB303";
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
        // Apply the persisted tier (no fade — this is the initial state, not a
        // user change) and report its latency so the host can PDC the
        // oversampling group delay from the first block.
        self.voice.set_tier_immediate(self.params.tier.value());
        self.reported_latency = self.voice.latency_samples() as u32;
        ctx.set_latency_samples(self.reported_latency);
        true
    }

    fn reset(&mut self) {
        // Clear the voice tail so transport stop / loop-jump doesn't bleed.
        self.voice = Voice::new(self.sample_rate);
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Block-rate read of the fidelity tier (starts a crossfade on change).
        self.voice.set_tier(self.params.tier.value());

        // Report PDC only when the tier's latency actually changes, so the host
        // isn't spammed with redundant restarts every block.
        let latency = self.voice.latency_samples() as u32;
        if latency != self.reported_latency {
            context.set_latency_samples(latency);
            self.reported_latency = latency;
        }

        let mut next_event = context.next_event();
        for (sample_idx, mut frame) in buffer.iter_samples().enumerate() {
            // Apply every event scheduled for this sample (sample-accurate).
            while let Some(event) = next_event {
                if event.timing() != sample_idx as u32 {
                    break;
                }
                // deferred (M4/M6): NoteOff gate-off + velocity→accent/VCA. The AD
                // env auto-decays, so a held note is audible without them at M1.
                if let NoteEvent::NoteOn { note, .. } = event {
                    self.voice.note_on(note);
                }
                next_event = context.next_event();
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
    const VST3_CLASS_ID: [u8; 16] = *b"desmen-tb303-vs3";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

impl ClapPlugin for Plugin303 {
    const CLAP_ID: &'static str = "com.desmen.tb303";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Souped-up TB-303 voice");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] =
        &[ClapFeature::Instrument, ClapFeature::Synthesizer];
}

nih_export_clap!(Plugin303);
nih_export_vst3!(Plugin303);
