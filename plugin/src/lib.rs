use nih_plug::prelude::*;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub mod delay;
pub mod dsp;
pub mod levelnorm;
pub mod model;
pub mod oversample;
pub mod wdf;

use dsp::ChannelDsp;
use levelnorm::LevelNorm;
use wdf::{QualityConfig, WdfModel};

/// Time constant (seconds) for the per-block glide of the WDF block-rate params (circuit_gain,
/// bias). Smooths coarse host automation steps into small per-block increments so the G0 rebuild
/// re-settles cleanly instead of stepping. Below these epsilons the glide snaps to the target so it
/// fully settles (no perpetual rebuilds from a never-quite-equal one-pole).
const WDF_PARAM_TAU_S: f32 = 0.02;
const WDF_RGAIN_EPS: f32 = 1.0e-3;
const WDF_BIAS_EPS: f32 = 1.0e-5;
const WDF_AGE_EPS: f32 = 1.0e-5;
#[cfg(feature = "trafo-core")]
const WDF_TRAFO_EPS: f32 = 1.0e-5;

/// Default aging "unit" seed (a fixed nonzero constant so a fresh instance is deterministic).
const DEFAULT_AGE_SEED: u64 = 0xBA283_0000_0001;

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
    /// Last WDF tier applied to the channels (block-rate). A change triggers a rebuild + snapshot
    /// re-install on each channel.
    last_tier: WdfTier,
    /// Last WDF precision applied (block-rate). A change triggers the same rebuild path as a tier
    /// change (the precision is folded into the `QualityConfig` via `with_precision`).
    last_precision: Precision,
    /// Stereo-linked levelNorm gain tracker (shared across channels so the stereo image is
    /// preserved). Always updated; its gain is applied only when the `level_norm` param is on.
    level_norm: LevelNorm,
    /// Per-block-glided WDF R_KJ gain / bias (smooth the coarse block-rate steps into the G0
    /// rebuild). Distinct from the NeuralNet per-sample bias smoother.
    wdf_rgain_smooth: f32,
    wdf_bias_smooth: f32,
    wdf_age_smooth: f32,
    /// Last WDF aging value / seed applied to the channels (block-rate). `NAN`/`MAX` force a
    /// first-block apply. `last_reroll` edge-detects the momentary re-roll trigger.
    last_age: f32,
    last_seed: u64,
    last_reroll: bool,
    #[cfg(feature = "trafo-core")]
    wdf_trafo_smooth: f32,
    #[cfg(feature = "trafo-core")]
    last_trafo: f32,
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

/// WDF solver tier. `Realtime` is the bit-exact reference (all speed levers off); `Fast` bakes in
/// predict + schur + freeze-caps + ls-trim (~5x faster, near-transparent); `UltraFast` additionally
/// drops internal oversampling 2→1 and bypasses the (now-needless) output decimation — ≈2x over
/// fast, and actually *better* than realtime vs SPICE (the os=2 decimation Butterworth deviates
/// from the circuit's true HF response). `Warp` additionally drops the BJT junction charge model
/// (~19 % more per-sample cost gone, ~1.3× over ultra-fast, ~2.6× over fast) — the caps shape only
/// the top octave, so vs SPICE it is equal-or-better than ultra-fast (−31/−37 dB). WDF-only;
/// the NeuralNet backend ignores it. Mirrors the C++ `qualityPreset("realtime"|"fast"|"ultra-fast")`.
#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum WdfTier {
    #[name = "Realtime"]
    Realtime,
    #[name = "Fast"]
    Fast,
    #[name = "Ultra Fast"]
    UltraFast,
    #[name = "Warp"]
    Warp,
}

impl WdfTier {
    fn config(self) -> QualityConfig {
        match self {
            WdfTier::Realtime => QualityConfig::realtime(),
            WdfTier::Fast => QualityConfig::fast(),
            WdfTier::UltraFast => QualityConfig::ultra_fast(),
            WdfTier::Warp => QualityConfig::warp(),
        }
    }
}

/// Lossy precision dial, orthogonal to [`WdfTier`]. `High` is the identity (Realtime+High stays
/// bit-exact / golden); `Medium`/`Low` layer the fixed-step solver + reduced charge model onto the
/// selected tier, trading fidelity for speed within the harness budget (ESR loss ≤ −50 dB vs
/// Realtime+High). WDF-only; the NeuralNet backend ignores it. Maps 1:1 to [`wdf::Precision`].
#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum Precision {
    #[name = "High"]
    High,
    #[name = "Medium"]
    Medium,
    #[name = "Low"]
    Low,
}

impl Precision {
    fn to_wdf(self) -> wdf::Precision {
        match self {
            Precision::High => wdf::Precision::High,
            Precision::Medium => wdf::Precision::Medium,
            Precision::Low => wdf::Precision::Low,
        }
    }
}

#[derive(Params)]
pub struct Ba283Params {
    #[id = "backend"]
    pub backend: EnumParam<InferenceBackend>,
    /// WDF solver tier (Realtime vs Fast). WDF-only; inert for the NeuralNet backend.
    #[id = "wdf_tier"]
    pub wdf_tier: EnumParam<WdfTier>,
    /// Lossy precision dial (High/Medium/Low), orthogonal to the tier. High = no extra loss
    /// (bit-exact at Realtime). WDF-only; inert for the NeuralNet backend.
    #[id = "precision"]
    pub precision: EnumParam<Precision>,
    #[id = "drive_db"]
    pub drive_db: FloatParam,
    #[id = "bias"]
    pub bias: FloatParam,
    #[id = "output_db"]
    pub output_db: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
    /// When on, scale the wet signal so output RMS tracks input RMS (stereo-linked, applied before
    /// `output_db`/`mix`). Off = faithful preamp gain (the default).
    #[id = "level_norm"]
    pub level_norm: BoolParam,
    /// R_KJ gain shunt (ohms), WDF-only. Inert for the NeuralNet backend. Block-rate via a `G0`
    /// rebuild (matches the C++ `setBias`/`setRgain`); the plugin glides it per block to smooth
    /// coarse automation steps.
    #[id = "circuit_gain"]
    pub circuit_gain: FloatParam,
    /// Component aging: random per-component drift (caps + resistors) emulating vintage units.
    /// 0 = pristine (bit-exact today); 1 = fully aged. WDF-only; inert for the NeuralNet backend.
    /// Block-rate via a `G0` rebuild, glided per block like bias/gain.
    #[id = "age"]
    pub age: FloatParam,
    /// Momentary "re-roll unit" trigger: a rising edge picks a new random drift pattern (a different
    /// vintage unit). WDF-only.
    #[id = "age_reroll"]
    pub age_reroll: BoolParam,
    /// Persisted aging seed — the "unit identity". Not an automatable param; serialized with the
    /// plugin state so a saved project recalls the exact same unit. Re-roll mutates it.
    #[persist = "age_seed"]
    pub age_seed: Arc<AtomicU64>,
    /// Transformer magnetic-core saturation/hysteresis amount (Jiles-Atherton). 0 = linear (today's
    /// sound), 0.5 = realistic LO1166, 1 = heavy. SPIKE behind `--features trafo-core`. WDF-only.
    /// NOTE: the default is 0.5 (ACTIVE), not 0 — a `--features trafo-core` build is therefore NOT
    /// bit-exact with the linear core out of the box and pays the JA RK4 cost whenever the knob > 0.
    /// A/B against the linear transformer requires zeroing this knob. (Default release builds omit
    /// the feature entirely, so they are unaffected.)
    #[cfg(feature = "trafo-core")]
    #[id = "trafo"]
    pub trafo: FloatParam,
}

impl Default for Ba283Params {
    fn default() -> Self {
        Self {
            backend: EnumParam::new("Backend", InferenceBackend::NeuralNet),
            wdf_tier: EnumParam::new("WDF Tier", WdfTier::Realtime),
            precision: EnumParam::new("Precision", Precision::High),
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
            level_norm: BoolParam::new("Level Norm", false),
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
            age: FloatParam::new("Age", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(50.0)),
            age_reroll: BoolParam::new("Re-roll Unit", false),
            age_seed: Arc::new(AtomicU64::new(DEFAULT_AGE_SEED)),
            #[cfg(feature = "trafo-core")]
            trafo: FloatParam::new("Trafo", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(50.0)),
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
            last_tier: WdfTier::Realtime,
            last_precision: Precision::High,
            level_norm: LevelNorm::new(),
            wdf_rgain_smooth: 220.0,
            wdf_bias_smooth: 0.5,
            wdf_age_smooth: 0.0,
            last_age: f32::NAN,
            last_seed: u64::MAX,
            last_reroll: false,
            #[cfg(feature = "trafo-core")]
            wdf_trafo_smooth: 0.5,
            #[cfg(feature = "trafo-core")]
            last_trafo: f32::NAN,
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
        let tier = self.params.wdf_tier.value();
        let precision = self.params.precision.value();
        let cfg = tier.config().with_precision(precision.to_wdf());
        // Warm at the realtime tier (the DC operating point is tier-independent — freeze-caps only
        // changes the per-iter companion), then install the settled snapshot into each channel's
        // tier-configured model.
        let mut warm = WdfModel::new(self.sample_rate as f64, rgain as f64, bias as f64);
        warm.warmup();
        let snap = warm.snapshot();
        let age = self.params.age.value();
        let seed = self.params.age_seed.load(Ordering::Relaxed);
        for ch in self.channels.iter_mut() {
            ch.wdf_rebuild_with(cfg, self.sample_rate, rgain, bias);
            ch.wdf_restore(&snap);
            // Re-apply aging after the fresh build (build resets to pristine), then refresh the
            // bias/rgain legs so their aged factor is current.
            ch.wdf_set_age_seed(seed);
            ch.wdf_set_age(age);
            ch.wdf_set_rgain(rgain);
            ch.wdf_set_bias(bias);
        }
        self.last_rgain = rgain;
        self.last_bias = bias;
        self.last_tier = tier;
        self.last_precision = precision;
        self.last_age = age;
        self.last_seed = seed;
        self.last_reroll = self.params.age_reroll.value();
        self.wdf_rgain_smooth = rgain;
        self.wdf_bias_smooth = bias;
        self.wdf_age_smooth = age;
        #[cfg(feature = "trafo-core")]
        {
            let trafo = self.params.trafo.value();
            for ch in self.channels.iter_mut() {
                ch.wdf_set_trafo(trafo);
            }
            self.last_trafo = trafo;
            self.wdf_trafo_smooth = trafo;
        }

        // levelNorm envelope coefficient depends on the host sample rate.
        self.level_norm.prepare(self.sample_rate);

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
        self.level_norm.reset();
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
            // Glide the block-rate WDF params toward their targets so a fast knob drag turns into
            // small per-block G0 rebuilds (smooth re-settle) instead of coarse steps. Snap within an
            // epsilon so the one-pole fully settles (no endless near-equal rebuilds).
            let n = buffer.samples().max(1) as f32;
            let a = 1.0 - (-(n / self.sample_rate) / WDF_PARAM_TAU_S).exp();
            let rgain_t = self.params.circuit_gain.value();
            let bias_t = self.params.bias.value();
            let age_t = self.params.age.value();
            self.wdf_rgain_smooth += a * (rgain_t - self.wdf_rgain_smooth);
            self.wdf_bias_smooth += a * (bias_t - self.wdf_bias_smooth);
            self.wdf_age_smooth += a * (age_t - self.wdf_age_smooth);
            if (rgain_t - self.wdf_rgain_smooth).abs() < WDF_RGAIN_EPS {
                self.wdf_rgain_smooth = rgain_t;
            }
            if (bias_t - self.wdf_bias_smooth).abs() < WDF_BIAS_EPS {
                self.wdf_bias_smooth = bias_t;
            }
            if (age_t - self.wdf_age_smooth).abs() < WDF_AGE_EPS {
                self.wdf_age_smooth = age_t;
            }
            let rgain = self.wdf_rgain_smooth;
            let bias = self.wdf_bias_smooth;
            let age = self.wdf_age_smooth;
            #[cfg(feature = "trafo-core")]
            let trafo = {
                let trafo_t = self.params.trafo.value();
                self.wdf_trafo_smooth += a * (trafo_t - self.wdf_trafo_smooth);
                // Snap to the target within an epsilon so the glide settles and, crucially, lands
                // exactly on 0 (the linear/active boundary) instead of thrashing the partition flip.
                if (trafo_t - self.wdf_trafo_smooth).abs() < WDF_TRAFO_EPS {
                    self.wdf_trafo_smooth = trafo_t;
                }
                self.wdf_trafo_smooth
            };
            // Tier change: rebuild each channel at the new tier and re-install the settled snapshot
            // (no fresh warmup). Block-rate, on actual change only. Reset rgain/bias trackers so the
            // post-rebuild model picks up the current values via the cheap G0 path below.
            let tier = self.params.wdf_tier.value();
            let precision = self.params.precision.value();
            if tier != self.last_tier || precision != self.last_precision {
                let cfg = tier.config().with_precision(precision.to_wdf());
                for ch in self.channels.iter_mut() {
                    ch.wdf_set_tier(cfg, self.sample_rate, rgain, bias);
                }
                self.last_tier = tier;
                self.last_precision = precision;
                // The rebuild reset aging + the bias/rgain legs to pristine — force all to re-apply.
                self.last_age = f32::NAN;
                self.last_seed = u64::MAX;
                self.last_rgain = f32::NAN;
                self.last_bias = f32::NAN;
                // The rebuild also reset the JA core to linear — force trafo to re-apply.
                #[cfg(feature = "trafo-core")]
                {
                    self.last_trafo = f32::NAN;
                }
            }
            // Aging (ordered after tier, before bias/rgain): re-roll edge -> seed -> age. Any change
            // forces the live bias/rgain legs to re-apply with the current aged factor.
            let reroll = self.params.age_reroll.value();
            let mut seed = self.params.age_seed.load(Ordering::Relaxed);
            if reroll && !self.last_reroll {
                seed = wdf::reroll_seed(seed);
                self.params.age_seed.store(seed, Ordering::Relaxed);
            }
            self.last_reroll = reroll;
            if seed != self.last_seed {
                for ch in self.channels.iter_mut() {
                    ch.wdf_set_age_seed(seed);
                }
                self.last_seed = seed;
                self.last_age = f32::NAN;
                self.last_rgain = f32::NAN;
                self.last_bias = f32::NAN;
            }
            if age != self.last_age {
                for ch in self.channels.iter_mut() {
                    ch.wdf_set_age(age);
                }
                self.last_age = age;
                self.last_rgain = f32::NAN;
                self.last_bias = f32::NAN;
            }
            #[cfg(feature = "trafo-core")]
            if trafo != self.last_trafo {
                for ch in self.channels.iter_mut() {
                    ch.wdf_set_trafo(trafo);
                }
                self.last_trafo = trafo;
            }
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

        let level_norm_on = self.params.level_norm.value();
        for mut frame in buffer.iter_samples() {
            let drive = db_to_gain(self.params.drive_db.smoothed.next());
            let bias = self.params.bias.smoothed.next();
            let out_g = db_to_gain(self.params.output_db.smoothed.next());
            let mix = self.params.mix.smoothed.next() * 0.01;
            // Stereo-linked: one gain for the whole frame (read before processing; the estimator is
            // updated after, so it lags by one frame — negligible for a ~75 ms envelope). Applied
            // only when on; the estimator is updated unconditionally so toggling on is seamless.
            let norm_gain = if level_norm_on { self.level_norm.gain() } else { 1.0 };
            let mut in_sq = 0.0f32;
            let mut wet_sq = 0.0f32;
            for (ch, sample) in frame.iter_mut().enumerate() {
                // AUDIO_IO_LAYOUTS only declare mono/stereo, but guard anyway.
                if ch >= self.channels.len() {
                    break;
                }
                let x = *sample;
                in_sq += x * x;
                *sample = self.channels[ch]
                    .process_sample(x, drive, bias, out_g, norm_gain, mix, host_ge_96k, backend);
                let w = self.channels[ch].last_wet();
                wet_sq += w * w;
            }
            self.level_norm.update(in_sq, wet_sq);
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
