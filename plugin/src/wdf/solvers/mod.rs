use super::*;

impl WdfModel {

    /// Residual 2-norm at current `v` (for line search / convergence), reusing per-sample `hist`.
    /// Must stamp the SAME branches as `newton_iterate`'s F for line-search consistency
    /// (`main.cpp:717-761`).
    pub(crate) fn residual_norm(&mut self, gmin: f64) -> f64 {
        use node::{N_A, N_PA, N_SA};
        let u = self.u;
        for i in 0..u {
            self.rn[i] = self.hist[i];
        }
        // resistors
        for i in 0..self.n_res {
            let r = self.res[i];
            let (va, vb) = (self.node_v(r.a), self.node_v(r.b));
            let (ra, rb) = (self.ur(r.a), self.ur(r.b));
            if ra >= 0 {
                self.rn[ra as usize] += r.g * (va - vb);
            }
            if rb >= 0 {
                self.rn[rb as usize] += r.g * (vb - va);
            }
        }
        // caps
        for i in 0..self.n_caps {
            let c = self.caps[i];
            let (va, vb) = (self.node_v(c.a), self.node_v(c.b));
            let (ra, rb) = (self.ur(c.a), self.ur(c.b));
            if ra >= 0 {
                self.rn[ra as usize] += c.geq * (va - vb);
            }
            if rb >= 0 {
                self.rn[rb as usize] += c.geq * (vb - va);
            }
        }
        // coupled inductor
        {
            let vp = self.node_v(N_PA) - self.node_v(N_A);
            let vs = self.node_v(N_SA);
            let (rp, rs) = (self.ur(N_PA), self.ur(N_SA));
            if rp >= 0 {
                self.rn[rp as usize] += self.gl[0][0] * vp + self.gl[0][1] * vs;
            }
            if rs >= 0 {
                self.rn[rs as usize] += self.gl[1][0] * vp + self.gl[1][1] * vs;
            }
        }
        // BJTs (transport + charge caps — must match newton_iterate's F)
        let k2 = 2.0 / self.dt;
        for bi in 0..self.bjts.len() {
            let t = self.bjts[bi];
            let vbe = self.node_v(t.b) - self.node_v(t.e);
            let vbc = self.node_v(t.b) - self.node_v(t.c);
            let o = compute_bjt(vbe, vbc, &t.p);
            let (rb, re, rc) = (self.ur(t.b), self.ur(t.e), self.ur(t.c));
            if rb >= 0 {
                self.rn[rb as usize] += o.ib;
            }
            if rc >= 0 {
                self.rn[rc as usize] += o.ic;
            }
            if re >= 0 {
                self.rn[re as usize] += o.ie;
            }
            if self.cap_tier != CapTier::Off {
                let tf = if self.cap_tier == CapTier::Full { t.p.tf } else { 0.0 };
                let tr = if self.cap_tier == CapTier::Full { t.p.tr } else { 0.0 };
                // Match newton_iterate's F: use the frozen companion when active (`main.cpp:960`).
                let (qbe, qbc) = if self.freeze_caps && self.frozen_caps_valid {
                    let fc = self.frozen_cap[bi];
                    (fc.2 + fc.0 * vbe, fc.3 + fc.1 * vbc)
                } else {
                    let (qbe_dep, _) =
                        depletion_charge_pre(vbe, t.p.cje, t.p.vje, t.p.mje, t.p.je_c0, t.p.je_q0, t.p.je_dcdv);
                    let (qbc_dep, _) =
                        depletion_charge_pre(vbc, t.p.cjc, t.p.vjc, t.p.mjc, t.p.jc_c0, t.p.jc_q0, t.p.jc_dcdv);
                    (qbe_dep + tf * o.itf, qbc_dep + tr * o.itr)
                };
                let ibe = k2 * (qbe - t.qbe_prev) - t.ibe_prev;
                let ibc = k2 * (qbc - t.qbc_prev) - t.ibc_prev;
                if rb >= 0 {
                    self.rn[rb as usize] += ibe + ibc;
                }
                if re >= 0 {
                    self.rn[re as usize] -= ibe;
                }
                if rc >= 0 {
                    self.rn[rc as usize] -= ibc;
                }
            }
        }
        if gmin != 0.0 {
            for i in 0..NUM_NODES {
                let r = self.ur(i as i32);
                if r >= 0 {
                    self.rn[r as usize] += gmin * self.v[i];
                }
            }
        }
        let mut n = 0.0;
        for i in 0..u {
            n += self.rn[i] * self.rn[i];
        }
        n.sqrt()
    }

    /// Dense Gaussian elimination with index-swap partial pivoting. Solves `A x = b` in place
    /// (`x` in `b`). `A` is accessed through the `row[]` permutation; `b` is swapped physically —
    /// the asymmetry is bit-identical to row-swap (`main.cpp:764-794`, commit 42b51ef).
    pub(crate) fn gauss_solve(a: &mut [f64], b: &mut [f64], n: usize) -> bool {
        let mut row = [0usize; NUM_NODES];
        for (i, slot) in row.iter_mut().enumerate().take(n) {
            *slot = i;
        }
        for col in 0..n {
            let mut piv = col;
            let mut best = a[row[col] * n + col].abs();
            for r in (col + 1)..n {
                let val = a[row[r] * n + col].abs();
                if val > best {
                    best = val;
                    piv = r;
                }
            }
            if best < 1e-30 {
                return false;
            }
            if piv != col {
                row.swap(col, piv);
                b.swap(col, piv);
            }
            let diag = a[row[col] * n + col];
            for r in (col + 1)..n {
                let f = a[row[r] * n + col] / diag;
                if f == 0.0 {
                    continue;
                }
                for c in col..n {
                    a[row[r] * n + c] -= f * a[row[col] * n + c];
                }
                b[r] -= f * b[col];
            }
        }
        for r in (0..n).rev() {
            let mut s = b[r];
            for c in (r + 1)..n {
                s -= a[row[r] * n + c] * b[c];
            }
            b[r] = s / a[row[r] * n + r];
        }
        true
    }

    /// Fixed-size (n=16) specialization of [`gauss_solve`] using PHYSICAL row swaps instead of
    /// `row[]` index indirection, so the elimination inner loop is contiguous (`a[col_base + c]`,
    /// `a[r_base + c]`) and LLVM can autovectorize it. Bit-identical to `gauss_solve(a, b, 16)`:
    /// same pivot selection (max-abs, `>` tie-break, scan `col+1..16`), same `f`, same operand
    /// order, same loop bounds (`col..16` / `r+1..16`), same `f == 0.0` short-circuit and
    /// `best < 1e-30` bail. Physical-swap vs index-swap differs only in memory address.
    pub(crate) fn gauss_solve_16(a: &mut [f64], b: &mut [f64]) -> bool {
        const N: usize = 16;
        for col in 0..N {
            // Pivot selection: max-abs in column `col`, scan col+1..N, strict `>` tie-break.
            let mut piv = col;
            let mut best = a[col * N + col].abs();
            for r in (col + 1)..N {
                let val = a[r * N + col].abs();
                if val > best {
                    best = val;
                    piv = r;
                }
            }
            if best < 1e-30 {
                return false;
            }
            if piv != col {
                // PHYSICAL swap of the two 16-element A rows AND the b entries.
                for c in 0..N {
                    a.swap(col * N + c, piv * N + c);
                }
                b.swap(col, piv);
            }
            let col_base = col * N;
            let diag = a[col_base + col];
            for r in (col + 1)..N {
                let r_base = r * N;
                let f = a[r_base + col] / diag;
                if f == 0.0 {
                    continue;
                }
                // Contiguous: matches `gauss_solve`'s `c in col..n`. Vectorizable.
                for c in col..N {
                    a[r_base + c] -= f * a[col_base + c];
                }
                b[r] -= f * b[col];
            }
        }
        // Back-substitution: SCALAR reduction, same summation order as `gauss_solve`.
        for r in (0..N).rev() {
            let r_base = r * N;
            let mut s = b[r];
            for c in (r + 1)..N {
                s -= a[r_base + c] * b[c];
            }
            b[r] = s / a[r_base + r];
        }
        true
    }

    /// Fixed-size (n=6) specialization of [`gauss_solve`] using PHYSICAL row swaps instead of
    /// `row[]` index indirection, so the elimination inner loop is contiguous (`a[col_base + c]`,
    /// `a[r_base + c]`). Bit-identical to `gauss_solve(a, b, 6)`: same pivot selection (max-abs,
    /// `>` tie-break, scan `col+1..6`), same `f`, same operand order, same loop bounds
    /// (`col..6` / `r+1..6`), same `f == 0.0` short-circuit and `best < 1e-30` bail. Physical-swap
    /// vs index-swap differs only in memory address. Used by `schur_solve` for the n=6 complement.
    pub(crate) fn gauss_solve_6(a: &mut [f64], b: &mut [f64]) -> bool {
        const N: usize = 6;
        for col in 0..N {
            // Pivot selection: max-abs in column `col`, scan col+1..N, strict `>` tie-break.
            let mut piv = col;
            let mut best = a[col * N + col].abs();
            for r in (col + 1)..N {
                let val = a[r * N + col].abs();
                if val > best {
                    best = val;
                    piv = r;
                }
            }
            if best < 1e-30 {
                return false;
            }
            if piv != col {
                // PHYSICAL swap of the two 6-element A rows AND the b entries.
                for c in 0..N {
                    a.swap(col * N + c, piv * N + c);
                }
                b.swap(col, piv);
            }
            let col_base = col * N;
            let diag = a[col_base + col];
            for r in (col + 1)..N {
                let r_base = r * N;
                let f = a[r_base + col] / diag;
                if f == 0.0 {
                    continue;
                }
                // Contiguous: matches `gauss_solve`'s `c in col..n`.
                for c in col..N {
                    a[r_base + c] -= f * a[col_base + c];
                }
                b[r] -= f * b[col];
            }
        }
        // Back-substitution: SCALAR reduction, same summation order as `gauss_solve`.
        for r in (0..N).rev() {
            let r_base = r * N;
            let mut s = b[r];
            for c in (r + 1)..N {
                s -= a[r_base + c] * b[c];
            }
            b[r] = s / a[r_base + r];
        }
        true
    }

    /// Reusable LU (partial pivot, index-swap) for the constant per-sample A_LL block: factor
    /// once, apply many times. Stores L/U in-place (row-permuted) + the pivot order so any RHS
    /// can be solved without refactoring (`main.cpp:1026-1047`).
    pub(crate) fn lu_factor(a: &mut [f64], piv: &mut [usize], n: usize) -> bool {
        for (i, slot) in piv.iter_mut().enumerate().take(n) {
            *slot = i;
        }
        for col in 0..n {
            let mut p = col;
            let mut best = a[piv[col] * n + col].abs();
            for r in (col + 1)..n {
                let val = a[piv[r] * n + col].abs();
                if val > best {
                    best = val;
                    p = r;
                }
            }
            if best < 1e-30 {
                return false;
            }
            if p != col {
                piv.swap(col, p);
            }
            let diag = a[piv[col] * n + col];
            for r in (col + 1)..n {
                let f = a[piv[r] * n + col] / diag;
                a[piv[r] * n + col] = f; // store multiplier (L factor)
                if f == 0.0 {
                    continue;
                }
                for c in (col + 1)..n {
                    a[piv[r] * n + c] -= f * a[piv[col] * n + c];
                }
            }
        }
        true
    }

    /// Solve `A x = b` in place (`x` in `b`) using LU factors + pivot order from [`lu_factor`]
    /// (`main.cpp:1049-1062`). `y` scratch is a stack array (no heap alloc).
    pub(crate) fn lu_solve(a: &[f64], piv: &[usize], n: usize, b: &mut [f64]) {
        let mut y = [0.0f64; NUM_NODES];
        for r in 0..n {
            let mut s = b[piv[r]];
            for c in 0..r {
                s -= a[piv[r] * n + c] * y[c];
            }
            y[r] = s; // unit-diagonal L
        }
        for r in (0..n).rev() {
            let mut s = y[r];
            for c in (r + 1)..n {
                s -= a[piv[r] * n + c] * b[c];
            }
            b[r] = s / a[piv[r] * n + r];
        }
    }

    /// Fixed-size (n=10) specialization of [`lu_factor`] using PHYSICAL row swaps instead of
    /// `piv[]` index indirection. Records the row permutation in `perm` (output position -> original
    /// row index), exactly mirroring the generic `piv[]` semantics, so [`lu_solve_10`] can permute
    /// the RHS while reading the contiguous (`a[r*10+c]`) factor storage. Bit-identical to
    /// `lu_factor(a, piv, 10)`: same pivot selection (max-abs in column `col`, scan `col+1..10`,
    /// strict `>` tie-break), same `f`, same operand order, same loop bounds (`col+1..10`), same
    /// unit-diagonal-L multiplier storage, same `f == 0.0` short-circuit, same `best < 1e-30` bail.
    /// Physical-swap vs index-swap differs only in memory address.
    pub(crate) fn lu_factor_10(a: &mut [f64], perm: &mut [usize]) -> bool {
        const N: usize = 10;
        for (i, slot) in perm.iter_mut().enumerate().take(N) {
            *slot = i;
        }
        for col in 0..N {
            // Pivot selection over physical rows col..N (identity-permuted, matches generic order).
            let mut p = col;
            let mut best = a[col * N + col].abs();
            for r in (col + 1)..N {
                let val = a[r * N + col].abs();
                if val > best {
                    best = val;
                    p = r;
                }
            }
            if best < 1e-30 {
                return false;
            }
            if p != col {
                // PHYSICAL swap of the two 10-element rows; record in perm for the RHS.
                for c in 0..N {
                    a.swap(col * N + c, p * N + c);
                }
                perm.swap(col, p);
            }
            let col_base = col * N;
            let diag = a[col_base + col];
            for r in (col + 1)..N {
                let r_base = r * N;
                let f = a[r_base + col] / diag;
                a[r_base + col] = f; // store multiplier (L factor)
                if f == 0.0 {
                    continue;
                }
                for c in (col + 1)..N {
                    a[r_base + c] -= f * a[col_base + c];
                }
            }
        }
        true
    }

    /// Solve `A x = b` in place (`x` in `b`) using the contiguous (physical-swap) LU factors +
    /// `perm` row permutation from [`lu_factor_10`]. Bit-identical to `lu_solve(a, piv, 10, b)`:
    /// the RHS read `b[perm[r]]` matches `b[piv[r]]`, factor reads are the same values in natural
    /// row order, forward/back substitution are SCALAR reductions in the same summation order.
    pub(crate) fn lu_solve_10(a: &[f64], perm: &[usize], b: &mut [f64]) {
        const N: usize = 10;
        let mut y = [0.0f64; NUM_NODES];
        for r in 0..N {
            let r_base = r * N;
            let mut s = b[perm[r]];
            for c in 0..r {
                s -= a[r_base + c] * y[c];
            }
            y[r] = s; // unit-diagonal L
        }
        for r in (0..N).rev() {
            let r_base = r * N;
            let mut s = y[r];
            for c in (r + 1)..N {
                s -= a[r_base + c] * b[c];
            }
            b[r] = s / a[r_base + r];
        }
    }
}

#[cfg(test)]
mod tests;
