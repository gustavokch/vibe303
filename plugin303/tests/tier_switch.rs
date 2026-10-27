//! Click-free tier switching guard (F5.1).
//!
//! Switching fidelity tiers swaps the entire signal path (TPT ↔ nodal ladder ↔
//! circuit VCO). The paths differ in timbre and the incoming one starts cold, so
//! a hard switch pops. `Voice::set_tier` runs a short equal-power crossfade; this
//! gate asserts the switch introduces no sample discontinuity beyond the signal's
//! own steady-state slew.

use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::Voice;
use plugin303::params::{CircuitTopology, CompTolerance, OscWave, VcaEnvMode, VcfTaps};

fn max_step(s: &[f32]) -> f32 {
    s.windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0, f32::max)
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
    let ref_step = steady_max_step(from, sr, switch_at - win / 2, win).max(steady_max_step(
        to,
        sr,
        switch_at - win / 2,
        win,
    ));

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
    assert!(
        buf.iter().all(|y| y.is_finite()),
        "{from:?}->{to:?} produced non-finite"
    );
}

#[test]
fn draft_to_low_is_click_free() {
    switch_test(Tier::Draft, Tier::Low);
}

#[test]
fn low_to_best_is_click_free() {
    switch_test(Tier::Low, Tier::Best);
}

#[test]
fn draft_to_best_is_click_free() {
    switch_test(Tier::Draft, Tier::Best);
}

#[test]
fn best_to_draft_is_click_free() {
    switch_test(Tier::Best, Tier::Draft);
}

#[test]
fn low_to_high_is_click_free() {
    switch_test(Tier::Low, Tier::High);
}

#[test]
fn high_to_low_is_click_free() {
    switch_test(Tier::High, Tier::Low);
}

#[test]
fn high_to_best_is_click_free() {
    switch_test(Tier::High, Tier::Best);
}

#[test]
fn best_to_high_is_click_free() {
    switch_test(Tier::Best, Tier::High);
}

/// Fun mode with the sub oscillator at full level, so a fade that ticks a
/// shared sub oscillator twice per sample shows up as a sub phase offset.
fn fun_mode_with_sub(v: &mut Voice) {
    v.update_params(
        0.0,
        0.0,
        0.0,
        0.0,
        OscWave::Saw,
        0.5,
        1.0, // sub_vol
        0.6,
        0.3,
        0.0,
        0.5,
        0.5,
        0.0,
        VcfTaps::FourPole,
        0.0,
        false,
        3.0,
        200.0,
        0.0,
        200.0,
        3.0,
        200.0,
        0.0,
        200.0,
        VcaEnvMode::Gate,
        0,
        0.0,
        CompTolerance::Zero,
        0,
        CircuitTopology::Fun,
        0.0,
        0,
        0.5,
        0.0,
    );
}

/// Renders `from`, switches to `to` at `switch_at` (crossfaded or immediate)
/// and returns the output.
fn render_switch(from: Tier, to: Tier, fun_sub: bool, fade: bool) -> Vec<f32> {
    let switch_at = 2_000;
    let mut v = Voice::new(48_000.0);
    v.set_tier_immediate(from);
    if fun_sub {
        fun_mode_with_sub(&mut v);
        v.set_gate(true, true, false, false);
    } else {
        v.set_cutoff(3_000.0);
        v.set_reso(0.0);
    }
    v.note_on(57);
    (0..6_000)
        .map(|i| {
            if i == switch_at {
                if fade {
                    v.set_tier(to);
                } else {
                    v.set_tier_immediate(to);
                }
            }
            v.tick()
        })
        .collect()
}

/// Tiers that share an oscillator must advance it once per output sample,
/// also during a crossfade. Otherwise the shared oscillator runs at double
/// rate for the fade (an audible pitch jump) and stays out of phase after it.
/// So once the 10 ms fade has ended, the output must match an immediate switch
/// made at the same sample.
#[test]
fn fade_keeps_shared_oscillators_in_phase() {
    let cases = [
        (Tier::High, Tier::Best, false),
        (Tier::Best, Tier::High, false),
        (Tier::Low, Tier::High, true),
        (Tier::High, Tier::Low, true),
        (Tier::Low, Tier::Best, true),
    ];
    let mut drifted = Vec::new();
    for (from, to, fun_sub) in cases {
        let faded = render_switch(from, to, fun_sub, true);
        let direct = render_switch(from, to, fun_sub, false);
        let after = 4_000..6_000;
        let err: f32 = faded[after.clone()]
            .iter()
            .zip(&direct[after.clone()])
            .map(|(a, b)| (a - b).powi(2))
            .sum();
        let power: f32 = direct[after].iter().map(|y| y * y).sum();
        assert!(power > 1e-3, "{from:?}->{to:?}: silent reference");
        if err / power >= 1e-3 {
            drifted.push(format!("{from:?}->{to:?} (sub: {fun_sub}): {:.4}", err / power));
        }
    }
    assert!(
        drifted.is_empty(),
        "post-fade output drifted from an immediate switch (relative error): {drifted:?}"
    );
}

#[test]
#[ignore = "diagnostic: print switch-window vs steady max step per pair"]
fn switch_step_report() {
    let sr = 48_000.0;
    let (switch_at, win) = (2_000usize, 800usize);
    for (from, to) in [
        (Tier::Draft, Tier::Low),
        (Tier::Low, Tier::Best),
        (Tier::Draft, Tier::Best),
        (Tier::Best, Tier::Draft),
    ] {
        let ref_step = steady_max_step(from, sr, switch_at - win / 2, win).max(steady_max_step(
            to,
            sr,
            switch_at - win / 2,
            win,
        ));
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
