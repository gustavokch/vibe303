use crate::dsp::filter::Tier;
use nih_plug::prelude::*;

#[derive(Params)]
pub struct Plugin303Params {
    #[id = "tier"]
    pub tier: EnumParam<Tier>,
}

impl Default for Plugin303Params {
    fn default() -> Self {
        Self {
            tier: EnumParam::new("Tier", Tier::Low),
        }
    }
}
