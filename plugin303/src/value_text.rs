//! Display text for parameter values. The host's `value_to_string` and the
//! knob readouts both come from here, so they cannot disagree with each other
//! or with the voice's mappings in `dsp::voice`.

use crate::dsp::voice;
use crate::params::CircuitTopology;

/// One decimal with a sign, `+0.0` on the centre detent. The unit comes from
/// the parameter's `with_unit`.
pub fn signed(v: f32) -> String {
    if v.abs() < 0.05 {
        "+0.0".to_string()
    } else {
        format!("{v:+.1}")
    }
}

/// Milliseconds; seconds from 999.5 ms, where `{:.0}` would round to "1000 ms";
/// one decimal below 10 ms.
pub fn ms_or_s(ms: f32) -> String {
    if ms >= 999.5 {
        format!("{:.1} s", ms / 1000.0)
    } else if ms < 9.95 {
        format!("{ms:.1} ms")
    } else {
        format!("{ms:.0} ms")
    }
}

/// Hz below 1 kHz, kHz with one decimal above.
pub fn hz(hz: f32) -> String {
    if hz >= 999.5 {
        format!("{:.1} kHz", hz / 1000.0)
    } else {
        format!("{hz:.0} Hz")
    }
}

/// The voice reads ADV ENV only in FUN (`voice.rs` forces it off in ACCURATE).
fn adv_env_live(topology: CircuitTopology, advanced_env: bool) -> bool {
    topology == CircuitTopology::Fun && advanced_env
}

/// CUTOFF readout: the base cutoff the voice uses in the active mode.
pub fn cutoff(k: f32, topology: CircuitTopology, advanced_env: bool) -> String {
    let base = if adv_env_live(topology, advanced_env) {
        voice::adv_base_cutoff_hz(k)
    } else {
        voice::classic_base_cutoff_hz(k as f64) as f32
    };
    hz(base)
}

/// GLIDE readout: the pitch-glide time constant the voice uses in the active mode.
pub fn glide(k: f32, topology: CircuitTopology, advanced_env: bool) -> String {
    let ms = if adv_env_live(topology, advanced_env) {
        voice::adv_glide_ms(k)
    } else {
        voice::calculate_classic_slide_tau_ms(topology, k) as f32
    };
    ms_or_s(ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::CircuitTopology::{Accurate, Fun};

    #[test]
    fn ms_switches_to_seconds_at_the_rounding_boundary() {
        // `{:.0}` would print 999.6 ms as "1000 ms".
        assert_eq!(ms_or_s(999.4), "999 ms");
        assert_eq!(ms_or_s(999.6), "1.0 s");
        assert_eq!(ms_or_s(5.51), "5.5 ms");
        assert_eq!(ms_or_s(2100.0), "2.1 s");
    }

    #[test]
    fn signed_reads_plus_zero_on_the_detent() {
        assert_eq!(signed(0.04), "+0.0");
        assert_eq!(signed(-0.04), "+0.0");
        assert_eq!(signed(-3.5), "-3.5");
        assert_eq!(signed(25.0), "+25.0");
    }

    #[test]
    fn cutoff_follows_the_mode_the_voice_uses() {
        assert_eq!(cutoff(0.0, Fun, false), "314 Hz");
        assert_eq!(cutoff(0.5, Fun, false), "867 Hz");
        assert_eq!(cutoff(1.0, Fun, false), "2.4 kHz");
        assert_eq!(cutoff(0.0, Fun, true), "200 Hz");
        assert_eq!(cutoff(1.0, Fun, true), "4.0 kHz");
        // ACCURATE forces ADV ENV off in the voice.
        assert_eq!(cutoff(1.0, Accurate, true), "2.4 kHz");
    }

    #[test]
    fn glide_follows_the_mode_the_voice_uses() {
        assert_eq!(glide(0.4, Fun, true), "170 ms");
        assert_eq!(glide(0.4, Fun, false), "5.5 ms");
        assert_eq!(glide(1.0, Fun, false), "110 ms");
    }
}
