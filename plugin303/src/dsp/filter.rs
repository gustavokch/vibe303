//! Filter interface and the quality-tier enum.

use nih_plug::prelude::Enum;

/// A 303 filter core: cutoff in Hz, resonance in [0,1], per-sample `tick`.
pub trait Filter303 {
    fn set_cutoff(&mut self, hz: f32);
    fn set_reso(&mut self, r: f32);
    fn tick(&mut self, x: f32) -> f32;
    fn reset(&mut self);
    /// Sets the number of poles for tap selection: 2, 3, or 4 poles.
    fn set_poles(&mut self, _n: usize) {}
}

/// Quality tier, ordered by fidelity and CPU cost.
///
/// The variant order is the saved-state index of the `tier` param: never
/// reorder or insert in the middle.
///
/// | Tier  | Oscillator            | Filter             | Oversampling |
/// |-------|-----------------------|--------------------|--------------|
/// | Draft | polyBLEP saw          | TPT ladder         | none         |
/// | Low   | polyBLEP saw          | nodal diode ladder | 2x           |
/// | High  | nodal circuit VCO     | nodal diode ladder | 2x           |
/// | Best  | nodal circuit VCO     | nodal diode ladder | 4x           |
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Enum)]
pub enum Tier {
    #[default]
    #[name = "Draft"]
    Draft,
    #[name = "Low"]
    Low,
    #[name = "High"]
    High,
    #[name = "Best"]
    Best,
}

impl Tier {
    /// Parses a CLI tier name (`draft`, `low`, `high`, `best`), ASCII
    /// case-insensitive. Anything else, including retired tier names and
    /// bare indices, is `None`.
    pub fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "draft" => Some(Self::Draft),
            "low" => Some(Self::Low),
            "high" => Some(Self::High),
            "best" => Some(Self::Best),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tpt_ladder_runs_through_trait_object() {
        use crate::dsp::ladder::TptLadder;
        let mut f = TptLadder::new(48_000.0);
        let dynf: &mut dyn Filter303 = &mut f;
        dynf.set_cutoff(1_000.0);
        dynf.set_reso(0.5);
        let mut acc = 0.0f32;
        for i in 0..2_000 {
            acc += dynf.tick(((i as f32) * 0.01).sin());
        }
        assert!(acc.is_finite());
    }

    #[test]
    fn parse_is_case_insensitive_and_rejects_retired_names() {
        assert_eq!(Tier::parse("draft"), Some(Tier::Draft));
        assert_eq!(Tier::parse("Low"), Some(Tier::Low));
        assert_eq!(Tier::parse("HIGH"), Some(Tier::High));
        assert_eq!(Tier::parse("best"), Some(Tier::Best));
        // Retired names and bare indices must fail, not map to a different tier.
        // `ultra` is the one permitted `ultra` in code: it is the retired-name case.
        for retired in ["ultra", "ultra2x", "0", "2", ""] {
            assert_eq!(Tier::parse(retired), None, "{retired:?}");
        }
    }

    #[test]
    fn saved_state_index_follows_fidelity_order() {
        let indices = [Tier::Draft, Tier::Low, Tier::High, Tier::Best].map(|t| t.to_index());
        assert_eq!(indices, [0, 1, 2, 3]);
    }
}
