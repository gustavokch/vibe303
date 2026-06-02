use nih_plug::prelude::*;
use std::num::NonZeroU32;
use std::sync::Arc;

pub struct Ba283Plugin {
    params: Arc<Ba283Params>,
}

#[derive(Params)]
pub struct Ba283Params {
    #[id = "drive_db"]
    pub drive_db: FloatParam,
    #[id = "bias"]
    pub bias: FloatParam,
    #[id = "output_db"]
    pub output_db: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
}

impl Default for Ba283Params {
    fn default() -> Self {
        Self {
            drive_db: FloatParam::new(
                "Drive",
                0.0,
                FloatRange::Linear {
                    min: -12.0,
                    max: 24.0,
                },
            )
            .with_unit(" dB")
            .with_smoother(SmoothingStyle::Linear(20.0)),
            bias: FloatParam::new("Bias", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(50.0)),
            output_db: FloatParam::new(
                "Output",
                0.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 12.0,
                },
            )
            .with_unit(" dB")
            .with_smoother(SmoothingStyle::Linear(20.0)),
            mix: FloatParam::new(
                "Mix",
                100.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 100.0,
                },
            )
            .with_unit(" %")
            .with_smoother(SmoothingStyle::Linear(20.0)),
        }
    }
}

impl Default for Ba283Plugin {
    fn default() -> Self {
        Self {
            params: Arc::new(Ba283Params::default()),
        }
    }
}

impl Plugin for Ba283Plugin {
    const NAME: &'static str = "BA283";
    const VENDOR: &'static str = "gruber.dev.br";
    const URL: &'static str = "";
    const EMAIL: &'static str = "contact@gruber.dev.br";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        _buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        true
    }

    fn reset(&mut self) {}

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Identity passthrough placeholder; replaced by the model chain in Task 3.2.
        #[allow(clippy::self_assignment)]
        for mut frame in buffer.iter_samples() {
            for sample in frame.iter_mut() {
                *sample = *sample;
            }
        }
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for Ba283Plugin {
    const VST3_CLASS_ID: [u8; 16] = *b"ba283-neve-vst3.";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

impl ClapPlugin for Ba283Plugin {
    const CLAP_ID: &'static str = "br.dev.gruber.ba283";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Neve BA283 neural emulation");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect];
}

nih_export_vst3!(Ba283Plugin);
