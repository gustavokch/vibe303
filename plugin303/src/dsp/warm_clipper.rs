//! Global WARMCLIPPER output stage.
//!
//! The hardware netlist is feed-forward, so it is intentionally evaluated as
//! four small, stateful stages rather than as one large zero-delay feedback
//! problem. Each stage owns its capacitor/integrator state and is warm started
//! by the previous oversampled sample. This keeps the audio path bounded and
//! allocation-free while retaining the important circuit behaviour: a
//! GB-referenced high-passed detector, fast attack/slow release JFET shunt,
//! germanium feedback clipping, and an unducked low-frequency blend.

use super::oversample::{Os2x, Os4x};

pub const CLIPPER_LATENCY: usize = 48;
const XFADE_SECONDS: f32 = 0.010;
const GB: f32 = -3.826;
const JFET_FLOOR: f32 = 0.089_125;

const GAIN_MINUS_12DB: f32 = 0.251_188_64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipperOs {
    X2,
    X4,
}

#[derive(Clone, Copy, Debug)]
pub struct ClipperParams {
    pub bypass: bool,
    pub os: ClipperOs,
    pub drive_ohms: f32,
    pub threshold_ohms: f32,
    pub bass_ohms: f32,
    pub tone_f: f32,
    pub asym: bool,
    pub mix: f32,      // 0.0 = dry, 1.0 = wet; equal-power law
    pub in_db: f32,    // pre-clipper input gain
    pub out_db: f32,   // post-clipper output gain
    pub tilt_db: f32,  // pre/post shelf pair (Task 2)
    pub dynamics: f32, // transient drive ducking (Task 3)
}

impl Default for ClipperParams {
    fn default() -> Self {
        Self {
            bypass: false,
            os: ClipperOs::X2,
            drive_ohms: 47_000.0,
            threshold_ohms: 4_700.0,
            bass_ohms: 8_200.0,
            tone_f: 1e-9,
            asym: false,
            mix: 1.0, // DSP default stays wet; the plugin param defaults to dry
            in_db: 0.0,
            out_db: 0.0,
            tilt_db: 0.0,
            dynamics: 0.0,
        }
    }
}

#[derive(Clone, Copy)]
struct Delay48 {
    buf: [f32; CLIPPER_LATENCY],
    pos: usize,
}

impl Delay48 {
    fn new() -> Self { Self { buf: [0.0; CLIPPER_LATENCY], pos: 0 } }
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.buf[self.pos];
        self.buf[self.pos] = x;
        self.pos = (self.pos + 1) % CLIPPER_LATENCY;
        y
    }
    fn reset(&mut self) { *self = Self::new(); }
}

#[derive(Clone, Copy)]
struct Delay16 {
    buf: [f32; 16],
    pos: usize,
}

impl Delay16 {
    fn new() -> Self { Self { buf: [0.0; 16], pos: 0 } }
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.buf[self.pos];
        self.buf[self.pos] = x;
        self.pos = (self.pos + 1) % 16;
        y
    }
    fn reset(&mut self) { *self = Self::new(); }
}

/// A compact state-space implementation of stages A--D in `warm_clipper.cir`.
struct StagedEngine {
    sr: f32,
    // A: buffer / drive amp state
    drive_state: f32,
    drive_alpha: f32,
    // B: two HPF sections, envelope and gate (all referenced to GB at rest)
    hp_x1: f32,
    hp_y1: f32,
    hp_x2: f32,
    hp_y2: f32,
    hp_a: f32,
    env_attack: f32,
    env_release: f32,
    env: f32,
    gate: f32,
    // C: tone capacitor; D: two bass low-pass capacitors
    tone: f32,
    tone_alpha: f32,
    bass1: f32,
    bass2: f32,
    bass_alpha: f32,
    params: ClipperParams,
}

impl StagedEngine {
    fn new(sr: f32) -> Self {
        let drive_alpha = 1.0 - (-2.0 * core::f32::consts::PI * 10_000.0 / sr).exp();
        let hp_a = (-2.0 * core::f32::consts::PI * 234.0 / sr).exp();
        let env_attack = 1.0 - (-1.0 / (0.001 * sr)).exp();
        let env_release = 1.0 - (-1.0 / (0.220 * sr)).exp();
        let bass_alpha = 1.0 - (-2.0 * core::f32::consts::PI * 106.0 / sr).exp();

        let mut engine = Self {
            sr,
            drive_state: 0.0,
            drive_alpha,
            hp_x1: 0.0, hp_y1: 0.0, hp_x2: 0.0, hp_y2: 0.0,
            hp_a,
            env_attack,
            env_release,
            env: GB, gate: GB, tone: 0.0, tone_alpha: 0.0, bass1: 0.0, bass2: 0.0,
            bass_alpha,
            params: ClipperParams::default(),
        };
        engine.update_tone_alpha();
        engine
    }

    fn update_tone_alpha(&mut self) {
        let tone_hz = (1.0 / (2.0 * core::f32::consts::PI * 22_000.0 * self.params.tone_f.clamp(470e-12, 4.7e-9)))
            .clamp(1_500.0, 16_000.0);
        self.tone_alpha = 1.0 - (-2.0 * core::f32::consts::PI * tone_hz / self.sr).exp();
    }

    fn set_params(&mut self, params: ClipperParams) {
        self.params = params;
        self.update_tone_alpha();
    }

    fn reset(&mut self) { *self = Self::new(self.sr); }

    #[inline]
    fn one_pole(state: &mut f32, x: f32, alpha: f32) -> f32 {
        *state += alpha * (x - *state);
        *state
    }

    /// Stages A→B→C→D, with exact same-sample handoffs. Gate junction
    /// coupling uses the previous stage-C state implicitly (the documented
    /// one-sweep-free policy; Ratk dominates in the operating region).
    fn tick(&mut self, input: f32) -> f32 {
        let x = if input.is_finite() { input.clamp(-16.0, 16.0) } else { 0.0 };
        let p = self.params;

        // A: U1 buffer + saturating U2. `tanh` is the anti-windup ±12.6 V
        // op-amp clamp expressed in normalised audio volts.
        let buf = x * 2.0;
        let gain = 1.0 + p.drive_ohms.clamp(22_000.0, 100_000.0) / 4_700.0;
        let target_drv = 12.0 * (buf * gain / 12.0).tanh();
        self.drive_state = Self::one_pole(&mut self.drive_state, target_drv, self.drive_alpha);
        let drv = self.drive_state;

        // B: GB-referenced two-pole HPF → U5/Rsens gain → D3/Cenv and
        // D4/Ratk/Cg. Detector caps are deliberately separate so their rest
        // voltage remains GB, not ground.
        let h1 = self.hp_a * (self.hp_y1 + drv - self.hp_x1);
        self.hp_x1 = drv; self.hp_y1 = h1;
        let h2 = self.hp_a * (self.hp_y2 + h1 - self.hp_x2);
        self.hp_x2 = h1; self.hp_y2 = h2;
        let sens_gain = (p.threshold_ohms.clamp(2_200.0, 22_000.0) / 4_700.0).recip();
        let rectified = (h2 * 0.18 * sens_gain - 0.50).max(0.0);
        let env_target = GB + rectified.min(12.0);
        let env_a = if env_target > self.env { self.env_attack } else { self.env_release };
        self.env += env_a * (env_target - self.env);
        // D4/Ratk attack and Rg1 release into Cg. The envelope cannot exceed
        // the U5 rail, preventing release time from growing with overdrive.
        let gate_a = if self.env > self.gate { self.env_attack } else { self.env_release };
        self.gate += gate_a * (self.env.clamp(GB, 0.0) - self.gate);

        // C: Rs/Rpad and 2N5457 shunt. Gate moves from GB (off) to 0 (on),
        // producing a bounded -21 dB floor before the Ge feedback clipper.
        let open = ((-self.gate) / -GB).clamp(0.0, 1.0);
        let vca_gain = JFET_FLOOR + (1.0 - JFET_FLOOR) * open * open;
        let vca = drv * vca_gain;
        let ge_knee = 1.65;
        let clipped = if p.asym {
            // Large equivalent series resistance on D2b: retain the topology
            // but make the negative feedback branch intentionally weak.
            if vca >= 0.0 { ge_knee * (vca / ge_knee).tanh() } else { vca * 0.55 }
        } else {
            ge_knee * (vca / ge_knee).tanh()
        };
        let clp = Self::one_pole(&mut self.tone, clipped, self.tone_alpha);

        // D: clean 106 Hz 2-pole bass leg (U6 polarity correction) and U4
        // summer with make-up gain.
        let b1 = Self::one_pole(&mut self.bass1, buf, self.bass_alpha);
        let bass = Self::one_pole(&mut self.bass2, b1, self.bass_alpha);
        let bass_mix = (8_200.0 / p.bass_ohms.clamp(4_700.0, 22_000.0)).clamp(0.37, 1.75);
        let out = (clp * 1.35 + bass * 0.20 * bass_mix) * GAIN_MINUS_12DB;
        if out.is_finite() { out } else { 0.0 }
    }
}

/// Preallocated 2× and 4× engines. The 2× and bypass paths are padded to the
/// 4× filter's measured 48-sample round-trip delay, so callers always report
/// the same 48-sample clipper latency.
pub struct WarmClipper {
    os2: Os2x,
    os4: Os4x,
    e2: StagedEngine,
    e4: StagedEngine,
    pad2: Delay16,
    dry: Delay48,
    params: ClipperParams,
    active_bypass: bool,
    active_os: ClipperOs,
    fade_from_bypass: bool,
    fade_from_os: ClipperOs,
    fade: f32,
    fade_step: f32,
    // Base-rate smoothed scalars for the new control dimensions.
    smooth_k: f32,
    in_t: f32, in_s: f32,
    out_t: f32, out_s: f32,
    dry_t: f32, dry_s: f32,
    wet_t: f32, wet_s: f32,
}

impl WarmClipper {
    pub fn new(sr: f32) -> Self {
        Self {
            os2: Os2x::new(), os4: Os4x::new(), e2: StagedEngine::new(sr * 2.0), e4: StagedEngine::new(sr * 4.0),
            pad2: Delay16::new(), dry: Delay48::new(), params: ClipperParams::default(),
            active_bypass: false, active_os: ClipperOs::X2, fade_from_bypass: false,
            fade_from_os: ClipperOs::X2, fade: 1.0, fade_step: 1.0 / (sr * XFADE_SECONDS).max(1.0),
            smooth_k: 1.0 - (-1.0 / (0.010 * sr)).exp(),
            in_t: 1.0, in_s: 1.0,
            out_t: 1.0, out_s: 1.0,
            dry_t: 0.0, dry_s: 0.0,
            wet_t: 1.0, wet_s: 1.0,
        }
    }
    pub fn latency_samples(&self) -> usize { CLIPPER_LATENCY }
    fn set_targets(&mut self, p: &ClipperParams) {
        self.in_t = 10f32.powf(p.in_db.clamp(-18.0, 18.0) / 20.0);
        self.out_t = 10f32.powf(p.out_db.clamp(-18.0, 18.0) / 20.0);
        let angle = p.mix.clamp(0.0, 1.0) * core::f32::consts::FRAC_PI_2;
        self.dry_t = angle.cos();
        self.wet_t = angle.sin();
    }
    fn snap_smoothers(&mut self) {
        self.in_s = self.in_t;
        self.out_s = self.out_t;
        self.dry_s = self.dry_t;
        self.wet_s = self.wet_t;
    }
    pub fn set_params(&mut self, params: ClipperParams) {
        self.e2.set_params(params); self.e4.set_params(params);
        if params.bypass != self.active_bypass || params.os != self.active_os {
            self.fade_from_bypass = self.active_bypass;
            self.fade_from_os = self.active_os;
            self.active_bypass = params.bypass;
            self.active_os = params.os;
            self.fade = 0.0;
        }
        self.set_targets(&params);
        self.params = params;
    }
    pub fn set_params_immediate(&mut self, params: ClipperParams) {
        self.e2.set_params(params); self.e4.set_params(params);
        self.active_bypass = params.bypass;
        self.active_os = params.os;
        self.fade_from_bypass = params.bypass;
        self.fade_from_os = params.os;
        self.fade = 1.0;
        self.set_targets(&params);
        self.snap_smoothers();
        self.params = params;
    }
    fn wet2(&mut self, x: f32) -> f32 {
        let hi = self.os2.up(x);
        // 2× round-trip is 32 samples. Pad 16 samples to the 4× 48-sample path.
        self.pad2.tick(self.os2.down([self.e2.tick(hi[0]), self.e2.tick(hi[1])]))
    }
    fn wet4(&mut self, x: f32) -> f32 {
        let hi = self.os4.up(x);
        self.os4.down([self.e4.tick(hi[0]), self.e4.tick(hi[1]), self.e4.tick(hi[2]), self.e4.tick(hi[3])])
    }
    fn branch(&mut self, bypass: bool, os: ClipperOs, x: f32, d: f32) -> f32 {
        if bypass {
            d
        } else {
            let xg = x * self.in_s;
            let wet = match os {
                ClipperOs::X2 => self.wet2(xg),
                ClipperOs::X4 => self.wet4(xg),
            } * self.out_s;
            self.dry_s * d + self.wet_s * wet
        }
    }
    pub fn tick(&mut self, x: f32) -> f32 {
        // Dry delay runs every sample so the mix path always has a warm buffer.
        // Sanitise: 0.0 * NaN is NaN, so a poisoned dry buffer would leak
        // through the mix law even at dry_s = 0.
        let d = self.dry.tick(x);
        let d = if d.is_finite() { d } else { 0.0 };
        self.in_s += self.smooth_k * (self.in_t - self.in_s);
        self.out_s += self.smooth_k * (self.out_t - self.out_s);
        self.dry_s += self.smooth_k * (self.dry_t - self.dry_s);
        self.wet_s += self.smooth_k * (self.wet_t - self.wet_s);
        // Both paths are only rendered during a transition. A bypassed steady
        // engine therefore incurs no nonlinear solve.
        let to = self.branch(self.active_bypass, self.active_os, x, d);
        if self.fade >= 1.0 { return to; }
        let from = self.branch(self.fade_from_bypass, self.fade_from_os, x, d);
        let theta = self.fade * core::f32::consts::FRAC_PI_2;
        let y = from * theta.cos() + to * theta.sin();
        self.fade = (self.fade + self.fade_step).min(1.0);
        y
    }
    pub fn reset(&mut self) {
        self.os2 = Os2x::new(); self.os4 = Os4x::new(); self.e2.reset(); self.e4.reset();
        self.pad2.reset(); self.dry.reset(); self.fade = 1.0;
        self.snap_smoothers();
        self.active_bypass = self.params.bypass;
        self.active_os = self.params.os;
        self.fade_from_bypass = self.params.bypass;
        self.fade_from_os = self.params.os;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rest_is_finite_and_silent() {
        let mut c = WarmClipper::new(48_000.0);
        for _ in 0..512 { assert!(c.tick(0.0).abs() < 1e-6); }
    }
    #[test]
    fn bypass_is_delay_matched() {
        let mut c = WarmClipper::new(48_000.0);
        let mut p = ClipperParams::default(); p.bypass = true; c.set_params(p);
        // Runtime bypass changes are deliberately click-free. Finish the 10 ms
        // transition before asserting the steady bypass transfer function.
        for _ in 0..512 { c.tick(0.0); }
        for i in 0..CLIPPER_LATENCY { assert_eq!(c.tick(if i == 0 { 1.0 } else { 0.0 }), 0.0); }
        assert_eq!(c.tick(0.0), 1.0);
    }
    #[test]
    fn hot_and_nonfinite_input_stay_bounded() {
        let mut c = WarmClipper::new(48_000.0);
        for &x in &[f32::NAN, f32::INFINITY, -f32::INFINITY, 1000.0] {
            for _ in 0..128 { let y = c.tick(x); assert!(y.is_finite() && y.abs() < 8.0); }
        }
    }
    #[test]
    fn mix_half_equal_power_gains() {
        let mut c = WarmClipper::new(48_000.0);
        let mut p = ClipperParams::default();
        p.mix = 0.5;
        c.set_params_immediate(p);
        let expect = (0.5f32 * core::f32::consts::FRAC_PI_2).sin(); // 0.7071
        assert!((c.dry_s - expect).abs() < 1e-6);
        assert!((c.wet_s - expect).abs() < 1e-6);
    }
}
