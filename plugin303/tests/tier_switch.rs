//! Click-free tier switching guard (F5.1).
//!
//! Switching fidelity tiers swaps the entire signal path (TPT ↔ nodal ladder ↔
//! circuit VCO). The paths differ in timbre and the incoming one starts cold, so
//! a hard switch pops. `Voice::set_tier` runs a short equal-power crossfade; this
//! gate asserts the switch introduces no sample discontinuity beyond the signal's
//! own steady-state slew.

use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::Voice;

fn max_step(s: &[f32]) -> f32 {
    s.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max)
}

/// Steady-state max per-sample step for a tier, measured over the same env phase
/// window the switch test uses (so the AD decay level matches).
fn steady_max_step(tier: Tier, sr: f32, skip: usize, take: usize) -> f32 {
    let mut v = Voice::new(sr);
    v.set_tier_immediate(tier);
    v.set_cutoff(1_000.0);
    v.set_reso(0.5);
    v.note_on(45);
    for _ in 0..skip {
        v.tick();
    }
    let buf: Vec<f32> = (0..take).map(|_| v.tick()).collect();
    max_step(&buf)
}

fn switch_test(from: Tier, to: Tier) {
    let sr = 48_000.0;
    let switch_at = 2_000usize;
    let win = 800usize; // ≈10 ms fade + margin, in samples around the switch

    // Reference slew: the louder of the two tiers, at the switch's env phase.
    let ref_step = steady_max_step(from, sr, switch_at - win / 2, win)
        .max(steady_max_step(to, sr, switch_at - win / 2, win));

    // Switch render: play `from`, flip to `to` at `switch_at`.
    let mut v = Voice::new(sr);
    v.set_tier_immediate(from);
    v.set_cutoff(1_000.0);
    v.set_reso(0.5);
    v.note_on(45);
    let mut buf = Vec::with_capacity(switch_at + win);
    for i in 0..switch_at + win {
        if i == switch_at {
            v.set_tier(to);
        }
        buf.push(v.tick());
    }
    let switch_step = max_step(&buf[switch_at - win / 2..switch_at + win / 2]);

    // A hard switch would spike well past the steady slew; the crossfade must
    // keep the window's max step within a small factor of normal playback.
    assert!(
        switch_step <= 2.5 * ref_step + 1e-6,
        "{from:?}->{to:?} click: switch step {switch_step:.4} vs steady {ref_step:.4}"
    );
    assert!(buf.iter().all(|y| y.is_finite()), "{from:?}->{to:?} produced non-finite");
}

#[test]
fn low_to_high_is_click_free() {
    switch_test(Tier::Low, Tier::High);
}

#[test]
fn high_to_ultra_is_click_free() {
    switch_test(Tier::High, Tier::Ultra);
}

#[test]
fn low_to_ultra_is_click_free() {
    switch_test(Tier::Low, Tier::Ultra);
}

#[test]
fn ultra_to_low_is_click_free() {
    switch_test(Tier::Ultra, Tier::Low);
}

#[test]
#[ignore = "diagnostic: print switch-window vs steady max step per pair"]
fn switch_step_report() {
    let sr = 48_000.0;
    let (switch_at, win) = (2_000usize, 800usize);
    for (from, to) in [
        (Tier::Low, Tier::High),
        (Tier::High, Tier::Ultra),
        (Tier::Low, Tier::Ultra),
        (Tier::Ultra, Tier::Low),
    ] {
        let ref_step = steady_max_step(from, sr, switch_at - win / 2, win)
            .max(steady_max_step(to, sr, switch_at - win / 2, win));
        let mut v = Voice::new(sr);
        v.set_tier_immediate(from);
        v.set_cutoff(1_000.0);
        v.set_reso(0.5);
        v.note_on(45);
        let mut buf = Vec::new();
        for i in 0..switch_at + win {
            if i == switch_at {
                v.set_tier(to);
            }
            buf.push(v.tick());
        }
        let switch_step = max_step(&buf[switch_at - win / 2..switch_at + win / 2]);
        println!(
            "{from:?}->{to:?}: switch {switch_step:.4}  steady {ref_step:.4}  ratio {:.2}",
            switch_step / ref_step.max(1e-9)
        );
    }
}
