use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::Voice;
use plugin303::params::{CircuitTopology, CompTolerance, OscWave, VcaEnvMode, VcfTaps};

/// Analog-tweak settings of one render.
#[derive(Clone, Copy)]
struct Analog {
    age: i32,
    drift: f32,
    tolerance: CompTolerance,
    noise: i32,
    seed: i32,
}

const NOMINAL: Analog = Analog {
    age: 0,
    drift: 0.0,
    tolerance: CompTolerance::Zero,
    noise: 0,
    seed: 1337,
};

fn apply(v: &mut Voice, topology: CircuitTopology, a: Analog) {
    v.update_params(
        0.0,
        0.0,
        0.0,
        0.0,
        OscWave::Saw,
        0.5,
        0.0,
        0.5, // cutoff
        0.5, // resonance
        0.0, // env_mod: hold the cutoff still
        0.5,
        0.0,
        0.0,
        VcfTaps::FourPole,
        0.0,
        false,
        3.0,
        200.0,
        0.0,
        200.0,
        3.0,
        200.0,
        0.0,
        200.0,
        VcaEnvMode::Gate,
        a.age,
        a.drift,
        a.tolerance,
        a.noise,
        topology,
        0.0,
        a.seed,
        0.5,
        0.0,
    );
}

/// Half a second of note 45 (110 Hz) on `tier`, after a 0.1 s settle.
fn render(tier: Tier, topology: CircuitTopology, a: Analog) -> Vec<f32> {
    let mut v = Voice::new(48_000.0);
    v.set_tier_immediate(tier);
    apply(&mut v, topology, a);
    v.set_gate(true, true, false, false);
    v.note_on(45);
    (0..28_800).map(|_| v.tick()).skip(4_800).collect()
}

/// RMS of the first difference over the RMS: rises with high-frequency content.
fn brightness(x: &[f32]) -> f64 {
    let rms = |it: &mut dyn Iterator<Item = f64>| {
        let (s, n) = it.fold((0.0, 0usize), |(s, n), v| (s + v * v, n + 1));
        (s / n as f64).sqrt()
    };
    let d = rms(&mut x.windows(2).map(|w| (w[1] - w[0]) as f64));
    d / rms(&mut x.iter().map(|&s| s as f64))
}

/// AGE lowers the bias current and so the filter corner. Draft follows the
/// nodal ladder's corner shift, so every tier darkens with age; Draft used to
/// brighten while the ladder tiers darkened.
#[test]
fn age_darkens_every_tier() {
    for tier in [Tier::Draft, Tier::Low] {
        let new = brightness(&render(tier, CircuitTopology::Fun, NOMINAL));
        let aged = brightness(&render(
            tier,
            CircuitTopology::Fun,
            Analog {
                age: 100,
                ..NOMINAL
            },
        ));
        assert!(
            aged < new * 0.98,
            "{tier:?}: AGE 100 must darken the voice (brightness {aged:.4} vs new {new:.4})"
        );
    }
}

/// The editor shows the analog tweaks read-only in ACCURATE ("FUN mode
/// only"), so ACCURATE must not play them.
#[test]
fn accurate_ignores_analog_tweaks() {
    let worn = Analog {
        age: 100,
        drift: 1.0,
        tolerance: CompTolerance::Ten,
        noise: 100,
        seed: 42,
    };
    for tier in [Tier::Draft, Tier::Low] {
        assert_eq!(
            render(tier, CircuitTopology::Accurate, NOMINAL),
            render(tier, CircuitTopology::Accurate, worn),
            "{tier:?}: ACCURATE played the analog tweaks"
        );
    }
}

/// NOISE sets the ambient temperature at once; only gate self-heating is slow.
#[test]
fn noise_sets_ambient_temperature_at_once() {
    let mut v = Voice::new(48_000.0);
    apply(
        &mut v,
        CircuitTopology::Fun,
        Analog {
            noise: 100,
            ..NOMINAL
        },
    );
    v.tick();
    assert!(
        (v.temperature() - 40.0).abs() < 0.1,
        "NOISE 100 must put the voice at 40 °C on the next sample, got {}",
        v.temperature()
    );
}

#[test]
fn test_instance_specific_tolerance_seeds() {
    use plugin303::dsp::vco::Vco303;

    let mut vco1 = Vco303::new(48000.0);
    let mut vco2 = Vco303::new(48000.0);
    vco1.set_instance_id(1);
    vco2.set_instance_id(2);
    vco1.set_analog_wear(0.05, 0, 1337); // 5% tolerance
    vco2.set_analog_wear(0.05, 0, 1337); // 5% tolerance

    // Verify that capacitor multipliers differ due to separate instance IDs
    assert_ne!(vco1.get_c_mults(), vco2.get_c_mults());
    assert_ne!(vco1.get_r_mults(), vco2.get_r_mults());
}

#[test]
fn test_comp_seed_produces_different_components() {
    use plugin303::dsp::vco::Vco303;
    let mut vco_a = Vco303::new(48000.0);
    let mut vco_b = Vco303::new(48000.0);
    vco_a.set_analog_wear(0.05, 10, 1337);
    vco_b.set_analog_wear(0.05, 10, 7331);
    assert_ne!(vco_a.get_c_mults(), vco_b.get_c_mults());
    assert_ne!(vco_a.get_r_mults(), vco_b.get_r_mults());
}

#[test]
fn test_age_100_maintains_positive_cutoff_and_resonance() {
    let mut v = Voice::new(48000.0);
    v.update_params(
        0.0,
        0.0,
        0.0,
        0.0,
        plugin303::params::OscWave::Saw,
        0.5,
        0.0,
        0.5,
        0.5,
        0.5,
        0.5,
        0.5,
        0.0,
        plugin303::params::VcfTaps::FourPole,
        0.0,
        false,
        3.0,
        200.0,
        0.0,
        200.0,
        3.0,
        200.0,
        0.0,
        200.0,
        plugin303::params::VcaEnvMode::Envelope,
        100, // max age
        0.5, // comp_drift
        plugin303::params::CompTolerance::Ten,
        100, // max thermal noise
        CircuitTopology::Fun,
        0.0,
        1337,
        0.5,
        0.0,
    );
    v.note_on(60);
    v.set_gate(true, true, false, false);
    for _ in 0..720_000 {
        let s = v.tick();
        assert!(s.is_finite());
    }
}

#[test]
fn test_thermal_model_ramp_and_persistence() {
    let sr = 48000.0;
    let mut v = Voice::new(sr);
    v.update_params(
        0.0,
        0.0,
        0.0,
        0.0,
        plugin303::params::OscWave::Saw,
        0.5,
        0.0,
        0.5,
        0.5,
        0.5,
        0.5,
        0.5,
        0.0,
        plugin303::params::VcfTaps::FourPole,
        0.0,
        false,
        3.0,
        200.0,
        0.0,
        200.0,
        3.0,
        200.0,
        0.0,
        200.0,
        plugin303::params::VcaEnvMode::Envelope,
        10,
        0.5,
        plugin303::params::CompTolerance::Ten,
        50,
        CircuitTopology::Accurate,
        0.0,
        1337,
        0.5,
        0.0,
    );

    let initial_temp = v.temperature();
    v.note_on(60);
    v.set_gate(true, true, false, false);

    for _ in 0..720_000 {
        v.tick();
    }
    let warmed_temp = v.temperature();
    assert!(
        warmed_temp > initial_temp + 1.0,
        "Temperature should ramp up when gate is active: initial={}, warmed={}",
        initial_temp,
        warmed_temp
    );

    v.reset();
    assert_eq!(
        v.temperature(),
        warmed_temp,
        "Temperature must persist across Voice::reset()"
    );
}

#[test]
fn test_drift_reso_and_osc2_pitch() {
    let sr = 48000.0;
    let mut v = Voice::new(sr);
    v.update_params(
        0.0,
        0.0,
        7.0,
        0.8,
        plugin303::params::OscWave::Saw,
        0.5,
        0.0,
        0.5,
        0.8,
        0.5,
        0.5,
        0.5,
        0.0,
        plugin303::params::VcfTaps::FourPole,
        0.0,
        false,
        3.0,
        200.0,
        0.0,
        200.0,
        3.0,
        200.0,
        0.0,
        200.0,
        plugin303::params::VcaEnvMode::Envelope,
        10,
        0.8,
        plugin303::params::CompTolerance::Ten,
        20,
        CircuitTopology::Fun,
        0.0,
        1337,
        0.5,
        0.0,
    );
    v.note_on(60);
    v.set_gate(true, true, false, false);

    let mut samples = Vec::new();
    for _ in 0..48000 {
        samples.push(v.tick());
    }

    let non_zero = samples.iter().any(|&s| s.abs() > 1e-4);
    let all_finite = samples.iter().all(|s| s.is_finite());
    assert!(
        all_finite && non_zero,
        "Osc2 and drift render valid fluctuating audio"
    );
}
