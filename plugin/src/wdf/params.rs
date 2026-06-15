use super::*;

impl WdfModel {

    /// Update the RV1 split (bias pot) + rebuild G0, preserving all integrator state
    /// (`main.cpp:432-438`). Composes the aged drift factor on each leg (see `set_age`).
    pub fn set_bias(&mut self, bias: f64) {
        let (r_upper, r_lower) = rv1_split(bias);
        self.res[self.r_upper_idx].g = 1.0 / (r_upper * self.age_factor_res(self.r_upper_idx));
        self.res[self.r_lower_idx].g = 1.0 / (r_lower * self.age_factor_res(self.r_lower_idx));
        self.build_g0();
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
        self.build_g0();
    }

    /// Re-roll the "unit": regenerate the per-component drift pattern from `seed`, then re-apply at
    /// the current `age`. The live bias/rgain legs are refreshed by the plugin afterward.
    pub fn set_age_seed(&mut self, seed: u64) {
        self.age_seed = seed;
        self.age_pattern =
            AgePattern::from_seed(seed, self.n_res, self.n_caps, &self.cap_base_far);
        self.set_age(self.age);
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
        self.ja_i_dev = self.trafo * (i_ja - i_lin);
        self.ja_m = m;
        self.ja_h_prev = h;
        self.ja_phi_prev = phi;
        self.ja_vm_prev = vm;
    }

    /// Set the `trafo` knob (0..1). 0 => `ja_active` false => the magnetizing-deviation current
    /// `ja_i_dev` is forced to 0 => bit-exact linear core. The J-A is a separable RK4 post-solve
    /// filter injecting a lagged current source, so it touches neither G0 nor the Schur partition
    /// (N-set stays 6 always); `set_trafo` only flips the gate.
    /// Reset the JA core to a clean origin: `M=0`, no flux/field/voltage history, `+δ` field-rate
    /// direction, no lagged deviation current. Single source of truth for "what a settled core looks
    /// like" — called on both edges that must not integrate the next trapezoidal step against a stale
    /// operating point: warmup→audio (`process_host_sample`) and inactive→active (`set_trafo`).
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

    #[cfg(feature = "trafo-core")]
    pub fn set_trafo(&mut self, trafo: f64) {
        self.trafo = trafo.clamp(0.0, 1.0);
        let active = self.trafo > 0.0;
        // Re-enable from a linear/inactive state: the core stopped committing when it went inactive,
        // so `ja_vm_prev`/`ja_phi_prev`/`ja_m` are frozen at a stale operating point. Integrating the
        // next trapezoidal flux step against that stale `ja_vm_prev` would jump the flux (audible
        // click). Re-seed to a clean start on the inactive→active edge (same reset the warmup→audio
        // transition does). No-op on a fresh model (state already zero) and on level changes while
        // already active.
        if active && !self.ja_active {
            self.ja_reseed();
        }
        self.ja_active = active;
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
