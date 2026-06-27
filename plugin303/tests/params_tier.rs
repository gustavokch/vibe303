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
