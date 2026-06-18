//! One-shot CALIBRATION (not a regression gate): prints the WDF small-signal voltage gain `Gv(R)`
//! across the circuit-gain range and the NN backend's intrinsic flat gain `G_nn`, so the
//! `wdf_gain_lin` table + `NN_FLAT_GAIN` constant in lib.rs can be derived to match WDF absolute.
//! Run: `cargo test -p ba283 --test calib_nn_makeup -- --nocapture --ignored`
use ba283::dsp::ChannelDsp;
use ba283::wdf::QualityConfig;
use ba283::InferenceBackend;

const SR: f32 = 48_000.0;
const BIAS: f32 = 0.7;

#[test]
#[ignore]
fn calibrate() {
    // ---- Gv(R): settled WDF voltage gain across R_KJ (log-spaced incl. the 5 GAIN_STEPS) ----
    let rgains = [
        1.0e9_f32, 10_000.0, 1_200.0, 906.0, 470.0, 220.0, 110.0, 82.0, 47.0,
    ];
    eprintln!("# Gv(R) (WDF realtime, settled):");
    for &r in &rgains {
        let mut ch = ChannelDsp::new();
        ch.wdf_rebuild_with(QualityConfig::realtime(), SR, r, BIAS);
        ch.wdf_warmup();
        let gv = ch.wdf_measure_gain();
        eprintln!("  rgain={r:>10.1}  lnR={:.4}  Gv={gv:.6}  ({:+.2} dB)", r.max(1.0).ln(), 20.0 * gv.log10());
    }

    // ---- G_nn(R): NN intrinsic gain at each gain's FiLM cond (drive=1, off, mix=1, out=1) ----
    // cond = rgain_to_gain_norm(R) (replicated here; private in lib.rs): 10kΩ→0, 82Ω→1, clamped.
    let cond_of = |r: f32| {
        let (lo, hi) = (10_000.0f32.ln(), 82.0f32.ln());
        ((r.max(1.0).ln() - lo) / (hi - lo)).clamp(0.0, 1.0)
    };
    let f = 220.0f32;
    let w = 2.0 * std::f32::consts::PI * f / SR;
    let amp = 0.1f32; // low level → stay in the linear region for an intrinsic-gain read
    let warm = 12_000usize; // > RF/host-rate margin
    let meas = 48_000usize;
    eprintln!("# G_nn(R) (NN, cond=rgain_to_gain_norm(R)):");
    for &r in &rgains {
        let mut ch = ChannelDsp::new();
        let cond = cond_of(r);
        let (mut in_sq, mut out_sq) = (0.0f64, 0.0f64);
        for i in 0..(warm + meas) {
            let x = amp * (w * i as f32).sin();
            let y = ch.process_sample(x, 1.0, cond, 1.0, 1.0, 1.0, false, InferenceBackend::NeuralNet);
            if i >= warm {
                in_sq += (x * x) as f64;
                out_sq += (y * y) as f64;
            }
        }
        let g_nn = (out_sq / in_sq).sqrt() as f32;
        eprintln!("  rgain={r:>10.1}  cond={cond:.4}  G_nn={g_nn:.6}  ({:+.2} dB)", 20.0 * g_nn.log10());
    }
}
