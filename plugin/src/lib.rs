use nih_plug::prelude::*;
use std::num::NonZeroU32;
use std::sync::Arc;

pub mod model;
pub mod oversample;

use model::Ba283Model;
use oversample::Oversample2x;

pub struct Ba283Plugin {
    params: Arc<Ba283Params>,
    /// One GRU instance per channel (mono/stereo). Each holds ~13KB of fixed
    /// arrays; constructing here is fine (no runtime allocation in forward()).
    models: [Ba283Model; 2],
    /// One 2x polyphase oversampler per channel, used only when the host runs
    /// below 96 kHz (the model is trained at 96 kHz).
    os: [Oversample2x; 2],
    /// Host sample rate cached in `initialize`. Default to the model's native
    /// 96 kHz so the no-oversampling path is taken until told otherwise.
    sample_rate: f32,
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
            models: [
                Ba283Model::new_from_embedded_weights(),
                Ba283Model::new_from_embedded_weights(),
            ],
            os: [Oversample2x::new(), Oversample2x::new()],
            sample_rate: 96_000.0,
        }
    }
}

impl Ba283Plugin {
    /// Base-rate latency introduced by the 2x oversampler. When the host runs
    /// at >= 96 kHz no oversampling is done, so latency is zero. Below 96 kHz
    /// the upsample -> model -> downsample cascade has a group delay of 16
    /// base-rate samples (measured by the `upsample_then_downsample_is_unity`
    /// test, which aligns `output[i]` with `input[i - 16]` at base rate).
    fn latency_samples(&self) -> u32 {
        if self.sample_rate < 96_000.0 {
            16
        } else {
            0
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
        buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;
        // Report the oversampler latency to the host (this nih_plug rev exposes
        // latency via InitContext::set_latency_samples; there is no
        // Plugin::latency_samples trait method).
        context.set_latency_samples(self.latency_samples());
        true
    }

    fn reset(&mut self) {
        for m in self.models.iter_mut() {
            m.reset();
        }
        for o in self.os.iter_mut() {
            o.reset();
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Model + 2x oversampling path. Drive/Output/Mix/dry-delay land in 3.4.
        // At >= 96 kHz the model runs directly; below 96 kHz we upsample 2x,
        // run the model on each oversampled sample, then downsample.
        let host_ge_96k = self.sample_rate >= 96_000.0;
        for mut frame in buffer.iter_samples() {
            let bias = self.params.bias.smoothed.next();
            for (ch, sample) in frame.iter_mut().enumerate() {
                // AUDIO_IO_LAYOUTS only declare mono/stereo, but guard anyway.
                if ch >= self.models.len() {
                    break;
                }
                let dry = *sample;
                let wet = if host_ge_96k {
                    self.models[ch].forward(&[dry, bias])[0]
                } else {
                    let (u0, u1) = self.os[ch].upsample(dry);
                    let w0 = self.models[ch].forward(&[u0, bias])[0];
                    let w1 = self.models[ch].forward(&[u1, bias])[0];
                    self.os[ch].downsample(w0, w1)
                };
                *sample = wet;
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
