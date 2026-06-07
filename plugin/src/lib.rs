use nih_plug::prelude::*;
use std::num::NonZeroU32;
use std::sync::Arc;

pub mod delay;
pub mod dsp;
pub mod model;
pub mod oversample;
pub mod wdf;

use dsp::ChannelDsp;
use wdf::WdfModel;

/// Convert decibels to a linear gain factor.
#[inline]
fn db_to_gain(db: f32) -> f32 {
    10.0f32.powf(db * 0.05)
}

pub struct Ba283Plugin {
    params: Arc<Ba283Params>,
    /// One per-channel DSP core (model + oversampler + dry-delay) per channel
    /// (mono/stereo). The SAME `ChannelDsp` chain is used by the offline
    /// standalone, so the Phase-4 null target matches the plugin exactly.
    channels: [ChannelDsp; 2],
    /// Host sample rate cached in `initialize`. Default to the model's native
    /// 96 kHz so the no-oversampling path is taken until told otherwise.
    sample_rate: f32,
    /// Last WDF R_KJ gain / bias applied to the channels (block-rate). `NAN` forces a first-block
    /// apply. WDF-only; the NeuralNet path ignores these.
    last_rgain: f32,
    last_bias: f32,
}

/// Which emulation runs per sample. `NeuralNet` is the default (preserves current behaviour); `Wdf`
/// is the circuit-faithful MNA solver. Selectable live in the plugin.
#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum InferenceBackend {
    #[name = "Neural Net"]
    NeuralNet,
    #[name = "Circuit (WDF)"]
    Wdf,
}

#[derive(Params)]
pub struct Ba283Params {
    #[id = "backend"]
    pub backend: EnumParam<InferenceBackend>,
    #[id = "drive_db"]
    pub drive_db: FloatParam,
    #[id = "bias"]
    pub bias: FloatParam,
    #[id = "output_db"]
    pub output_db: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
    /// R_KJ gain shunt (ohms), WDF-only. Inert for the NeuralNet backend. No smoother — consumed at
    /// block rate via a `G0` rebuild (matches the C++ `setBias`/`setRgain`).
    #[id = "circuit_gain"]
    pub circuit_gain: FloatParam,
}

impl Default for Ba283Params {
    fn default() -> Self {
        Self {
            backend: EnumParam::new("Backend", InferenceBackend::NeuralNet),
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
            circuit_gain: FloatParam::new(
                "Circuit Gain",
                220.0,
                FloatRange::Skewed {
                    min: 47.0,
                    max: 10_000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Ohm"),
        }
    }
}

impl Default for Ba283Plugin {
    fn default() -> Self {
        Self {
            params: Arc::new(Ba283Params::default()),
            channels: [ChannelDsp::new(), ChannelDsp::new()],
            sample_rate: 96_000.0,
            last_rgain: f32::NAN,
            last_bias: f32::NAN,
        }
    }
}

impl Ba283Plugin {
    /// Base-rate latency introduced by the 2x oversampler. When the host runs
    /// at >= 96 kHz no oversampling is done, so latency is zero. Below 96 kHz
    /// the upsample -> model -> downsample cascade (63-tap half-band FIR per
    /// direction) has a group delay of 32 base-rate samples (measured by the
    /// `upsample_then_downsample_is_unity` test, which aligns `output[i]` with
    /// `input[i - 32]` at base rate). This is the single source of truth for
    /// the host report and Task 3.4's dry-path delay.
    fn latency_samples(&self) -> u32 {
        if self.sample_rate < 96_000.0 {
            dsp::OS_LATENCY_SAMPLES as u32
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

        // Warm ONE WDF model at the real SR / current gain / bias (both channels are identical), then
        // install its settled snapshot into each channel as the reset target. Off the audio thread —
        // bounded (~1 s of internal-rate solves with early-exit), acceptable at plugin load.
        let rgain = self.params.circuit_gain.value();
        let bias = self.params.bias.value();
        let mut warm = WdfModel::new(self.sample_rate as f64, rgain as f64, bias as f64);
        warm.warmup();
        let snap = warm.snapshot();
        for ch in self.channels.iter_mut() {
            ch.wdf_rebuild(self.sample_rate, rgain, bias);
            ch.wdf_restore(&snap);
        }
        self.last_rgain = rgain;
        self.last_bias = bias;

        // Report the oversampler latency to the host (this nih_plug rev exposes
        // latency via InitContext::set_latency_samples; there is no
        // Plugin::latency_samples trait method).
        context.set_latency_samples(self.latency_samples());
        true
    }

    fn reset(&mut self) {
        for ch in self.channels.iter_mut() {
            ch.reset();
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Full chain via the shared per-channel `ChannelDsp` core: Drive ->
        // model (direct at >= 96 kHz, else via 2x oversampler) -> Output, blended
        // with a latency-aligned dry path by Mix. Params are sample-smoothed.
        let host_ge_96k = self.sample_rate >= 96_000.0;
        let backend = self.params.backend.value();

        // WDF gain/bias are consumed at block rate (a cheap G0 rebuild, not per-sample-smoothed). Apply
        // to both channels only when changed; integrator state is preserved so the model re-settles
        // dynamically. NeuralNet blocks skip this entirely.
        if backend == InferenceBackend::Wdf {
            let rgain = self.params.circuit_gain.value();
            let bias = self.params.bias.value();
            if rgain != self.last_rgain {
                for ch in self.channels.iter_mut() {
                    ch.wdf_set_rgain(rgain);
                }
                self.last_rgain = rgain;
            }
            if bias != self.last_bias {
                for ch in self.channels.iter_mut() {
                    ch.wdf_set_bias(bias);
                }
                self.last_bias = bias;
            }
        }

        for mut frame in buffer.iter_samples() {
            let drive = db_to_gain(self.params.drive_db.smoothed.next());
            let bias = self.params.bias.smoothed.next();
            let out_g = db_to_gain(self.params.output_db.smoothed.next());
            let mix = self.params.mix.smoothed.next() * 0.01;
            for (ch, sample) in frame.iter_mut().enumerate() {
                // AUDIO_IO_LAYOUTS only declare mono/stereo, but guard anyway.
                if ch >= self.channels.len() {
                    break;
                }
                *sample = self.channels[ch]
                    .process_sample(*sample, drive, bias, out_g, mix, host_ge_96k, backend);
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
