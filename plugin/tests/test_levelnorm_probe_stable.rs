//! Regression: measure_gain must be stable on a drive-wound model. The plugin re-probes the
//! levelNorm trim every glide block by cloning the live model; before the fix the clone carried the
//! saturated audio state, so the probed gain jumped block-to-block under Drive (22x -> 250x at
//! +18 dB, 15 dB spread) and the trim jitter produced loud peaks during a Circuit-Gain sweep with
//! real program material. measure_gain now re-settles a clean operating point before probing.
use ba283::dsp::ChannelDsp;
use ba283::wdf::QualityConfig;
use ba283::InferenceBackend;

const SR: f32 = 48_000.0;

fn probe_spread(amp: f32, drive: f32, label: &str) {
    let mut ch = ChannelDsp::new();
    ch.wdf_rebuild_with(QualityConfig::realtime(), SR, 220.0, 0.7);
    ch.wdf_set_age(0.5);
    ch.wdf_warmup();

    let g_clean = ch.wdf_measure_gain();
    let f = 220.0f32;
    let w = 2.0 * std::f32::consts::PI * f / SR;
    let mut phase = 0usize;

    // Feed loud driven signal; sample measure_gain at many block boundaries, like the live reprobe.
    let (mut gmin, mut gmax) = (f32::INFINITY, f32::NEG_INFINITY);
    for _blk in 0..40 {
        for _ in 0..256 {
            let x = amp * (w * phase as f32).sin();
            phase += 1;
            let _ = ch.process_sample(x, 1.0, drive, 0.7, 0.5, 1.0, 0.04, 1.0, false, InferenceBackend::Wdf);
        }
        let g = ch.wdf_measure_gain();
        gmin = gmin.min(g);
        gmax = gmax.max(g);
    }
    let spread_db = 20.0 * (gmax / gmin).log10();
    let off_clean_db = 20.0 * (gmax.max(g_clean) / gmin.min(g_clean)).log10();
    eprintln!("{label:>26}: g_clean={g_clean:.3} wound g in [{gmin:.3}, {gmax:.3}] spread={spread_db:.2} dB  vs_clean={off_clean_db:.2} dB");
    // measure_gain re-settles a clean operating point before probing, so the probe must be stable
    // regardless of how hard Drive winds the live model — otherwise the trim jitters into peaks
    // during a Circuit-Gain sweep. (Pre-fix this reached 15 dB at +18 dB drive.)
    assert!(spread_db < 0.5, "{label}: probe jitters {spread_db:.2} dB on a drive-wound model");
    assert!(off_clean_db < 0.5, "{label}: wound probe drifts {off_clean_db:.2} dB from clean");
}

#[test]
fn probe_jitter_under_drive() {
    probe_spread(0.3, 1.0, "amp0.3 drive0dB");
    probe_spread(0.5, 4.0, "amp0.5 drive+12dB");
    probe_spread(0.8, 8.0, "amp0.8 drive+18dB");
    probe_spread(0.95, 16.0, "amp0.95 drive+24dB");
}
