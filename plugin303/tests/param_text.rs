use nih_plug::prelude::{FloatParam, Param};
use plugin303::params::Plugin303Params;

fn text(param: &FloatParam, plain: f32) -> String {
    param.normalized_value_to_string(param.preview_normalized(plain), true)
}

/// The host and the knob show one string per value, and it says what the voice
/// does with that value.
#[test]
fn knob_params_print_what_the_voice_does() {
    let p = Plugin303Params::default();
    // DECAY uses the voice's decay table: 100 ms .. 2.1 s.
    assert_eq!(text(&p.decay, 0.0), "100 ms");
    assert_eq!(text(&p.decay, 0.5), "400 ms");
    assert_eq!(text(&p.decay, 1.0), "2.1 s");
    // VCF drive is dB, not a percentage.
    assert_eq!(text(&p.vcf_drive, 12.0), "12.0 dB");
    // Bipolar values are signed, +0.0 on the detent.
    assert_eq!(text(&p.tuning, 25.0), "+25.0 ct");
    assert_eq!(text(&p.tuning, 0.0), "+0.0 ct");
    assert_eq!(text(&p.osc1_pitch, -12.0), "-12.0 st");
    assert_eq!(text(&p.volume, -6.0), "-6.0 dB");
    // Unipolar amounts are percentages of their range.
    assert_eq!(text(&p.resonance, 0.5), "50%");
    assert_eq!(text(&p.osc1_pw, 0.5), "50%");
    assert_eq!(text(&p.clipper_drive, 0.5), "50%");
}
