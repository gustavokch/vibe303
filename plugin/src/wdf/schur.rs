use super::*;

impl WdfModel {

    /// Build the Schur N/L partition once (at construction). N = every unknown row that a BJT
    /// terminal (and thus its charge cap) stamps; L = the remaining linear-only unknowns. Derived
    /// from the actual `bjts[]` terminal ids so it always matches the stamping (`main.cpp:483-527`).
    pub(in crate::wdf) fn build_schur_partition(&mut self) {
        let mut in_n = [false; NUM_NODES];
        for t in self.bjts.iter() {
            for nd in [t.b, t.e, t.c] {
                if nd >= 0 && self.urow[nd as usize] >= 0 {
                    in_n[nd as usize] = true;
                }
            }
        }
        // JA core is decoupled (separable RK4 post-solve filter → lagged current source), so it adds
        // nothing to the nonlinear N-set: the partition is identical with or without `trafo-core`.
        self.n_sz = 0;
        for nd in 0..NUM_NODES {
            if !in_n[nd] {
                continue;
            }
            let r = self.urow[nd];
            if r < 0 {
                continue;
            }
            self.n_idx[self.n_sz] = r as usize;
            self.n_sz += 1;
        }
        let mut is_n_row = [false; NUM_NODES];
        for i in 0..self.n_sz {
            is_n_row[self.n_idx[i]] = true;
        }
        self.l_sz = 0;
        for r in 0..self.u {
            if is_n_row[r] {
                continue;
            }
            self.l_idx[self.l_sz] = r;
            self.l_sz += 1;
        }
        self.schur_ready = false;
        // First-time prep (build_with's initial build_g0 ran before the partition existed).
        if self.solver == Solver::Schur {
            self.prepare_schur();
        }
    }

    /// Per-sample Schur setup (valid only on the `gmin == 0` path). Slices the CONSTANT linear
    /// blocks out of G0, LU-factors A_LL, and precomputes `W = A_LL^{-1} A_LN` and
    /// `S_const = A_NN_lin - A_NL W`. Sets `schur_ready`; returns false (→ dense fallback) if
    /// A_LL is singular (`main.cpp:533-568`).
    pub(in crate::wdf) fn prepare_schur(&mut self) -> bool {
        self.schur_ready = false;
        let u = self.u;
        let n = self.n_sz;
        let l = self.l_sz;
        for i in 0..n {
            for j in 0..n {
                self.s_annlin[i * n + j] = self.g0[self.n_idx[i] * u + self.n_idx[j]];
            }
        }
        for i in 0..n {
            for j in 0..l {
                self.s_anl[i * l + j] = self.g0[self.n_idx[i] * u + self.l_idx[j]];
            }
        }
        for i in 0..l {
            for j in 0..n {
                self.s_aln[i * n + j] = self.g0[self.l_idx[i] * u + self.n_idx[j]];
            }
        }
        for i in 0..l {
            for j in 0..l {
                self.s_ll_fac[i * l + j] = self.g0[self.l_idx[i] * u + self.l_idx[j]];
            }
        }
        // Use the contiguous physical-swap LU pair when l == 10 (fast tier's A_LL block);
        // generic index-swap pair otherwise. Both are bit-identical; only storage layout differs.
        let factored = if l == 10 {
            Self::lu_factor_10(&mut self.s_ll_fac, &mut self.s_ll_piv)
        } else {
            Self::lu_factor(&mut self.s_ll_fac, &mut self.s_ll_piv, l)
        };
        if !factored {
            return false;
        }
        // W = A_LL^{-1} A_LN  (solve A_LL * W[:,c] = A_LN[:,c] for each N column)
        let mut col = [0.0f64; NUM_NODES];
        for c in 0..n {
            for i in 0..l {
                col[i] = self.s_aln[i * n + c];
            }
            if l == 10 {
                Self::lu_solve_10(&self.s_ll_fac, &self.s_ll_piv, &mut col);
            } else {
                Self::lu_solve(&self.s_ll_fac, &self.s_ll_piv, l, &mut col);
            }
            for i in 0..l {
                self.s_w[i * n + c] = col[i];
            }
        }
        // S_const = A_NN_lin - A_NL W
        for i in 0..n {
            for j in 0..n {
                let mut acc = 0.0;
                for k in 0..l {
                    acc += self.s_anl[i * l + k] * self.s_w[k * n + j];
                }
                self.s_sconst[i * n + j] = self.s_annlin[i * n + j] - acc;
            }
        }
        self.schur_ready = true;
        true
    }

    /// Schur static-block solve of `J delta = -F`, reusing the per-sample A_LL factorization +
    /// `S_const`. Reads the freshly built full `J` (`self.j`) and the RHS `-F` already scattered
    /// into `self.delta`, and writes the solution back into `self.delta`. Only the NxN complement
    /// is factored per iter (full partial pivot via [`gauss_solve`]). Caller guarantees
    /// `gmin == 0` and `schur_ready`. Returns false if the complement is singular (→ dense
    /// fallback; `self.delta` is left as `-F`, untouched) (`main.cpp:1069-1104`).
    pub(in crate::wdf) fn schur_solve(&mut self) -> bool {
        let u = self.u;
        let n = self.n_sz;
        let l = self.l_sz;
        let mut b_n = [0.0f64; NUM_NODES];
        let mut y = [0.0f64; NUM_NODES];
        for i in 0..n {
            b_n[i] = self.delta[self.n_idx[i]];
        }
        for i in 0..l {
            y[i] = self.delta[self.l_idx[i]]; // y starts as bL
        }
        // S = S_const + (J_NN - A_NN_lin)  (only the nonlinear block changes per iter)
        for i in 0..n {
            let gi = self.n_idx[i];
            for j in 0..n {
                let jnn = self.j[gi * u + self.n_idx[j]];
                self.s_s[i * n + j] = self.s_sconst[i * n + j] + (jnn - self.s_annlin[i * n + j]);
            }
        }
        // y = A_LL^{-1} bL (reuse per-sample factorization)
        if l == 10 {
            Self::lu_solve_10(&self.s_ll_fac, &self.s_ll_piv, &mut y);
        } else {
            Self::lu_solve(&self.s_ll_fac, &self.s_ll_piv, l, &mut y);
        }
        // b~N = bN - A_NL y
        let mut x_n = [0.0f64; NUM_NODES];
        for i in 0..n {
            let mut acc = 0.0;
            for k in 0..l {
                acc += self.s_anl[i * l + k] * y[k];
            }
            x_n[i] = b_n[i] - acc;
        }
        // solve S xN = b~N with full partial pivoting (S can be ill-conditioned)
        let solved = if n == 6 {
            Self::gauss_solve_6(&mut self.s_s, &mut x_n)
        } else {
            Self::gauss_solve(&mut self.s_s, &mut x_n, n)
        };
        if !solved {
            return false;
        }
        // xL = y - W xN
        for i in 0..l {
            let mut acc = 0.0;
            for k in 0..n {
                acc += self.s_w[i * n + k] * x_n[k];
            }
            y[i] -= acc;
        }
        // scatter back into delta
        for i in 0..n {
            self.delta[self.n_idx[i]] = x_n[i];
        }
        for i in 0..l {
            self.delta[self.l_idx[i]] = y[i];
        }
        true
    }
}
