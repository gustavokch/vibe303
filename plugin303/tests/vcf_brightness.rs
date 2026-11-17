//! The FUN-only BRIGHT dial (`vcf_brightness`) moves every tier's filter corner,
//! and ACCURATE ignores it.

mod common;

use plugin303::dsp::filter::Tier;
use plugin303::dsp::vco::Vco303;
use plugin303::dsp::voice::Voice;
use plugin303::params::CircuitTopology;

const SR: f32 = 48_000.0;

fn render(tier: Tier, topology: CircuitTopology, dial: f32) -> Vec<f32> {
    Vco303::warm_cache_nominal(SR, true);
    let mut v = Voice::new(SR);
    v.update_clipper_params_immediate(common::bypassed_clipper());
    v.set_tier_immediate(tier);
    common::apply_patch(&mut v, 0, 0.0, topology, dial);
    v.set_advanced_env(true);
    v.set_cutoff(800.0);
    v.set_reso(0.5);
    v.note_on(45);
    (0..12_000).map(|_| v.tick()).skip(2_400).collect()
}

/// Energy of the first difference over the signal energy: rises with the
/// spectral centroid.
fn brightness(x: &[f32]) -> f64 {
    let e: f64 = x.iter().map(|&s| f64::from(s).powi(2)).sum();
    let d: f64 = x.windows(2).map(|w| f64::from(w[1] - w[0]).powi(2)).sum();
    d / e
}

#[test]
fn fun_dial_darkens_and_brightens_every_tier() {
    for tier in [Tier::Draft, Tier::Low, Tier::High, Tier::Best] {
        let b: Vec<f64> = [0.5, 1.0, 1.5]
            .map(|dial| brightness(&render(tier, CircuitTopology::Fun, dial)))
            .into();
        assert!(
            b[0] < b[1] && b[1] < b[2],
            "{tier:?}: brightness at dial 50/100/150% = {b:?}, expected increasing"
        );
    }
}

#[test]
fn accurate_ignores_the_dial() {
    for tier in [Tier::Draft, Tier::Low, Tier::High, Tier::Best] {
        let nominal = render(tier, CircuitTopology::Accurate, 1.0);
        for dial in [0.5, 1.5] {
            assert_eq!(
                render(tier, CircuitTopology::Accurate, dial),
                nominal,
                "{tier:?}: ACCURATE output changed with dial {dial}"
            );
        }
    }
}
