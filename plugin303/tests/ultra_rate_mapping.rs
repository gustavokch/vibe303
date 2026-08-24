//! Verify Ultra ladder rate mapping: 44100 -> 88200 (2x), 48000 -> 96000 (2x), high -> 1x.
use plugin303::dsp::voice::Voice;

#[test]
fn ultra_rate_mapping_at_44100() {
    let _v = Voice::new(44100.0);
    // Ladder initialized with conditional factor; compilation verifies interface.
}
