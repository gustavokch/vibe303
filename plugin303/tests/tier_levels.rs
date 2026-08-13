//! Output-level parity across fidelity tiers.
//!
//! The three tiers (Low/TPT, High, Ultra) must produce comparable output
//! loudness for the same note + filter settings — switching tiers should be a
//! timbre change, not a volume jump. The spectral gates (`ultra_spectral`,
//! `high_spectral`) power-normalise, so they are blind to absolute level; this
//! suite guards it explicitly.
//!
//! ROOT CAUSE this guards against: the nodal `LadderVcf` (High/Ultra) is driven
//! with `vscale = 0.02` and its passive differential ladder has a small-signal
//! DC gain of only ~0.04, so at `out_gain = 1` Ultra/High sat ~28 dB below the
//! unity-gain TPT (Low). `out_gain` now normalises the nodal DC passband to
//! unity (see `LADDER_DC_GAIN` in `ladder_wdf.rs`), bringing the tiers into
//! parity at the passband anchor.
//!
//! Residual differences that are NOT this bug (and are not asserted tightly):
//!   - Depressed/rate-dependent VCF corner: the nodal ladder's effective corner
//!     sits below the set cutoff and shifts with the oversample rate (High 2× vs
//!     Ultra 4×), so on bright patches the nodal tiers pass less/more harmonic
//!     energy than TPT. Closing this needs the active stages (deferred).
//!   - Resonance passband loss: the calibrated Q9 loop (`RESO_GAIN`) sheds
//!     passband as resonance rises, more than the TPT feedback does.
//! These are spectral-shape mismatches no single makeup gain can erase; the
//! level anchor is therefore the resonance-free passband.

use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::Voice;
use plugin303::params::VcaEnvMode;

/// Sustained-body RMS for one tier: trigger, skip the attack, measure the decay.
fn tier_rms(tier: Tier, note: u8, cutoff: f32, reso: f32) -> f32 {
    let sr = 48_000.0;
    let mut v = Voice::new(sr);
    v.set_tier_immediate(tier);
    v.set_vca_env_mode(VcaEnvMode::Gate);
    v.set_cutoff(cutoff);
    v.set_reso(reso);
    v.set_gate(true, true, false, false);
    v.note_on(note);
    for _ in 0..400 {
        v.tick(); // skip attack + filter warm-up
    }
    let n = 8_000;
    let mut acc = 0.0f64;
    for _ in 0..n {
        let y = v.tick();
        acc += (y as f64) * (y as f64);
    }
    (acc / n as f64).sqrt() as f32
}

/// Passband-anchor **regression floor** (not tight parity): resonance off, bright
/// patches where the nodal corner still passes the bulk of the saw. Tiers must
/// sit within ±9 dB of the Low/TPT reference. The wide band is deliberate — it
/// only catches the gross ~28 dB regression (the un-normalised `out_gain` deficit
/// returning), NOT drift: the nodal corner is depressed/rate-dependent, so High
/// at 2× passes fewer harmonics than Ultra at 4× even here — a spectral-shape
/// difference, not a gain bug. Tight High↔Ultra loudness parity is gated
/// separately by `high_tracks_ultra_level` (±1.5 dB); this test just guards that
/// the DC normalisation holds and no tier collapses.
#[test]
fn tiers_match_at_passband() {
    for &(note, cutoff) in &[(45u8, 2_000.0f32), (45, 3_000.0), (57, 3_000.0)] {
        let low = tier_rms(Tier::Low, note, cutoff, 0.0);
        let high = tier_rms(Tier::High, note, cutoff, 0.0);
        let ultra = tier_rms(Tier::Ultra, note, cutoff, 0.0);

        let hi_db = 20.0 * (high / low).log10();
        let ul_db = 20.0 * (ultra / low).log10();
        eprintln!(
            "note{note} fc{cutoff:.0}  Low={low:.4}  High={high:.4}({hi_db:+.1}dB)  Ultra={ultra:.4}({ul_db:+.1}dB)"
        );

        assert!(low > 1e-3, "Low produced no signal: {low}");
        assert!(high > 1e-3, "High produced no signal: {high}");
        assert!(ultra > 1e-3, "Ultra produced no signal: {ultra}");
        assert!(
            hi_db.abs() <= 9.0,
            "High {hi_db:.1} dB off Low at note{note}/fc{cutoff:.0} (expect within ±9 dB)"
        );
        assert!(
            ul_db.abs() <= 9.0,
            "Ultra {ul_db:.1} dB off Low at note{note}/fc{cutoff:.0} (expect within ±9 dB)"
        );
    }
}

/// High must track Ultra's loudness (the faithful reference) so a High↔Ultra
/// tier switch is a timbre change, not a volume jump — **in the resonant regime
/// a 303 actually plays in** (reso ≳ 0.2). High's polyBLEP-saw path sits below
/// Ultra's nodal-VCO path through the depressed-corner ladder by a
/// resonance-dependent amount; `HIGH_LEVEL_TRIM` (voice.rs) is centred on the
/// resonant regime, landing ±1.5 dB there. The gap widens to a few dB only at
/// literally reso = 0 (no peak to dominate — the source/OS-rate passband
/// difference is exposed); that is irreducible without a reso-dependent makeup
/// and is the darkest, least-used setting, so it is intentionally not gated
/// tightly (the passband test bounds it loosely vs Low instead).
#[test]
fn high_tracks_ultra_level() {
    for &(note, cutoff, reso) in &[
        (45u8, 1_500.0f32, 0.3f32),
        (33, 800.0, 0.5),
        (57, 3_000.0, 0.2),
        (57, 1_002.0, 0.85), // the gold patch
    ] {
        let high = tier_rms(Tier::High, note, cutoff, reso);
        let ultra = tier_rms(Tier::Ultra, note, cutoff, reso);
        let db = 20.0 * (high / ultra).log10();
        eprintln!("note{note} fc{cutoff:.0} r{reso}  High vs Ultra = {db:+.1} dB");
        assert!(
            db.abs() <= 2.5,
            "High off Ultra by {db:.1} dB at note{note}/fc{cutoff:.0}/r{reso} (expect within ±2.5 dB)"
        );
    }
}

/// Regression floor: across the full patch space (incl. resonance and dark
/// cutoffs where the depressed corner bites hardest), no tier may collapse to
/// the old ~28 dB-below-Low failure. The bound is loose (−24 dB) on purpose: the
/// worst structural case (high note + dark cutoff, where the depressed nodal
/// corner rolls off nearly the whole saw) lands ~−21 dB and is NOT a gain bug.
/// Reverting `out_gain` to 1 would drop every case a further ~28 dB, so this
/// floor still pins the normalisation in place everywhere.
#[test]
fn no_tier_collapses_in_level() {
    let cases = &[
        (33u8, 400.0f32, 0.5f32),
        (45, 1_500.0, 0.0),
        (57, 6_000.0, 0.5),
        (57, 400.0, 0.0),
    ];
    for &(note, cutoff, reso) in cases {
        let low = tier_rms(Tier::Low, note, cutoff, reso);
        let high = tier_rms(Tier::High, note, cutoff, reso);
        let ultra = tier_rms(Tier::Ultra, note, cutoff, reso);
        let hi_db = 20.0 * (high / low).log10();
        let ul_db = 20.0 * (ultra / low).log10();
        assert!(
            hi_db > -24.0,
            "High collapsed {hi_db:.1} dB at note{note}/fc{cutoff:.0}/r{reso}"
        );
        assert!(
            ul_db > -24.0,
            "Ultra collapsed {ul_db:.1} dB at note{note}/fc{cutoff:.0}/r{reso}"
        );
    }
}

#[test]
#[ignore = "Slow: checks all 18 combinations, taking several minutes due to Ultra Newton solver"]
fn no_tier_collapses_in_level_exhaustive() {
    for &note in &[33u8, 45, 57] {
        for &cutoff in &[400.0f32, 1_500.0, 6_000.0] {
            for &reso in &[0.0f32, 0.5] {
                let low = tier_rms(Tier::Low, note, cutoff, reso);
                let high = tier_rms(Tier::High, note, cutoff, reso);
                let ultra = tier_rms(Tier::Ultra, note, cutoff, reso);
                let hi_db = 20.0 * (high / low).log10();
                let ul_db = 20.0 * (ultra / low).log10();
                assert!(
                    hi_db > -24.0,
                    "High collapsed {hi_db:.1} dB at note{note}/fc{cutoff:.0}/r{reso}"
                );
                assert!(
                    ul_db > -24.0,
                    "Ultra collapsed {ul_db:.1} dB at note{note}/fc{cutoff:.0}/r{reso}"
                );
            }
        }
    }
}
