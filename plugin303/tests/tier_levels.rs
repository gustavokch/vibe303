//! Output-level parity across fidelity tiers.
//!
//! The three tiers (Draft/TPT, Low, Best) must produce comparable output
//! loudness for the same note + filter settings — switching tiers should be a
//! timbre change, not a volume jump. The spectral gates (`best_spectral`,
//! `low_spectral`) power-normalise, so they are blind to absolute level; this
//! suite guards it explicitly.
//!
//! ROOT CAUSE this guards against: the nodal `LadderVcf` (Low/Best) is driven
//! with `vscale = 0.02` and its passive differential ladder has a small-signal
//! DC gain of only ~0.04, so at `out_gain = 1` Best/Low sat ~28 dB below the
//! unity-gain TPT (Draft). `out_gain` now normalises the nodal DC passband to
//! unity (see `LADDER_DC_GAIN` in `ladder_wdf.rs`), bringing the tiers into
//! parity at the passband anchor.
//!
//! Residual differences that are NOT this bug (and are not asserted tightly):
//!   - Depressed/rate-dependent VCF corner: the nodal ladder's effective corner
//!     sits below the set cutoff and shifts with the oversample rate (Low and
//!     Best both 4×), so on bright patches the nodal tiers pass less/more harmonic
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
/// sit within ±9 dB of the Draft/TPT reference. The wide band is deliberate — it
/// only catches the gross ~28 dB regression (the un-normalised `out_gain` deficit
/// returning), NOT drift: the nodal corner is depressed/rate-dependent, so Low
/// at 2× passes fewer harmonics than Best at 4× even here — a spectral-shape
/// difference, not a gain bug. Tight Low↔Best loudness parity is gated
/// separately by `low_tracks_best_level` (±1.5 dB); this test just guards that
/// the DC normalisation holds and no tier collapses.
#[test]
fn tiers_match_at_passband() {
    for &(note, cutoff) in &[(45u8, 2_000.0f32), (45, 3_000.0), (57, 3_000.0)] {
        let draft = tier_rms(Tier::Draft, note, cutoff, 0.0);
        let low = tier_rms(Tier::Low, note, cutoff, 0.0);
        let best = tier_rms(Tier::Best, note, cutoff, 0.0);

        let low_db = 20.0 * (low / draft).log10();
        let best_db = 20.0 * (best / draft).log10();
        eprintln!(
            "note{note} fc{cutoff:.0}  Draft={draft:.4}  Low={low:.4}({low_db:+.1}dB)  Best={best:.4}({best_db:+.1}dB)"
        );

        assert!(draft > 1e-3, "Draft produced no signal: {draft}");
        assert!(low > 1e-3, "Low produced no signal: {low}");
        assert!(best > 1e-3, "Best produced no signal: {best}");
        assert!(
            low_db.abs() <= 9.0,
            "Low {low_db:.1} dB off Draft at note{note}/fc{cutoff:.0} (expect within ±9 dB)"
        );
        assert!(
            best_db.abs() <= 9.0,
            "Best {best_db:.1} dB off Draft at note{note}/fc{cutoff:.0} (expect within ±9 dB)"
        );
    }
}

/// Low must track Best's loudness (the faithful reference) so a Low↔Best
/// tier switch is a timbre change, not a volume jump — **in the resonant regime
/// a 303 actually plays in** (reso ≳ 0.2). Low's polyBLEP-saw path sits below
/// Best's nodal-VCO path through the depressed-corner ladder by a
/// resonance-dependent amount; `LOW_LEVEL_TRIM` (voice.rs) is a reso-dependent
/// makeup table that matches the median level per resonance knot. The residual
/// spread is the filter-shape difference (largest at reso = 0 with the cutoff
/// near the fundamental), so it is not gated tightly at reso = 0 (the passband
/// test bounds it loosely vs Draft instead).
#[test]
fn low_tracks_best_level() {
    for &(note, cutoff, reso) in &[
        (45u8, 1_500.0f32, 0.3f32),
        (33, 800.0, 0.5),
        (57, 3_000.0, 0.2),
        (57, 1_002.0, 0.85), // the gold patch
    ] {
        let low = tier_rms(Tier::Low, note, cutoff, reso);
        let best = tier_rms(Tier::Best, note, cutoff, reso);
        let db = 20.0 * (low / best).log10();
        eprintln!("note{note} fc{cutoff:.0} r{reso}  Low vs Best = {db:+.1} dB");
        assert!(
            db.abs() <= 2.5,
            "Low off Best by {db:.1} dB at note{note}/fc{cutoff:.0}/r{reso} (expect within ±2.5 dB)"
        );
    }
}

/// Regression floor: across the full patch space (incl. resonance and dark
/// cutoffs where the depressed corner bites hardest), no tier may collapse to
/// the old ~28 dB-below-Draft failure. The bound is loose (−24 dB) on purpose: the
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
        let draft = tier_rms(Tier::Draft, note, cutoff, reso);
        let low = tier_rms(Tier::Low, note, cutoff, reso);
        let best = tier_rms(Tier::Best, note, cutoff, reso);
        let low_db = 20.0 * (low / draft).log10();
        let best_db = 20.0 * (best / draft).log10();
        assert!(
            low_db > -24.0,
            "Low collapsed {low_db:.1} dB at note{note}/fc{cutoff:.0}/r{reso}"
        );
        assert!(
            best_db > -24.0,
            "Best collapsed {best_db:.1} dB at note{note}/fc{cutoff:.0}/r{reso}"
        );
    }
}

#[test]
#[ignore = "Slow: checks all 18 combinations, taking several minutes due to Best Newton solver"]
fn no_tier_collapses_in_level_exhaustive() {
    for &note in &[33u8, 45, 57] {
        for &cutoff in &[400.0f32, 1_500.0, 6_000.0] {
            for &reso in &[0.0f32, 0.5] {
                let draft = tier_rms(Tier::Draft, note, cutoff, reso);
                let low = tier_rms(Tier::Low, note, cutoff, reso);
                let best = tier_rms(Tier::Best, note, cutoff, reso);
                let low_db = 20.0 * (low / draft).log10();
                let best_db = 20.0 * (best / draft).log10();
                assert!(
                    low_db > -24.0,
                    "Low collapsed {low_db:.1} dB at note{note}/fc{cutoff:.0}/r{reso}"
                );
                assert!(
                    best_db > -24.0,
                    "Best collapsed {best_db:.1} dB at note{note}/fc{cutoff:.0}/r{reso}"
                );
            }
        }
    }
}
