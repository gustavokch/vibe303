//! Diagnostic: which VCF brightness each nodal tier really plays, and what it
//! does to the sound. Run before and after a brightness change and compare:
//!
//!   cargo test -p plugin303 --release --test brightness_audit -- \
//!       --ignored --nocapture --test-threads=1
//!
//! Brightness is the `cal` factor on the ladder's cutoff→bias map. Intended
//! values: Low `LOW_BRIGHT`, High/Best `CIRCUIT_BRIGHT`, each scaled by the
//! analog-wear age factor `1 - 0.15 * age/100 * recalibration_scale`.

mod common;

use common::{apply_plugin_defaults, gold_dir, load_f32, load_gold_op, spectral_match, GoldOp};
use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::{Voice, CIRCUIT_BRIGHT, LOW_BRIGHT};
use plugin303::params::VcaEnvMode;

const SR: f32 = 48_000.0;

fn print_cals(label: &str, v: &Voice) {
    let [low, high, best] = v.dbg_ladder_cals();
    println!("{label:<36} low {low:.4}  high {high:.4}  best {best:.4}");
}

/// Overrides the brightness of `tier`'s ladder. `None` keeps the voice's state.
fn override_brightness(v: &mut Voice, tier: Tier, bright: Option<f64>) {
    match (tier, bright) {
        (_, None) | (Tier::Draft, _) => {}
        (Tier::Low, Some(c)) => v.set_low_brightness(c),
        (Tier::High | Tier::Best, Some(c)) => v.set_circuit_brightness(c),
    }
}

#[test]
#[ignore = "diagnostic: effective ladder brightness per scenario"]
fn a_brightness_state() {
    println!("== A. Effective brightness (cal) ==");
    println!("intended (age 0)                     low {LOW_BRIGHT:.4}  high {CIRCUIT_BRIGHT:.4}  best {CIRCUIT_BRIGHT:.4}");
    let mut v = Voice::new(SR);
    print_cals("Voice::new", &v);
    for tier in [Tier::Low, Tier::High, Tier::Best] {
        v.set_tier_immediate(tier);
    }
    print_cals("after set_tier_immediate Low/High/Best", &v);
    apply_plugin_defaults(&mut v, 0);
    print_cals("plugin defaults (seed 1337, age 0)", &v);
    apply_plugin_defaults(&mut v, 50);
    print_cals("plugin defaults, age 50", &v);
    v.reset();
    apply_plugin_defaults(&mut v, 0);
    print_cals("reset + plugin defaults", &v);
}

/// Renders `tier` at the gold operating point the way the spectral gates do.
/// `plugin` first applies the plugin's default param update (analog wear).
fn render_gold_op(tier: Tier, bright: Option<f64>, plugin: bool, op: &GoldOp, n: usize) -> Vec<f32> {
    let mut v = Voice::new(op.sr);
    v.update_clipper_params_immediate(common::bypassed_clipper());
    v.set_tier_immediate(tier);
    if plugin {
        apply_plugin_defaults(&mut v, 0);
    }
    override_brightness(&mut v, tier, bright);
    v.set_advanced_env(true);
    v.set_cutoff(op.cutoff);
    v.set_reso(op.reso);
    v.note_on(op.note);
    for _ in 0..48 {
        v.tick();
    }
    (0..n).map(|_| v.tick()).collect()
}

fn rms_db(x: &[f32]) -> f32 {
    let p: f64 = x.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / x.len() as f64;
    10.0 * p.max(1e-30).log10() as f32
}

#[test]
#[ignore = "diagnostic: spectral match vs SPICE gold per tier and brightness"]
fn b_spectral_vs_gold() {
    println!("== B. Spectral match vs gold voice_A2_mid (LSD dB, corr, RMS dBFS) ==");
    let gold = load_f32(&gold_dir().join("voice_A2_mid.f32"));
    let op = load_gold_op("voice_A2_mid.json");
    let rows: [(Tier, &str, Option<f64>, bool); 12] = [
        (Tier::Low, "voice state", None, false),
        (Tier::Low, "plugin defaults", None, true),
        (Tier::Low, "cal 0.50", Some(0.50), false),
        (Tier::Low, "cal 1.00", Some(1.00), false),
        (Tier::High, "voice state", None, false),
        (Tier::High, "plugin defaults", None, true),
        (Tier::High, "cal 0.70", Some(0.70), false),
        (Tier::High, "cal 1.00", Some(1.00), false),
        (Tier::Best, "voice state", None, false),
        (Tier::Best, "plugin defaults", None, true),
        (Tier::Best, "cal 0.70", Some(0.70), false),
        (Tier::Best, "cal 1.00", Some(1.00), false),
    ];
    for (tier, label, bright, plugin) in rows {
        let m = render_gold_op(tier, bright, plugin, &op, gold.len());
        let (lsd, corr) = spectral_match(&gold, &m, op.sr);
        println!(
            "{:<5} {label:<16} LSD {lsd:6.2}  corr {corr:.3}  rms {:6.1}",
            format!("{tier:?}"),
            rms_db(&m)
        );
    }
}

/// Gate-mode RMS of one tier, as `tests/tier_levels.rs` measures it.
fn tier_rms(tier: Tier, bright: Option<f64>, note: u8, cutoff: f32, reso: f32) -> f32 {
    let mut v = Voice::new(SR);
    v.set_tier_immediate(tier);
    override_brightness(&mut v, tier, bright);
    v.set_vca_env_mode(VcaEnvMode::Gate);
    v.set_cutoff(cutoff);
    v.set_reso(reso);
    v.set_gate(true, true, false, false);
    v.note_on(note);
    for _ in 0..400 {
        v.tick();
    }
    let n = 8_000;
    let acc: f64 = (0..n).map(|_| (v.tick() as f64).powi(2)).sum();
    (acc / n as f64).sqrt() as f32
}

/// (label, Low brightness, Best brightness). `None` = voice state.
const COMBOS: [(&str, Option<f64>, Option<f64>); 4] = [
    ("voice state", None, None),
    ("plugin today (1.00/1.00)", Some(1.00), Some(1.00)),
    ("tests today (0.50/1.00)", Some(0.50), Some(1.00)),
    ("intended (0.50/0.70)", Some(0.50), Some(0.70)),
];

#[test]
#[ignore = "diagnostic: Low vs Best level at the tier_levels patches"]
fn c_low_vs_best_level() {
    println!("== C. Low vs Best level, dB (tier_levels::low_tracks_best_level window ±2.5) ==");
    let patches = [(45u8, 1_500.0f32, 0.3f32), (33, 800.0, 0.5), (57, 3_000.0, 0.2), (57, 1_002.0, 0.85)];
    for (label, low_c, best_c) in COMBOS {
        let dbs: Vec<f32> = patches
            .iter()
            .map(|&(note, fc, r)| {
                20.0 * (tier_rms(Tier::Low, low_c, note, fc, r) / tier_rms(Tier::Best, best_c, note, fc, r)).log10()
            })
            .collect();
        let mean = dbs.iter().sum::<f32>() / dbs.len() as f32;
        println!("{label:<26} {:?}  mean {mean:+.2}", dbs.iter().map(|d| format!("{d:+.2}")).collect::<Vec<_>>());
    }
}

/// Envelope sweep, as `tests/tier_calibration.rs::render_sweep` renders it.
fn render_sweep(tier: Tier, bright: Option<f64>, n: usize) -> Vec<f32> {
    let mut v = Voice::new(SR);
    v.update_clipper_params_immediate(common::bypassed_clipper());
    v.set_tier_immediate(tier);
    override_brightness(&mut v, tier, bright);
    v.set_vca_env_mode(VcaEnvMode::Gate);
    v.set_cutoff(200.0 * 20.0f32.powf(0.4));
    v.set_reso(0.65);
    v.set_env_mod(0.7);
    v.note_on(57);
    v.set_gate(true, true, false, false);
    for _ in 0..48 {
        v.tick();
    }
    (0..n).map(|_| v.tick()).collect()
}

#[test]
#[ignore = "diagnostic: Low and Draft vs Best parity (tier_calibration sweep)"]
fn d_parity_vs_best() {
    println!("== D. Parity vs Best, envelope sweep (budgets: Low LSD<=17 corr>=0.975; Draft LSD<=42 corr>=0.94) ==");
    for (label, low_c, best_c) in COMBOS {
        let best = render_sweep(Tier::Best, best_c, 48_000);
        let low = render_sweep(Tier::Low, low_c, 48_000);
        let draft = render_sweep(Tier::Draft, None, 48_000);
        let (ll, lc) = spectral_match(&best, &low, SR);
        let (dl, dc) = spectral_match(&best, &draft, SR);
        println!("{label:<26} Low LSD {ll:6.2} corr {lc:.3} | Draft LSD {dl:6.2} corr {dc:.3}");
    }
}
