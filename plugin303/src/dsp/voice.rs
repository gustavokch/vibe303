//! Monophonic voice: osc -> ladder VCF -> VCA(env). M1 wiring only.
//! Accent, glide, sub/detune and the souped levers come in M4/M6.

use super::env::AdEnv;
use super::filter::{Filter303, Tier};
use super::ladder::TptLadder;
use super::ladder_wdf::LadderVcf;
use super::osc::Saw;
use super::oversample::{Os2x, Os4x};
use super::vco::Vco303;

/// Equal-tempered MIDI note -> frequency (A4 = note 69 = 440 Hz).
pub fn midi_to_freq(note: u8) -> f32 {
    440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0)
}

/// Ultra-tier oversampling factor (nodal ladder runs at `OS_FACTOR × sr`).
pub const ULTRA_OS_FACTOR: f32 = 4.0;

/// High-tier oversampling factor (polyBLEP saw → nodal ladder at `2 × sr`).
pub const HIGH_OS_FACTOR: f32 = 2.0;

/// Level trim to bring the DC-blocked VCO saw to ~unit (polyBLEP-saw) amplitude.
pub const VCO_GAIN: f32 = 2.0;


/// Per-tier VCF brightness (cutoff→bias scale on the shared `LadderVcf`). With
/// the active Q9 resonance (F6.3) the spectral-LSD optimum vs the whole-voice
/// gold sits *below* 1.0 — the resonance peak otherwise over-brightens. Tuned
/// per tier (different OS rate ⇒ different optimum): Ultra (4×) at 0.70 minimises
/// LSD to 3.87 dB; High (2×) at 0.50 keeps corr ≥ 0.95 while beating its prior
/// LSD. See `brightness_sweep`/`high_brightness_sweep` diagnostics (F6.5).
const ULTRA_BRIGHT: f64 = 0.70;
const HIGH_BRIGHT: f64 = 0.50;

/// High-tier output level trim so High tracks Ultra's loudness on a tier switch.
/// High's polyBLEP-saw path sits below Ultra's nodal-VCO path through the
/// (faithfully depressed-corner) ladder by a *resonance-dependent* amount:
/// ~3 dB in the resonant regime a 303 actually plays in (reso ≳ 0.2), shrinking
/// to ~0 dB near reso 0.85 and widening to ~6 dB only at reso 0 (no peak to
/// dominate). A constant +3.2 dB makeup centres High on Ultra to within ±0.5 dB
/// across the resonant regime; the reso-0 residual is left (a constant trim
/// cannot flatten a reso-dependent gap, and reso 0 is the darkest/least-used
/// setting). The spectral gates power-normalise, so this trim is invisible to
/// them (no fidelity-gate impact); it only equalises perceived loudness.
const HIGH_LEVEL_TRIM: f32 = 1.45; // +3.2 dB

/// Tier-switch crossfade length in seconds (equal-power settle-then-crossfade).
/// Long enough to mask the incoming path's cold-start transient (VCO 1 ms self-
/// start, ladder warm-up) and any oscillator phase jump, short enough to feel
/// instant.
const XFADE_SECS: f32 = 0.010;

/// Compensating-delay ring size. Must exceed the largest tier OS group delay
/// (Os4x = 48 base samples) so every tier can be padded up to the common max.
const COMP_BUF: usize = 64;

/// Fixed integer-sample delay line (zero-alloc) used to pad each tier's output
/// to the common reported latency, so the host PDC value never changes at
/// runtime and a crossfade blends time-aligned paths.
#[derive(Clone)]
struct CompDelay {
    buf: [f32; COMP_BUF],
    pos: usize,
    delay: usize,
}

impl CompDelay {
    fn new(delay: usize) -> Self {
        Self {
            buf: [0.0; COMP_BUF],
            pos: 0,
            delay,
        }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        self.buf[self.pos] = x;
        let rd = (self.pos + COMP_BUF - self.delay) % COMP_BUF;
        let y = self.buf[rd];
        self.pos = (self.pos + 1) % COMP_BUF;
        y
    }

    /// Clear the delay line to silence, keeping the configured `delay`.
    fn reset(&mut self) {
        self.buf = [0.0; COMP_BUF];
        self.pos = 0;
    }
}

pub struct Voice {
    /// Low-tier polyBLEP saw.
    osc: Saw,
    /// High-tier polyBLEP saw — a *separate* oscillator so Low and High can run
    /// simultaneously during a crossfade without double-advancing one shared
    /// phase (which would jump the pitch mid-fade).
    osc_hi: Saw,
    /// Ultra-tier nodal circuit VCO (replaces the polyBLEP saw for Ultra only).
    vco: Vco303,
    filter: TptLadder,
    /// Ultra-tier nodal diode-ladder, preallocated, run inside `os`.
    ladder: LadderVcf,
    os: Os4x,
    /// High-tier nodal diode-ladder (polyBLEP saw source), run inside `os2`.
    ladder_hi: LadderVcf,
    os2: Os2x,
    env: AdEnv,
    /// Active (target) tier.
    tier: Tier,
    /// Tier being faded *out*, if a crossfade is in progress. `None` = steady.
    xfade_from: Option<Tier>,
    /// Crossfade position in [0, 1] (0 = all `xfade_from`, 1 = all `tier`).
    xfade_pos: f32,
    /// Per-sample crossfade increment (= 1 / fade-length-samples).
    xfade_step: f32,
    /// Tier requested while a fade was still in flight — applied as the next fade
    /// once the current one completes, so a fast multi-tier sweep never hard-cuts
    /// a still-weighted path.
    pending_tier: Option<Tier>,
    /// Per-tier output delays that pad each path up to `max_latency`, so the
    /// reported host PDC is constant and the crossfade blends aligned signals.
    comp_low: CompDelay,
    comp_high: CompDelay,
    comp_ultra: CompDelay,
    /// Common reported latency (base samples) = max OS group delay over all tiers.
    max_latency: usize,
    sr: f32,
}

impl Voice {
    pub fn new(sr: f32) -> Self {
        // Per-tier intrinsic OS group delays; pad each up to the common max.
        let lat_low = 0;
        let lat_high = Os2x::new().latency_samples();
        let lat_ultra = Os4x::new().latency_samples();
        let max_latency = lat_ultra.max(lat_high).max(lat_low);
        let mut ladder = LadderVcf::new(sr * ULTRA_OS_FACTOR);
        ladder.set_cal(ULTRA_BRIGHT);
        let mut ladder_hi = LadderVcf::new(sr * HIGH_OS_FACTOR);
        ladder_hi.set_cal(HIGH_BRIGHT);
        Self {
            osc: Saw::new(),
            osc_hi: Saw::new(),
            vco: Vco303::new(sr),
            filter: TptLadder::new(sr),
            ladder,
            os: Os4x::new(),
            ladder_hi,
            os2: Os2x::new(),
            env: AdEnv::new(sr),
            tier: Tier::Low,
            xfade_from: None,
            xfade_pos: 1.0,
            xfade_step: 1.0 / (sr * XFADE_SECS).max(1.0),
            pending_tier: None,
            comp_low: CompDelay::new(max_latency - lat_low),
            comp_high: CompDelay::new(max_latency - lat_high),
            comp_ultra: CompDelay::new(max_latency - lat_ultra),
            max_latency,
            sr,
        }
    }

    /// Select the fidelity tier. Low routes to the TPT path; High routes to the
    /// nodal diode-ladder inside the 2× oversampler (polyBLEP saw, F4.1); Ultra
    /// routes to the circuit VCO → nodal ladder at 4× (F3.5).
    ///
    /// A change starts a short equal-power crossfade (F5.1) from the previous
    /// tier so the path switch is click-free — the tiers have different timbre
    /// and the incoming path starts cold, so a hard switch would pop. No
    /// allocation, RT-safe (plain field writes).
    /// Set the tier with no crossfade — for the initial/persisted selection at
    /// construction or `initialize`, where there is no prior audio to fade from.
    /// Runtime changes go through [`set_tier`] (faded).
    pub fn set_tier_immediate(&mut self, tier: Tier) {
        self.tier = tier;
        self.xfade_from = None;
        self.xfade_pos = 1.0;
        self.pending_tier = None;
    }

    /// Clear all runtime state to silence, **in place** — the host reset path.
    ///
    /// nih-plug's `reset()` may fire from the audio thread and must not allocate
    /// (`plugin.rs` `reset` contract). Reconstructing the Voice (`Voice::new`)
    /// would re-enter the VCO pitch-LUT cache: on a warm cache that is only a
    /// `Mutex` lock, but on a cold cache it runs the secant LUT build (heavy) +
    /// a `Vec::push` allocation — both illegal here. So the LUT-bearing VCO and
    /// the nodal ladders are reset *in place* (dynamic state cleared; calibration
    /// and the cached LUT preserved), and the remaining fixed-array components
    /// are cheap, alloc-free `new()`s. The cache is therefore touched only from
    /// `initialize()` (non-RT). Leaves a clean steady Low; the caller re-applies
    /// the persisted tier via [`set_tier_immediate`].
    pub fn reset(&mut self) {
        self.osc = Saw::new();
        self.osc_hi = Saw::new();
        self.vco.reset(); // preserves cv_lut — no cache lock, no rebuild, no alloc
        self.filter.reset();
        self.ladder.reset(); // preserves cutoff / reso / brightness
        self.ladder_hi.reset();
        self.os = Os4x::new();
        self.os2 = Os2x::new();
        self.env = AdEnv::new(self.sr);
        self.tier = Tier::Low;
        self.xfade_from = None;
        self.xfade_pos = 1.0;
        self.pending_tier = None;
        self.comp_low.reset();
        self.comp_high.reset();
        self.comp_ultra.reset();
    }

    pub fn set_tier(&mut self, tier: Tier) {
        // If a fade is already in flight, queue the new target instead of
        // clobbering the outgoing path (which would hard-cut a still-weighted
        // tier → click on a fast multi-tier sweep). The queued tier starts its
        // fade when the current one completes (see `tick`).
        if self.xfade_from.is_some() {
            self.pending_tier = if tier == self.tier { None } else { Some(tier) };
            return;
        }
        if tier == self.tier {
            return;
        }
        self.xfade_from = Some(self.tier);
        self.xfade_pos = 0.0;
        self.tier = tier;
    }

    /// Reported host latency (base samples). Constant across tiers: every tier is
    /// padded to the max OS group delay (Ultra's 4×), so the PDC value never
    /// changes at runtime — no host graph re-init on a tier switch, and the
    /// crossfade blends paths that share the same total latency.
    pub fn latency_samples(&self) -> usize {
        self.max_latency
    }

    pub fn note_on(&mut self, note: u8) {
        let f = midi_to_freq(note);
        self.osc.set_freq(f, self.sr);
        self.osc_hi.set_freq(f, self.sr);
        self.vco.set_freq(f);
        self.env.trigger();
    }

    /// Filter cutoff in Hz. Applied to every tier's backend so tier switches are
    /// click-free on the parameter axis.
    pub fn set_cutoff(&mut self, hz: f32) {
        self.filter.set_cutoff(hz);
        self.ladder.set_cutoff(hz);
        self.ladder_hi.set_cutoff(hz);
    }

    /// Filter resonance in [0, 1].
    pub fn set_reso(&mut self, r: f32) {
        self.filter.set_reso(r);
        self.ladder.set_reso(r);
        self.ladder_hi.set_reso(r);
    }

    /// Ultra-tier VCF brightness calibration (spectral V5). Default is the
    /// ladder's built-in 1.0; the spectral gate raises it toward the SPICE
    /// envelope. No effect on Low/High.
    pub fn set_ultra_brightness(&mut self, cal: f64) {
        self.ladder.set_cal(cal);
    }

    /// Diagnostic: override the Ultra ladder's Q9 resonance feedback gain (for
    /// spectral calibration sweeps against the gold). Not used at runtime.
    pub fn set_ultra_reso_gain(&mut self, g: f64) {
        self.ladder.set_reso_gain(g);
    }

    /// High-tier VCF brightness calibration. High shares the `LadderVcf` core but
    /// runs at 2× (vs Ultra's 4×); with the active Q9 resonance the brightness
    /// optimum differs, so it is tuned independently of `set_ultra_brightness`.
    pub fn set_high_brightness(&mut self, cal: f64) {
        self.ladder_hi.set_cal(cal);
    }

    /// One pre-envelope sample from a single tier's signal path, advancing only
    /// that tier's backend state. Each tier owns independent oscillator/filter
    /// state, so two tiers can be rendered in the same sample (for a crossfade)
    /// without interfering.
    fn render_tier(&mut self, tier: Tier) -> f32 {
        match tier {
            // Low: polyBLEP saw → TPT ladder (cheap, ships). Padded to the common
            // latency so the host PDC is constant across tiers.
            Tier::Low => {
                let y = self.filter.tick(self.osc.tick());
                self.comp_low.process(y)
            }
            // High (F4.1): polyBLEP saw → nodal diode-ladder at 2× oversample.
            // Skips the costly nodal VCO Newton but keeps the circuit-filter
            // character — sits between Low and Ultra at a fraction of Ultra cost.
            Tier::High => {
                let hi = self.os2.up(self.osc_hi.tick());
                let o = [self.ladder_hi.tick(hi[0]), self.ladder_hi.tick(hi[1])];
                // HIGH_LEVEL_TRIM equalises High's loudness to Ultra (tier-switch
                // is then timbre-only). Pre-comp-delay so the crossfade blends
                // level-matched paths.
                self.comp_high.process(self.os2.down(o) * HIGH_LEVEL_TRIM)
            }
            // Ultra: circuit VCO saw → nodal diode-ladder at 4× oversample.
            // The nodal VCO runs at base rate; its sharp JFET reset edge is corrected
            // by a 2-tap first-order BLEP computed inside `Vco303::tick()`. `blep_now`
            // is already calibrated to SPICE OR_SWING × VCO_GAIN (= 2.92 V); tap 1 is
            // auto-injected on the following tick. The 4× OS + decimation FIR then
            // band-limits the downstream ladder nonlinearity as before.
            //
            // RESIDUAL LIMITATION: the VCO still runs at base rate; higher-order alias
            // from the smooth ramp (not the reset edge) is negligible relative to the
            // reset. Option A (full 4× VCO, future `Tier::UltraHd`) gives complete alias
            // freedom at ~4× the CPU cost.
            Tier::Ultra => {
                let s = self.vco.tick() * VCO_GAIN + self.vco.blep_now;
                let hi = self.os.up(s);
                let o = [
                    self.ladder.tick(hi[0]),
                    self.ladder.tick(hi[1]),
                    self.ladder.tick(hi[2]),
                    self.ladder.tick(hi[3]),
                ];
                self.comp_ultra.process(self.os.down(o))
            }
        }
    }

    pub fn tick(&mut self) -> f32 {
        let f = match self.xfade_from {
            // Steady state: render the active tier only.
            None => self.render_tier(self.tier),
            // Crossfade: render both paths and blend equal-power. The outgoing
            // path keeps running until the fade completes, then is dropped.
            Some(from) => {
                let a = self.render_tier(from);
                let b = self.render_tier(self.tier);
                let g = self.xfade_pos.min(1.0);
                // Equal-power (constant-energy) weights: the tiers are largely
                // uncorrelated in timbre, so equal-power avoids the mid-fade dip
                // a linear blend would give.
                let theta = g * std::f32::consts::FRAC_PI_2;
                let out = a * theta.cos() + b * theta.sin();
                self.xfade_pos += self.xfade_step;
                if self.xfade_pos >= 1.0 {
                    self.xfade_from = None;
                    // Fade complete: launch any tier queued mid-fade.
                    if let Some(next) = self.pending_tier.take() {
                        if next != self.tier {
                            self.xfade_from = Some(self.tier);
                            self.xfade_pos = 0.0;
                            self.tier = next;
                        }
                    }
                }
                out
            }
        };
        f * self.env.tick()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midi_to_freq_anchors() {
        assert!((midi_to_freq(69) - 440.0).abs() < 0.01);
        assert!((midi_to_freq(81) - 880.0).abs() < 0.01); // one octave up
        assert!((midi_to_freq(57) - 220.0).abs() < 0.01); // one octave down
    }

    fn rms(v: &mut Voice, n: usize) -> f32 {
        let mut acc = 0.0f32;
        for _ in 0..n {
            let y = v.tick();
            acc += y * y;
        }
        (acc / n as f32).sqrt()
    }

    #[test]
    fn silent_before_note_on() {
        let mut v = Voice::new(48_000.0);
        assert_eq!(rms(&mut v, 1_000), 0.0);
    }

    #[test]
    fn cutoff_setter_changes_brightness() {
        fn hf_rms(cut: f32) -> f32 {
            let mut v = Voice::new(48_000.0);
            v.set_cutoff(cut);
            v.note_on(57);
            for _ in 0..400 {
                v.tick();
            }
            let mut acc = 0.0f32;
            for _ in 0..4_000 {
                let s = v.tick();
                acc += s * s;
            }
            (acc / 4_000.0).sqrt()
        }
        assert!(
            hf_rms(4_000.0) > hf_rms(300.0) * 1.5,
            "cutoff did not change output energy"
        );
    }

    #[test]
    fn latency_is_constant_across_tiers() {
        // PDC contract (F5.2, revised): all tiers are padded to the max OS group
        // delay so the reported host latency never changes at runtime (no graph
        // re-init on a switch). It equals Ultra's 4× round-trip group delay.
        use super::super::oversample::Os4x;
        let expect = Os4x::new().latency_samples();
        let mut v = Voice::new(48_000.0);
        assert!(expect > 0, "Ultra OS must add latency");
        for tier in [Tier::Low, Tier::High, Tier::Ultra] {
            v.set_tier_immediate(tier);
            assert_eq!(
                v.latency_samples(),
                expect,
                "tier {tier:?} latency must equal the common max"
            );
        }
    }

    #[test]
    fn fast_tier_sweep_queues_without_clobbering() {
        // Low→High→Ultra within one fade window must not hard-cut the still-
        // weighted path: the second change is queued, not applied instantly.
        let mut v = Voice::new(48_000.0);
        v.note_on(45);
        v.set_tier_immediate(Tier::Low);
        v.tick();
        v.set_tier(Tier::High); // starts Low→High fade
        v.tick();
        v.set_tier(Tier::Ultra); // mid-fade → queued
        assert_eq!(
            v.tier,
            Tier::High,
            "second change must not pre-empt the fade"
        );
        assert_eq!(v.pending_tier, Some(Tier::Ultra), "Ultra must be queued");
        // Run past the fade; the queued Ultra fade should then take over.
        for _ in 0..1_000 {
            v.tick();
        }
        assert_eq!(
            v.tier,
            Tier::Ultra,
            "queued tier must engage after the fade"
        );
        assert_eq!(v.pending_tier, None, "queue cleared once applied");
    }

    #[test]
    fn produces_signal_after_note_on() {
        let mut v = Voice::new(48_000.0);
        v.note_on(69);
        // Skip the short attack, then measure the sustained decay body.
        for _ in 0..200 {
            v.tick();
        }
        assert!(rms(&mut v, 2_000) > 0.01, "voice produced no signal");
    }
}
