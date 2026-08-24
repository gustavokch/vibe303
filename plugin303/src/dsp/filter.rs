//! Tier-swappable filter interface. TPT (Low) impls it now; WDF (High/Ultra) in F3/F4.

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

/// Fidelity tier. Low = TPT; High = WDF-lite (F4); Ultra = WDF (F3).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Enum)]
pub enum Tier {
    #[default]
    #[name = "Low (TPT)"]
    Low,
    #[name = "High (WDF-lite)"]
    High,
    #[name = "Ultra (WDF)"]
    Ultra,
    #[name = "Ultra 2x (WDF)"]
    Ultra2x,
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
}
