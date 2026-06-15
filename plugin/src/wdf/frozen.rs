use super::*;

impl WdfModel {
    /// True trapezoidal junction charges `(qbe, qbc)` for `t` at the current `v` (depletion +
    /// diffusion). Single source for the trailing-state update's recompute path AND its debug
    /// bit-exactness guard, so the two can't silently drift (cf. the duplication warning on the
    /// `depletion_charge_pre` golden test). Mirrors the per-BJT charge built in `newton_iterate`.
    #[inline]
    pub(in crate::wdf) fn junction_charges(&self, t: &Bjt) -> (f64, f64) {
        let tf = if self.cap_tier == CapTier::Full { t.p.tf } else { 0.0 };
        let tr = if self.cap_tier == CapTier::Full { t.p.tr } else { 0.0 };
        let vbe = self.node_v(t.b) - self.node_v(t.e);
        let vbc = self.node_v(t.b) - self.node_v(t.c);
        let (i_be, _) = diode_branch(vbe, t.p.is, t.p.vt, t.p.vcrit, t.p.cheap_exp);
        let (i_bc, _) = diode_branch(vbc, t.p.is, t.p.vt, t.p.vcrit, t.p.cheap_exp);
        let (qbe_dep, _) =
            depletion_charge_pre(vbe, t.p.cje, t.p.vje, t.p.mje, t.p.je_c0, t.p.je_q0, t.p.je_dcdv);
        let (qbc_dep, _) =
            depletion_charge_pre(vbc, t.p.cjc, t.p.vjc, t.p.mjc, t.p.jc_c0, t.p.jc_q0, t.p.jc_dcdv);
        (qbe_dep + tf * i_be, qbc_dep + tr * i_bc)
    }

    /// Sub-rate spike helper: add the linear branch residuals (resistors + linear-cap companions +
    /// coupled inductor) at the current `v` into the reduced residual `f`. Byte-for-byte the same
    /// stamps as `newton_iterate`'s linear block, but without touching `self.j` or the devices.
    #[cfg(feature = "subrate-spike")]
    fn add_linear_residual(&self, f: &mut [f64]) {
        use node::{N_A, N_PA, N_SA};
        for i in 0..self.n_res {
            let r = self.res[i];
            let d = r.g * (self.node_v(r.a) - self.node_v(r.b));
            let (ra, rb) = (self.ur(r.a), self.ur(r.b));
            if ra >= 0 {
                f[ra as usize] += d;
            }
            if rb >= 0 {
                f[rb as usize] -= d;
            }
        }
        for i in 0..self.n_caps {
            let c = self.caps[i];
            let d = c.geq * (self.node_v(c.a) - self.node_v(c.b));
            let (ra, rb) = (self.ur(c.a), self.ur(c.b));
            if ra >= 0 {
                f[ra as usize] += d;
            }
            if rb >= 0 {
                f[rb as usize] -= d;
            }
        }
        let vp = self.node_v(N_PA) - self.node_v(N_A);
        let vs = self.node_v(N_SA);
        let (rp, rs) = (self.ur(N_PA), self.ur(N_SA));
        if rp >= 0 {
            f[rp as usize] += self.gl[0][0] * vp + self.gl[0][1] * vs;
        }
        if rs >= 0 {
            f[rs as usize] += self.gl[1][0] * vp + self.gl[1][1] * vs;
        }
    }

    #[cfg(feature = "subrate-spike")]
    /// Sub-rate spike: after a converged full solve at `v0`, build the frozen linear model used by
    /// the next `subrate-1` skipped samples. `newton_iterate` already snapshot the converged J into
    /// `j_frozen_raw` (= G0 + J_dev(v0)). LU-factor it; derive `j_dev = j_frozen_raw - g0`; and
    /// capture the device Norton constant `i0_const = D(v0) - J_dev·v0`, computed from the converged
    /// identity `D(v0) = -L(v0)` (full residual ≈ 0 ⇒ device residual = −linear residual). On failure
    /// (no converged J this sample, or singular J) clears `subrate_valid` so the next sample also
    /// full-solves.
    fn capture_frozen(&mut self) {
        let u = self.u;
        if !self.j_captured {
            self.subrate_valid = false;
            return;
        }
        self.lu_frozen[..u * u].copy_from_slice(&self.j_frozen_raw[..u * u]);
        if !Self::lu_factor(&mut self.lu_frozen[..u * u], &mut self.piv_frozen[..u], u) {
            self.subrate_valid = false;
            return;
        }
        for i in 0..u * u {
            self.j_dev[i] = self.j_frozen_raw[i] - self.g0[i];
        }
        // Reduced converged voltages.
        let mut v0_red = [0.0f64; NUM_NODES];
        for n in 0..NUM_NODES {
            let r = self.urow[n];
            if r >= 0 {
                v0_red[r as usize] = self.v[n];
            }
        }
        // L(v0) = hist + linear residuals at v0 (self.hist still holds this sample's history).
        let mut lv = [0.0f64; NUM_NODES];
        lv[..u].copy_from_slice(&self.hist[..u]);
        self.add_linear_residual(&mut lv[..u]);
        // i0_const[r] = D(v0) - (J_dev·v0)[r] = -L(v0)[r] - (J_dev·v0)[r].
        for r in 0..u {
            let mut s = -lv[r];
            let base = r * u;
            for c in 0..u {
                s -= self.j_dev[base + c] * v0_red[c];
            }
            self.i0_const[r] = s;
        }
        self.subrate_valid = true;
    }

    #[cfg(feature = "subrate-spike")]
    /// Sub-rate spike: solve one skipped sample with the frozen model. The residual is affine in
    /// `v` — `F(v) = j_frozen·v + (hist + known-node const) + i0_const` — so one LU back-substitution
    /// against the pre-factored `lu_frozen` lands on the exact frozen-linear solution regardless of
    /// the starting guess. No `exp4` / depletion / Gummel-Poon, no re-factor.
    fn solve_frozen(&mut self) {
        let u = self.u;
        // F = hist + linear residuals + frozen device residual, evaluated at the current v.
        self.f[..u].copy_from_slice(&self.hist[..u]);
        // Borrow split: add_linear_residual reads &self, so stage into a temp then fold in.
        let mut lin = [0.0f64; NUM_NODES];
        self.add_linear_residual(&mut lin[..u]);
        for r in 0..u {
            self.f[r] += lin[r];
        }
        let mut v_red = [0.0f64; NUM_NODES];
        for n in 0..NUM_NODES {
            let r = self.urow[n];
            if r >= 0 {
                v_red[r as usize] = self.v[n];
            }
        }
        for r in 0..u {
            let mut d = self.i0_const[r];
            let base = r * u;
            for c in 0..u {
                d += self.j_dev[base + c] * v_red[c];
            }
            self.f[r] += d;
        }
        for r in 0..u {
            self.delta[r] = -self.f[r];
        }
        Self::lu_solve(&self.lu_frozen, &self.piv_frozen, u, &mut self.delta);
        for n in 0..NUM_NODES {
            let r = self.urow[n];
            if r >= 0 {
                self.v[n] += self.delta[r as usize];
            }
        }
    }
}
