use super::*;

/// Fixed injection reference for the J-A magnetizing-deviation current. The two front-panel dials
/// shape the *physical character* of the core (knee position, loop width); this constant sets the
/// overall injection depth. Pinned to 0.5 so the default panel state (switch ON, both dials at noon)
/// reproduces the legacy `trafo=0.5` sound exactly — the old single knob halved the deviation at its
/// default, and that depth is preserved here while the dials take over character.
#[cfg(feature = "trafo-core")]
const JA_DEV_REF: f64 = 0.5;

impl WdfModel {

    /// Update the RV1 split (bias pot) + rebuild G0, preserving all integrator state
    /// (`main.cpp:432-438`). Composes the aged drift factor on each leg (see `set_age`).
    pub fn set_bias(&mut self, bias: f64) {
        let (r_upper, r_lower) = rv1_split(bias);
        self.res[self.r_upper_idx].g = 1.0 / (r_upper * self.age_factor_res(self.r_upper_idx));
        self.res[self.r_lower_idx].g = 1.0 / (r_lower * self.age_factor_res(self.r_lower_idx));
        self.build_g0();
    }

    /// Toggle the in-solve ADAA1 transport spike (`adaa-insolve` feature only). Pure A/B switch for
    /// the offline harness — no state change, no rebuild. Off by default; never set in the plugin.
    #[cfg(feature = "adaa-insolve")]
    pub fn set_adaa(&mut self, on: bool) {
        self.adaa = on;
    }

    /// Toggle the Path-1 dominant-variable ADAA (`adaa-port` feature only). Pure A/B switch for the
    /// offline harness — no state change, no rebuild. Off by default; never set in the plugin.
    #[cfg(feature = "adaa-port")]
    pub fn set_adaa_port(&mut self, on: bool) {
        self.adaa_port = on;
    }

    /// Update R_KJ (circuit gain) + rebuild G0, preserving integrator state. New vs the C++
    /// reference (which only sets gain at build time); mirrors `set_bias`. Composes aged drift.
    pub fn set_rgain(&mut self, rgain: f64) {
        self.res[self.r_kj_idx].g = 1.0 / (rgain * self.age_factor_res(self.r_kj_idx));
        self.build_g0();
    }

    /// Aged resistance multiplier for resistor `idx`: `1 + age*dev`. Clamped away from 0.
    #[inline]
    fn age_factor_res(&self, idx: usize) -> f64 {
        (1.0 + self.age * self.age_pattern.res_dev[idx]).max(1e-6)
    }

    /// Aged primary/secondary winding inductance at the current age (drift from `ind_dev`).
    #[inline]
    fn aged_lp(&self) -> f64 {
        (self.lp_base * (1.0 + self.age * self.age_pattern.ind_dev[0])).max(1.0e-3)
    }
    #[inline]
    fn aged_ls(&self) -> f64 {
        (self.ls_base * (1.0 + self.age * self.age_pattern.ind_dev[1])).max(1.0e-3)
    }

    /// Rebuild the coupled-inductor companion `gl = (dt/2)·inv([Lp M; M Ls])` from (aged) winding
    /// inductances. Mirrors the build-time stamp; called whenever the age knob drifts the core.
    fn recompute_gl(&mut self, lp: f64, ls: f64, k: f64) {
        let m = k * (lp * ls).sqrt();
        let det = lp * ls - m * m;
        let l_inv = [[ls / det, -m / det], [-m / det, lp / det]];
        let h = self.dt * 0.5;
        self.gl = [
            [h * l_inv[0][0], h * l_inv[0][1]],
            [h * l_inv[1][0], h * l_inv[1][1]],
        ];
    }

    /// Re-voice the JA core from the cached dials and compose the per-unit aging factors: retune the
    /// `lp_eff`/`k_hi` lump to the aged primary inductance `lp_aged`, then trim the knee/pinning/
    /// reversibility. At age=0 all factors are 1 and `lp_aged == lp_base`, so this is bit-exact with
    /// the un-aged voicing.
    #[cfg(feature = "trafo-core")]
    fn apply_trafo_aged(&mut self, lp_aged: f64) {
        self.ja.set_lp_eff(lp_aged);
        self.ja.set_voicing(self.trafo_hyst, self.trafo_sat);
        let f_ms = (1.0 + self.age * self.age_pattern.core_dev[0]).max(1e-3);
        let f_k = (1.0 + self.age * self.age_pattern.core_dev[1]).max(1e-3);
        let f_c = (1.0 + self.age * self.age_pattern.core_dev[2]).max(1e-3);
        self.ja.apply_core_aging(f_ms, f_k, f_c);
    }

    /// Apply the aging knob: re-derive every resistor `g` and cap `geq` from its pristine base
    /// times the per-unit drift factor `(1 + age*dev)`, then rebuild G0 (+ Schur). Block-rate;
    /// preserves integrator state (dynamic re-settle, exactly like `set_bias`). The three
    /// parameter-derived legs (`r_kj`/`r_upper`/`r_lower`) are skipped here — their drift is folded
    /// in by `set_bias`/`set_rgain` via `age_factor_res`, which the plugin re-applies after a
    /// change.
    pub fn set_age(&mut self, age: f64) {
        self.age = age.clamp(0.0, 1.0);
        for i in 0..self.n_res {
            if i == self.r_kj_idx || i == self.r_upper_idx || i == self.r_lower_idx {
                continue;
            }
            let f = (1.0 + self.age * self.age_pattern.res_dev[i]).max(1e-6);
            self.res[i].g = self.res_base_g[i] / f;
        }
        for i in 0..self.n_caps {
            let f = (1.0 + self.age * self.age_pattern.cap_dev[i]).max(1e-9);
            let c = self.cap_base_far[i] * f;
            self.caps[i].c = c;
            self.caps[i].geq = 2.0 * c / self.dt;
        }
        // Age the transformer: recompute the coupled-inductor companion from drifted winding L's,
        // and (with the JA core) retune the lump + trim the core voicing. `build_g0` re-stamps `gl`.
        let lp = self.aged_lp();
        let ls = self.aged_ls();
        self.recompute_gl(lp, ls, self.k_coupl_base);
        #[cfg(feature = "trafo-core")]
        self.apply_trafo_aged(lp);
        self.build_g0();
    }

    /// Re-roll the "unit": regenerate the per-component drift pattern from `seed` (using the per-leg
    /// materials and current grade), then re-apply at the current `age`. The live bias/rgain legs
    /// are refreshed by the plugin afterward.
    pub fn set_age_seed(&mut self, seed: u64) {
        self.age_seed = seed;
        self.age_pattern = AgePattern::from_seed(
            seed,
            self.n_res,
            self.n_caps,
            &self.res_material,
            &self.cap_material,
            self.tol_grade,
        );
        self.set_age(self.age);
    }

    /// Set the build tolerance grade (0=Ideal, 1=Standard, 2=Loose): regenerate the drift pattern at
    /// the current seed (band magnitude depends on grade), then re-apply at the current age. Mirrors
    /// `set_age_seed`. `Ideal` => zero drift (bit-exact with pristine at any age).
    pub fn set_tolerance_grade(&mut self, idx: i32) {
        self.tol_grade = TolGrade::from_index(idx);
        self.set_age_seed(self.age_seed);
    }

    /// Commit the JA core state for the next sample — the separable RK4 magnetizing filter (call
    /// once per sample after the network solve, when `ja_active`). The linear network solve already
    /// ran with the magnetizing branch represented by the *linear* companion; this advances the
    /// hysteretic magnetization `M` by integrating `dM/dB` with fixed-step RK4 over the flux
    /// increment `[B[n-1], B[n]]`, then computes the lagged *deviation* current
    /// `i_dev = trafo·(i_JA − i_lin)` that is injected into the next sample's RHS (decoupled from
    /// the Newton loop — no Jacobian contribution, no inner Newton). `delta` is frozen across the
    /// four RK4 stages and updated from the net field change at the end.
    #[cfg(feature = "trafo-core")]
    #[inline]
    pub(in crate::wdf) fn ja_commit(&mut self) {
        use node::{N_A, N_PA};
        let vm = self.node_v(N_PA) - self.node_v(N_A);
        // trapezoidal flux at the converged magnetizing voltage
        let phi = self.ja_phi_prev + 0.5 * self.dt * (vm + self.ja_vm_prev);
        let b0 = self.ja_phi_prev / self.ja.k_phi; // B[n-1]
        let b1 = phi / self.ja.k_phi; // B[n]
        let hb = b1 - b0;
        let m0 = self.ja_m;
        let d = self.ja_delta;
        // RK4 over [b0, b1] (4 fixed stages; H is algebraic at each stage so no inner Newton).
        let k1 = self.ja.dmdb(b0, m0, d);
        let k2 = self.ja.dmdb(b0 + 0.5 * hb, m0 + 0.5 * hb * k1, d);
        let k3 = self.ja.dmdb(b0 + 0.5 * hb, m0 + 0.5 * hb * k2, d);
        let k4 = self.ja.dmdb(b1, m0 + hb * k3, d);
        let m = m0 + (hb / 6.0) * (k1 + 2.0 * k2 + 2.0 * k3 + k4);
        let h = b1 / self.ja.mu0 - m; // algebraic field at the new state
        let dh = h - self.ja_h_prev;
        if dh != 0.0 {
            self.ja_delta = dh.signum();
        }
        // Lagged magnetizing-deviation current for the next sample (excess beyond the linear core,
        // so it vanishes at low drive → smooth from trafo=0⁺).
        let i_ja = h / self.ja.k_hi;
        let i_lin = phi / self.ja.lp_eff;
        self.ja_i_dev = JA_DEV_REF * (i_ja - i_lin);
        self.ja_m = m;
        self.ja_h_prev = h;
        self.ja_phi_prev = phi;
        self.ja_vm_prev = vm;
    }

    /// Reset the JA core to a clean origin: `M=0`, no flux/field/voltage history, `+δ` field-rate
    /// direction, no lagged deviation current. Single source of truth for "what a settled core looks
    /// like" — called on both edges that must not integrate the next trapezoidal step against a stale
    /// operating point: warmup→audio (`process_host_sample`) and inactive→active (`set_trafo_enabled`).
    #[cfg(feature = "trafo-core")]
    #[inline]
    pub(in crate::wdf) fn ja_reseed(&mut self) {
        self.ja_m = 0.0;
        self.ja_h_prev = 0.0;
        self.ja_phi_prev = 0.0;
        self.ja_vm_prev = 0.0;
        self.ja_delta = 1.0;
        self.ja_i_dev = 0.0;
    }

    /// Transformer ON/OFF switch. `false` => `ja_active` false => the magnetizing-deviation current
    /// `ja_i_dev` is forced to 0 => bit-exact linear core. The J-A is a separable RK4 post-solve
    /// filter injecting a lagged current source, so it touches neither G0 nor the Schur partition
    /// (N-set stays 6 always); this only flips the gate.
    ///
    /// Re-enable from a linear/inactive state: the core stopped committing when it went inactive, so
    /// `ja_vm_prev`/`ja_phi_prev`/`ja_m` are frozen at a stale operating point. Integrating the next
    /// trapezoidal flux step against that stale `ja_vm_prev` would jump the flux (audible click).
    /// Re-seed to a clean start on the inactive→active edge (same reset the warmup→audio transition
    /// does). No-op on a fresh model (state already zero) and when already active.
    #[cfg(feature = "trafo-core")]
    pub fn set_trafo_enabled(&mut self, on: bool) {
        if on && !self.ja_active {
            self.ja_reseed();
        }
        self.ja_active = on;
    }

    /// Set the transformer voicing from the two front-panel dials (HYSTERESIS, SATURATION; each
    /// 0..1, default 0.5 = today's sound). Re-voices `m_s, a, k, c` in place — cheap, no lump
    /// rebuild, no reseed (the magnetization state carries across so a dial glide doesn't click).
    /// Independent of the ON/OFF gate: voicing a disabled core is harmless (no deviation injected).
    #[cfg(feature = "trafo-core")]
    pub fn set_trafo_voicing(&mut self, hyst: f64, sat: f64) {
        // Cache the dials so an age change can re-voice from the same point, then apply with the
        // current per-unit aging composed in (bit-exact with `ja.set_voicing` at age=0).
        self.trafo_hyst = hyst.clamp(0.0, 1.0);
        self.trafo_sat = sat.clamp(0.0, 1.0);
        self.apply_trafo_aged(self.aged_lp());
    }

    /// Current core operating point `(H, B)` in SI units — for offline B-H loop logging. Inert
    /// (returns 0,0) when the core is linear/inactive.
    #[cfg(feature = "trafo-core")]
    pub fn ja_hb(&self) -> (f64, f64) {
        if !self.ja_active {
            return (0.0, 0.0);
        }
        let b = self.ja_phi_prev / self.ja.k_phi;
        (self.ja_h_prev, b)
    }

    /// Lagged magnetizing-deviation current `i_dev` queued for the next sample's RHS injection — the
    /// actual click source on a knob re-enable (a stale value steps the network current). For tests.
    #[cfg(feature = "trafo-core")]
    pub fn ja_i_dev(&self) -> f64 {
        self.ja_i_dev
    }

    /// TR3 quiescent collector current (mA) at the current operating point (`main.cpp:441-445`).
    pub fn tr3_idle_current_ma(&self) -> f64 {
        let t = &self.bjts[2];
        let vbe = self.node_v(t.b) - self.node_v(t.e);
        let vbc = self.node_v(t.b) - self.node_v(t.c);
        1000.0 * compute_bjt(vbe, vbc, &t.p).ic
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> WdfModel {
        WdfModel::new(48_000.0, 1000.0, 0.5)
    }

    #[test]
    fn age_zero_is_bitexact() {
        let pristine = fresh();
        let mut m = fresh();
        m.set_age_seed(0x00AB_CDEF); // Standard grade (model default)
        m.set_age(0.0);
        assert_eq!(m.gl, pristine.gl, "coupled-inductor companion");
        for i in 0..m.n_res {
            assert_eq!(m.res[i].g, pristine.res[i].g, "res {i}");
        }
        for i in 0..m.n_caps {
            assert_eq!(m.caps[i].c, pristine.caps[i].c, "cap {i}");
        }
    }

    #[test]
    fn ideal_grade_is_bitexact_at_full_age() {
        let pristine = fresh();
        let mut m = fresh();
        m.set_tolerance_grade(0); // Ideal => zero drift even at age = 1
        m.set_age(1.0);
        assert_eq!(m.gl, pristine.gl, "coupled-inductor companion");
        for i in 0..m.n_res {
            assert_eq!(m.res[i].g, pristine.res[i].g, "res {i}");
        }
        for i in 0..m.n_caps {
            assert_eq!(m.caps[i].c, pristine.caps[i].c, "cap {i}");
        }
    }

    #[test]
    fn transformer_inductor_ages_at_full_age() {
        let pristine = fresh();
        let mut m = fresh();
        m.set_age_seed(0x00AB_CDEF);
        m.set_age(1.0);
        assert_ne!(m.gl, pristine.gl, "coupled inductor should drift when aged");
    }

    #[cfg(feature = "trafo-core")]
    #[test]
    fn ja_lump_tracks_aged_inductance() {
        let pristine = fresh();
        let mut m = fresh();
        m.set_age_seed(0x00AB_CDEF);
        m.set_age(1.0);
        // lp_eff is re-pinned to the aged primary inductance...
        assert!((m.ja.lp_eff - m.aged_lp()).abs() < 1e-12);
        // ...and k_hi is re-back-solved, staying strictly proportional to lp_eff (the chi0 lump is
        // fixed), so the low-drive deviation keeps cancelling.
        let ratio_lp = m.ja.lp_eff / pristine.ja.lp_eff;
        let ratio_khi = m.ja.k_hi / pristine.ja.k_hi;
        assert!((ratio_lp - ratio_khi).abs() < 1e-9, "k_hi must track lp_eff");
        assert!((ratio_lp - 1.0).abs() > 1e-6, "primary inductance should actually drift");
    }

    #[cfg(feature = "trafo-core")]
    #[test]
    fn ja_core_voicing_shifts_when_aged() {
        let mut p = fresh();
        p.set_tolerance_grade(0); // Ideal => pristine voicing at age 1
        p.set_age(1.0);
        let mut m = fresh();
        m.set_age_seed(0x00AB_CDEF);
        m.set_age(1.0);
        let (ms0, k0, c0) = p.ja.core_msc();
        let (ms1, k1, c1) = m.ja.core_msc();
        assert!(
            (ms1 - ms0).abs() > 1e-3 || (k1 - k0).abs() > 1e-3 || (c1 - c0).abs() > 1e-6,
            "aged core voicing should differ from pristine: ({ms0},{k0},{c0}) vs ({ms1},{k1},{c1})"
        );
    }
}
