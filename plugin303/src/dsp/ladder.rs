//! Diode-ladder VCF. M1: a linear 4-pole TPT lowpass (4 cascaded 1-pole stages).
//! Resonance, per-stage tanh, tap-select and the vco/vcf303 fit come in M2.

pub struct TptLadder {
    g: f32,
    z: [f32; 4],
    sr: f32,
}

impl TptLadder {
    pub fn new(sr: f32) -> Self {
        let mut s = Self { g: 0.0, z: [0.0; 4], sr };
        s.set_cutoff(1_000.0);
        s
    }

    pub fn set_cutoff(&mut self, hz: f32) {
        let wd = std::f32::consts::PI * hz.clamp(20.0, self.sr * 0.45) / self.sr;
        self.g = wd.tan(); // per-stage 1-pole TPT coefficient
    }

    pub fn tick(&mut self, x: f32) -> f32 {
        let mut v = x;
        for z in self.z.iter_mut() {
            // 1-pole TPT: hp = (in - state) / (1 + g); lp = hp*g + state; state += 2*hp*g.
            let hp = (v - *z) / (1.0 + self.g);
            let lp = hp * self.g + *z;
            *z = lp + hp * self.g;
            v = lp;
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(f: &mut TptLadder, freq: f32, sr: f32) -> f32 {
        let mut acc = 0.0f32;
        let n = 20_000;
        let mut ph = 0.0f32;
        for _ in 0..n {
            let x = (2.0 * std::f32::consts::PI * ph).sin();
            let y = f.tick(x);
            acc += y * y;
            ph += freq / sr;
            if ph >= 1.0 {
                ph -= 1.0;
            }
        }
        (acc / n as f32).sqrt()
    }

    #[test]
    fn passes_dc_attenuates_hf() {
        let sr = 48_000.0;
        let mut f = TptLadder::new(sr);
        f.set_cutoff(1_000.0);
        let lo = rms(&mut f, 100.0, sr); // well below cutoff
        let mut f2 = TptLadder::new(sr);
        f2.set_cutoff(1_000.0);
        let hi = rms(&mut f2, 8_000.0, sr); // 3 octaves above
        assert!(lo > 0.6, "passband too low: {lo}");
        assert!(hi < 0.05, "stopband too high: {hi}");
    }
}
