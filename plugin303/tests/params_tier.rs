//! The fidelity-tier param exists, defaults to Low, and round-trips through nih-plug's
//! `Enum`/`EnumParam` (id "tier").

use nih_plug::prelude::*;
use plugin303::dsp::filter::Tier;
use plugin303::params::Plugin303Params;

#[test]
fn tier_param_defaults_low() {
    let p = Plugin303Params::default();
    assert_eq!(p.tier.value(), Tier::Low);
}

#[test]
fn tier_enum_has_three_variants() {
    assert_eq!(Tier::variants().len(), 3);
}

#[test]
fn test_analog_character_parameter_ranges() {
    let params = Plugin303Params::default();
    assert_eq!(params.comp_age.value(), 0);
    assert_eq!(params.comp_drift.value(), 0.0);
    assert_eq!(params.tolerance.value(), plugin303::params::CompTolerance::Zero);
    assert_eq!(params.thermal_noise.value(), 0);
    
    // Check formatting of thermal_noise
    let format_zero = params.thermal_noise.to_string();
    assert_eq!(format_zero, "Off");
}

#[test]
fn test_sequencer_parameters_declared() {
    let params = Plugin303Params::default();
    assert_eq!(params.seq_mode.value(), plugin303::params::SequencerMode::DirectGrid);
    assert_eq!(params.seq_timebase.value(), plugin303::params::Timebase::Straight);
    assert!(!params.seq_jitter.value());
}

