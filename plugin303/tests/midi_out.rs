use nih_plug::params::Param;
use plugin303::params::Plugin303Params;

#[test]
fn test_midi_and_bypass_params_exist_and_default() {
    let params = Plugin303Params::default();

    assert_eq!(params.midi_out.value(), false);
    assert_eq!(params.midi_out_channel.value(), 1);
    assert_eq!(params.synth_bypass.value(), false);

    // Verify channel range
    let ch_param = &params.midi_out_channel;
    assert_eq!(ch_param.default_plain_value(), 1);
}
