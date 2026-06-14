//! Regression: Drive must be cancelled by levelNorm. With Drive up + Age/Bias moved (the user's
//! rig), the old static trim `1/G_v` left the output hot by the full Drive amount (+12 dB), which
//! surfaced as "crazy peaks when sweeping precision". The trim now divides out the known Drive
//! scalar (`1/(G_v*drive)`), so a precision sweep under Drive stays leveled. Mirrors lib.rs's
//! precision-change path: wdf_set_tier (rebuild + arm crossfade) -> reprobe target -> per-sample ramp.
use ba283::dsp::ChannelDsp;
use ba283::wdf::{Precision, QualityConfig};
use ba283::InferenceBackend;

const SR: f32 = 48_000.0;
const BLOCK: usize = 256;

fn base_tiers() -> [(&'static str, QualityConfig); 4] {
    [
        ("realtime", QualityConfig::realtime()),
        ("fast", QualityConfig::fast()),
        ("ultra_fast", QualityConfig::ultra_fast()),
        ("warp", QualityConfig::warp()),
    ]
}

#[test]
fn repro_precision_sweep_all_tiers() {
    for (name, base) in base_tiers() {
        let worst = sweep_one_tier(base);
        eprintln!("### tier {name}: WORST out/in = {worst:+.2} dB");
        assert!(worst < 2.0, "precision sweep peak on tier {name}: {worst:+.2} dB");
    }
}

fn sweep_one_tier(base: QualityConfig) -> f32 {
    // User's rig when it peaks: Drive up, Age/Bias moved, non-realtime tier.
    const RGAIN: f32 = 220.0;
    const BIAS: f32 = 0.7;
    const AGE: f32 = 0.5;
    const DRIVE: f32 = 4.0; // ~+12 dB
    let cfg_for = |p: Precision| base.with_precision(p);
    let mut ch = ChannelDsp::new();
    ch.wdf_rebuild_with(cfg_for(Precision::High), SR, RGAIN, BIAS);
    ch.wdf_set_age(AGE);
    ch.wdf_warmup();

    // Mirror lib.rs: target cancels the circuit gain AND the known Drive scalar (= 1/(G_v*drive)).
    let mut norm_trim_target = (1.0f32 / ch.wdf_measure_gain() / DRIVE).clamp(1.0e-4, 256.0);
    let mut norm_trim = norm_trim_target;

    let f = 220.0f32;
    let w = 2.0 * std::f32::consts::PI * f / SR;
    let amp = 0.5f32; // hot input; with DRIVE -> circuit driven into nonlinearity
    let mut phase = 0usize;
    let mut worst = f32::NEG_INFINITY;

    // RAPID drag: switch precision EVERY block, cycling High->Med->Low->High... for many blocks,
    // and on each switch block do the FULL lib.rs NAN-reset re-apply (tier rebuild + age + seed +
    // trafo + rgain + bias) — all on the freshly-rebuilt model while the crossfade is still active.
    let cycle = [Precision::High, Precision::Medium, Precision::Low];
    let mut last_p = Precision::High;
    for i in 0..60usize {
        let p = cycle[i % 3];
        let mut changed = false;
        if p != last_p {
            ch.wdf_set_tier(cfg_for(p), SR, RGAIN, BIAS);
            // lib.rs NAN-reset: tier change forces age/seed/trafo/rgain/bias to re-apply same block.
            ch.wdf_set_age(AGE);
            ch.wdf_set_age_seed(0xBA283_0000_0001);
            #[cfg(feature = "trafo-core")]
            ch.wdf_set_trafo(0.5);
            ch.wdf_set_rgain(RGAIN);
            ch.wdf_set_bias(BIAS);
            last_p = p;
            changed = true;
        }
        if changed {
            let g = ch.wdf_measure_gain();
            norm_trim_target = (1.0f32 / g / DRIVE).clamp(1.0e-4, 256.0);
        }

        let g_start = norm_trim;
        let g_step = (norm_trim_target - g_start) / BLOCK as f32;
        let (mut in_sq, mut out_sq, mut peak) = (0.0f64, 0.0f64, 0.0f32);
        for k in 0..BLOCK {
            let x = amp * (w * phase as f32).sin();
            phase += 1;
            let g = g_start + g_step * (k as f32 + 1.0);
            let y = ch.process_sample(x, DRIVE, BIAS, 1.0, g, 1.0, false, InferenceBackend::Wdf);
            in_sq += (x * x) as f64;
            out_sq += (y * y) as f64;
            peak = peak.max(y.abs());
        }
        norm_trim = norm_trim_target;
        let db = 10.0 * (out_sq / in_sq).log10();
        worst = worst.max(db as f32);
        if db > 1.0 || peak > amp * 1.3 {
            eprintln!("  blk{i:2} -> {p:?}: out/in={db:+6.2} dB  peak|y|={peak:.3} (in {amp:.3}) target={norm_trim_target:.5}");
        }
    }
    worst
}
