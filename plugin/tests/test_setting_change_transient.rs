//! Issue-2 repro: changing tier / precision / age mid-stream must not produce a loud transient.
//!
//! The live path (`lib.rs::process`) reacts to a tier/precision change by calling
//! `ChannelDsp::wdf_set_tier` — which reallocates the `WdfModel`, restores the carried snapshot, and
//! ZEROES the decimation-biquad + upsample state (`WdfModel::restore`) — then forces the age/rgain/
//! bias legs to re-stamp. Each of these is a discontinuity in the output; with no crossfade or DC
//! blocker on the chain, the user hears "VERY loud clicks". This test streams a steady tone, fires
//! each change mid-stream, and measures the post-change peak vs the steady-state peak.

use ba283::dsp::ChannelDsp;
use ba283::wdf::{Precision, QualityConfig};
use ba283::InferenceBackend;

const SR: f64 = 48_000.0;
const DEFAULT_AGE_SEED: u64 = 0xBA283_0000_0001;

/// Steady −12 dBFS 220 Hz tone (host samples).
fn tone(n: usize) -> Vec<f32> {
    let amp = 0.25_f32; // ~ −12 dBFS
    let w = 2.0 * std::f32::consts::PI * 220.0 / SR as f32;
    (0..n).map(|i| amp * (w * i as f32).sin()).collect()
}

/// Plugin disable->enable scenario: the host re-activates the plugin (calls `reset()`) with audio
/// running. Without a resume fade the full-gain circuit wet snaps on instantly (~6.0) = a VERY loud
/// click. The fade must ramp the wet in: the first post-reset output is near zero and grows
/// smoothly, with no large sample-to-sample jump.
#[test]
fn resume_after_reset_ramps_in() {
    let (rgain, bias) = (220.0f32, 0.5f32);
    let settle = (SR * 0.2) as usize;
    let window = (SR * 0.05) as usize;

    let mut ch = ChannelDsp::new();
    ch.wdf_rebuild(SR as f32, rgain, bias);
    ch.wdf_warmup();
    // Run a tone so the wet is at its full steady level.
    let mut h = Harness { ch, drive: 1.0, bias, last: 0.0 };
    let steady_peak = h.stream_peak(settle);
    assert!(steady_peak > 1.0, "expected loud steady wet, got {steady_peak}");

    // Host disable->enable: reset() then resume processing the tone.
    h.ch.reset();
    let t = tone(window);
    let (mut first, mut max_jump, mut prev) = (f32::NAN, 0.0f32, 0.0f32);
    for (i, &x) in t.iter().enumerate() {
        let y = h.ch.process_sample(x, 1.0, bias, 1.0, 1.0, 1.0, false, InferenceBackend::Wdf, false, 0.0);
        if i == 0 {
            first = y;
        }
        max_jump = max_jump.max((y - prev).abs());
        prev = y;
    }
    eprintln!("[resume after reset] first_out={first:.5} max_jump={max_jump:.5} steady_peak={steady_peak:.4}");
    // The wet must ramp in, not snap: first output near zero, and no jump near the ~6.0 steady level.
    assert!(first.abs() < 0.5, "wet snapped on at reset (first_out {first:.4}); resume fade missing");
    assert!(max_jump < 0.6, "resume produced a click: max_jump {max_jump:.4}");
}

/// Transport-stopped scenario: switch precision High->Medium with SILENT input. With no signal to
/// mask it, any fade-boundary corner or operating-point step is an audible click. Measures the max
/// |output| and max sample-to-sample jump over the post-switch window — both must stay tiny.
#[test]
fn precision_change_silent_no_click() {
    let (rgain, bias) = (220.0f32, 0.5f32);
    let settle = (SR * 0.2) as usize;
    let window = (SR * 0.05) as usize;

    let mut ch = ChannelDsp::new();
    ch.wdf_rebuild(SR as f32, rgain, bias);
    ch.wdf_warmup();
    // Onto Fast+High, settle in silence.
    ch.wdf_set_tier(QualityConfig::fast().with_precision(Precision::High), SR as f32, rgain, bias);
    ch.wdf_set_age_seed(DEFAULT_AGE_SEED);
    ch.wdf_set_age(0.0);
    ch.wdf_set_rgain(rgain);
    ch.wdf_set_bias(bias);
    let mut prev = 0.0f32;
    for _ in 0..settle {
        prev = ch.process_sample(0.0, 1.0, bias, 1.0, 1.0, 1.0, false, InferenceBackend::Wdf, false, 0.0);
    }
    // Switch to Fast+Medium (the reported click) while silent.
    ch.wdf_set_tier(QualityConfig::fast().with_precision(Precision::Medium), SR as f32, rgain, bias);
    ch.wdf_set_age_seed(DEFAULT_AGE_SEED);
    ch.wdf_set_age(0.0);
    ch.wdf_set_rgain(rgain);
    ch.wdf_set_bias(bias);
    let (mut peak, mut max_jump) = (0.0f32, 0.0f32);
    for _ in 0..window {
        let y = ch.process_sample(0.0, 1.0, bias, 1.0, 1.0, 1.0, false, InferenceBackend::Wdf, false, 0.0);
        peak = peak.max(y.abs());
        max_jump = max_jump.max((y - prev).abs());
        prev = y;
    }
    eprintln!("[silent precision switch] post_peak={peak:.5} max_jump={max_jump:.5}");
    // In silence the output should stay near zero with no step. Threshold well below a −40 dBFS tick.
    assert!(max_jump < 0.01, "silent precision switch clicks: max_jump {max_jump:.5} (want < 0.01)");
    assert!(peak < 0.02, "silent precision switch peak too high: {peak:.5}");
}

struct Harness {
    ch: ChannelDsp,
    drive: f32,
    bias: f32,
    last: f32,
}

impl Harness {
    /// Warm at Realtime+High and settle through `settle` samples; return the steady-state peak.
    fn new(rgain: f32, bias: f32, settle: usize) -> (Self, f32) {
        let mut ch = ChannelDsp::new();
        ch.wdf_rebuild(SR as f32, rgain, bias);
        ch.wdf_warmup();
        let drive = 1.0;
        let mut h = Self { ch, drive, bias, last: 0.0 };
        let peak = h.stream_peak(settle);
        (h, peak)
    }

    /// Stream `n` tone samples, return the peak |output|.
    fn stream_peak(&mut self, n: usize) -> f32 {
        self.stream(n).0
    }

    /// Stream `n` tone samples; return (peak |output|, max |y[n]-y[n-1]| sample-to-sample jump).
    /// The jump captures a click/step discontinuity that the magnitude peak alone misses.
    fn stream(&mut self, n: usize) -> (f32, f32) {
        let t = tone(n);
        let (mut peak, mut max_jump, mut prev) = (0.0f32, 0.0f32, self.last);
        for &x in &t {
            let y = self
                .ch
                .process_sample(x, self.drive, self.bias, 1.0, 1.0, 1.0, false, InferenceBackend::Wdf, false, 0.0);
            peak = peak.max(y.abs());
            max_jump = max_jump.max((y - prev).abs());
            prev = y;
        }
        self.last = prev;
        (peak, max_jump)
    }
}

/// Measure the post-change transient peak for each setting change. Currently diagnostic — prints
/// the peaks so we can see which source dominates and size the crossfade fix. Asserts only that the
/// model stays finite/bounded through the change (the divergence guard from Issue 1).
#[test]
fn setting_change_transients() {
    let (rgain, bias) = (220.0f32, 0.5f32);
    let settle = (SR * 0.2) as usize;
    let window = (SR * 0.05) as usize; // 50 ms post-change peak window

    let mut report = String::new();
    let mut record = |name: &str, steady_jump: f32, post: (f32, f32)| {
        let (post_peak, post_jump) = post;
        let jump_ratio = post_jump / steady_jump.max(1e-9);
        report.push_str(&format!(
            "\n  {name:24} steady_jump={steady_jump:.4} post_jump={post_jump:.4} \
             (x{jump_ratio:.1}) post_peak={post_peak:.4}"
        ));
        (post_peak, post_jump, jump_ratio)
    };

    let mut worst_ratio = 0.0f32;

    // 1. Tier change: Realtime+High -> Fast+High.
    {
        let (mut h, _) = Harness::new(rgain, bias, settle);
        let (_, steady_jump) = h.stream(window);
        let cfg = QualityConfig::fast().with_precision(Precision::High);
        h.ch.wdf_set_tier(cfg, SR as f32, rgain, bias);
        h.ch.wdf_set_age_seed(DEFAULT_AGE_SEED);
        h.ch.wdf_set_age(0.0);
        h.ch.wdf_set_rgain(rgain);
        h.ch.wdf_set_bias(bias);
        let post = h.stream(window);
        let (_, _, r) = record("tier (RT->Fast)", steady_jump, post);
        worst_ratio = worst_ratio.max(r);
        assert!(post.0.is_finite() && post.0 < 1e3, "tier change diverged: {:.3e}", post.0);
    }
    // 2. Precision change: Fast+High -> Fast+Medium.
    {
        let (mut h, _) = Harness::new(rgain, bias, settle);
        h.ch.wdf_set_tier(QualityConfig::fast().with_precision(Precision::High), SR as f32, rgain, bias);
        h.ch.wdf_set_age_seed(DEFAULT_AGE_SEED);
        h.ch.wdf_set_age(0.0);
        h.ch.wdf_set_rgain(rgain);
        h.ch.wdf_set_bias(bias);
        let _ = h.stream_peak(settle); // re-settle on Fast+High
        let (_, steady_jump) = h.stream(window);
        h.ch.wdf_set_tier(QualityConfig::fast().with_precision(Precision::Medium), SR as f32, rgain, bias);
        h.ch.wdf_set_age_seed(DEFAULT_AGE_SEED);
        h.ch.wdf_set_age(0.0);
        h.ch.wdf_set_rgain(rgain);
        h.ch.wdf_set_bias(bias);
        let post = h.stream(window);
        let (_, _, r) = record("precision (H->Med)", steady_jump, post);
        worst_ratio = worst_ratio.max(r);
        assert!(post.0.is_finite() && post.0 < 1e3, "precision change diverged: {:.3e}", post.0);
    }
    // 3. Age re-roll (seed change).
    {
        let (mut h, _) = Harness::new(rgain, bias, settle);
        let (_, steady_jump) = h.stream(window);
        h.ch.wdf_set_age_seed(ba283::wdf::reroll_seed(DEFAULT_AGE_SEED));
        let post = h.stream(window);
        let (_, _, r) = record("age re-roll", steady_jump, post);
        worst_ratio = worst_ratio.max(r);
        assert!(post.0.is_finite() && post.0 < 1e3, "age re-roll diverged: {:.3e}", post.0);
    }

    eprintln!("[setting-change transients] @ 48kHz steady −12 dBFS tone:{report}\n  worst jump ratio: x{worst_ratio:.1}");

    // With the equal-power crossfade + DC blocker, no setting change may produce a sample-to-sample
    // jump much larger than the steady-state jump (a "click"). Pre-fix the precision change was x29.
    assert!(
        worst_ratio < 4.0,
        "setting change still clicks: worst post/steady jump ratio x{worst_ratio:.1} (want < 4.0)"
    );
}
