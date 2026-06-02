//! Per-channel DSP core shared by the VST3 `process` and the offline standalone.
//!
//! Both the realtime plugin and the offline renderer MUST run the identical
//! signal chain so the Phase-4 null test (standalone vs SPICE) reflects exactly
//! what the plugin does. That chain lives here, in `ChannelDsp`, and is used by
//! both — never duplicated.
//!
//! Chain: `driven = x * drive` -> model (directly at >= 96 kHz, else via the 2x
//! oversampler) -> `* out_gain` = wet. The dry path is delayed by the wet-path
//! latency so the Mix blend stays phase-aligned: the oversampler latency is 32
//! base-rate samples sub-96k (see `Ba283Plugin::latency_samples`, the single
//! source of truth) and 0 at >= 96 kHz (no oversampling, no delay).
//!
//! `process_sample` is zero-alloc: it only calls the existing zero-alloc
//! model/oversampler/delay methods plus arithmetic.

use crate::delay::ShortDelay;
use crate::model::Ba283Model;
use crate::oversample::Oversample2x;

/// Dry-path delay depth on the sub-96k oversampling path. MUST equal
/// `Ba283Plugin::latency_samples()` below 96 kHz (32) so the Mix blend stays
/// phase-aligned. At >= 96 kHz the dry delay is bypassed entirely.
pub const OS_LATENCY_SAMPLES: usize = 32;

pub struct ChannelDsp {
    model: Ba283Model,
    os: Oversample2x,
    dry_delay: ShortDelay<OS_LATENCY_SAMPLES>,
}

impl ChannelDsp {
    pub fn new() -> Self {
        Self {
            model: Ba283Model::new_from_embedded_weights(),
            os: Oversample2x::new(),
            dry_delay: ShortDelay::new(),
        }
    }

    pub fn reset(&mut self) {
        self.model.reset();
        self.os.reset();
        self.dry_delay.reset();
    }

    /// Run one sample through the full chain. Param values are already resolved
    /// (smoothed/constant) and linear: `drive`/`out_gain` are linear gains
    /// (db_to_gain applied by the caller), `mix` is the wet fraction in 0..1.
    pub fn process_sample(
        &mut self,
        x: f32,
        drive: f32,
        bias: f32,
        out_gain: f32,
        mix: f32,
        host_ge_96k: bool,
    ) -> f32 {
        let driven = x * drive;
        let wet = if host_ge_96k {
            self.model.forward(&[driven, bias])[0]
        } else {
            let (u0, u1) = self.os.upsample(driven);
            let w0 = self.model.forward(&[u0, bias])[0];
            let w1 = self.model.forward(&[u1, bias])[0];
            self.os.downsample(w0, w1)
        } * out_gain;
        // Dry path delayed to match the wet-path latency so Mix aligns.
        let dry_aligned = if host_ge_96k {
            x
        } else {
            self.dry_delay.push_pop(x)
        };
        wet * mix + dry_aligned * (1.0 - mix)
    }
}

impl Default for ChannelDsp {
    fn default() -> Self {
        Self::new()
    }
}
