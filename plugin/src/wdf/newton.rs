use super::*;

impl WdfModel {

    /// One Newton solve (up to `max_iters`) at the given `gmin` regularization. Returns iters used.
    /// Honors the active levers: `ls_trim` (deferred line search), `freeze_caps` (frozen iter-0
    /// cap companion), and `solver` (dense vs Schur) (`main.cpp:701-927`).
    pub(in crate::wdf) fn newton_iterate(&mut self, gmin: f64) -> usize {
        use node::{N_A, N_PA, N_SA};
        let u = self.u;
        let mut iters_used = 0;

        // --freeze-caps: reset per newton_iterate call (NOT once per sample), so each
        // gmin-stepping homotopy pass freezes its cap companion at ITS own iter-0 operating
        // point rather than reusing a stale one from the gmin=0 attempt (`main.cpp:712`).
        self.frozen_caps_valid = false;
        // Trailing-state reuse: invalidate per call; only the convergence break re-arms it. After the
        // gmin homotopy the last (gmin=0) call's break leaves the cache at the returned `v`.
        self.cap_cache_valid = false;

        // Deferred-acceptance line-search state (only used when ls_trim). Instead of verifying
        // each Newton step with its own residual_norm call, take the full step optimistically and
        // let the NEXT iter's mandatory F build double as verification; if the residual rose,
        // retroactively restore and run a real backtracking line search (`main.cpp:714-723`).
        // Fixed-step (Med/Low precision): never trim, never run the per-step `residual_norm` line
        // search; take the bounded full Newton step each iter (the cheap `norm_f` tol-break still
        // short-circuits a converged sample). Lossy on transients; gated by the harness.
        let fixed = self.fixed_step;
        let use_trim = self.ls_trim && !fixed;
        let mut prev_norm_f = f64::INFINITY; // guarded by `provisional`, never read on iter 0
        let mut provisional = false;
        let mut v_before_step = [0.0f64; NUM_NODES];
        let mut last_delta = [0.0f64; NUM_NODES]; // first `u` entries valid

        let mut iter = 0;
        while iter < self.max_iters {
            iters_used = iter + 1;
            // J starts as the constant linear conductance matrix; F starts at history.
            self.j[..u * u].copy_from_slice(&self.g0[..u * u]);
            self.f[..u].copy_from_slice(&self.hist[..u]);

            // linear branch residuals into F only (their J entries are already in G0)
            for i in 0..self.n_res {
                let r = self.res[i];
                let d = r.g * (self.node_v(r.a) - self.node_v(r.b));
                let (ra, rb) = (self.ur(r.a), self.ur(r.b));
                if ra >= 0 {
                    self.f[ra as usize] += d;
                }
                if rb >= 0 {
                    self.f[rb as usize] -= d;
                }
            }
            for i in 0..self.n_caps {
                let c = self.caps[i];
                let d = c.geq * (self.node_v(c.a) - self.node_v(c.b));
                let (ra, rb) = (self.ur(c.a), self.ur(c.b));
                if ra >= 0 {
                    self.f[ra as usize] += d;
                }
                if rb >= 0 {
                    self.f[rb as usize] -= d;
                }
            }
            {
                // coupled inductor residual (F only)
                let vp = self.node_v(N_PA) - self.node_v(N_A);
                let vs = self.node_v(N_SA);
                let (rp, rs) = (self.ur(N_PA), self.ur(N_SA));
                if rp >= 0 {
                    self.f[rp as usize] += self.gl[0][0] * vp + self.gl[0][1] * vs;
                }
                if rs >= 0 {
                    self.f[rs as usize] += self.gl[1][0] * vp + self.gl[1][1] * vs;
                }
            }

            // BJTs (Gummel-Poon transport form). Currents flow INTO the device.
            // Gather → batched device eval (SLP-vectorized) → scalar scatter.
            let k2 = 2.0 / self.dt;

            // Gather vbe/vbc for 3 BJTs into [f64;4] SoA (lane 3 = padding=0).
            let mut vbe4 = [0.0_f64; 4];
            let mut vbc4 = [0.0_f64; 4];
            for bi in 0..3 {
                let t = &self.bjts[bi];
                vbe4[bi] = self.node_v(t.b) - self.node_v(t.e);
                vbc4[bi] = self.node_v(t.b) - self.node_v(t.c);
            }

            // Pre-batch exp for 3 BJTs. SoA inv_vt4/vcrit4 give stride-8 loads so
            // LLVM can emit vmovupd + vmulpd, then SLP-fuse the 3 independent exp_bl
            // polynomial chains into AVX2 YMM ops (3 exp calls → 1 exp4 per array).
            // Results (ef4/er4/ec4) are 3 × 32-byte locals — no large intermediate struct.
            let b = &self.bjts;
            let inv_vt4 = [1.0/b[0].p.vt, 1.0/b[1].p.vt, 1.0/b[2].p.vt, 0.0];
            let vcrit4  = [b[0].p.vcrit,   b[1].p.vcrit,   b[2].p.vcrit,   0.0];
            // T3: FMA contraction on fast/ultra (lossy); realtime keeps the bit-identical a*b+c path.
            let fma_on = self.fma;
            let (ef4, er4, ec4) = if fma_on {
                (exp4::<true>(&vbe4, &inv_vt4), exp4::<true>(&vbc4, &inv_vt4), exp4::<true>(&vcrit4, &inv_vt4))
            } else {
                (exp4::<false>(&vbe4, &inv_vt4), exp4::<false>(&vbc4, &inv_vt4), exp4::<false>(&vcrit4, &inv_vt4))
            };

            // Batch depletion charge — only when caps enabled and not using frozen companion.
            let use_frozen_dep = self.freeze_caps && self.frozen_caps_valid;
            let run_dep = self.cap_tier != CapTier::Off && !use_frozen_dep;
            let (qbe_dep4, cbe_dep4, qbc_dep4, cbc_dep4) = if run_dep {
                let cje4 = [b[0].p.cje, b[1].p.cje, b[2].p.cje, 0.0];
                let vje4 = [b[0].p.vje, b[1].p.vje, b[2].p.vje, 0.75];
                let mje4 = [b[0].p.mje, b[1].p.mje, b[2].p.mje, 0.33];
                let je_c04 = [b[0].p.je_c0, b[1].p.je_c0, b[2].p.je_c0, 0.0];
                let je_q04 = [b[0].p.je_q0, b[1].p.je_q0, b[2].p.je_q0, 0.0];
                let je_dcdv4 = [b[0].p.je_dcdv, b[1].p.je_dcdv, b[2].p.je_dcdv, 0.0];
                let cjc4 = [b[0].p.cjc, b[1].p.cjc, b[2].p.cjc, 0.0];
                let vjc4 = [b[0].p.vjc, b[1].p.vjc, b[2].p.vjc, 0.75];
                let mjc4 = [b[0].p.mjc, b[1].p.mjc, b[2].p.mjc, 0.33];
                let jc_c04 = [b[0].p.jc_c0, b[1].p.jc_c0, b[2].p.jc_c0, 0.0];
                let jc_q04 = [b[0].p.jc_q0, b[1].p.jc_q0, b[2].p.jc_q0, 0.0];
                let jc_dcdv4 = [b[0].p.jc_dcdv, b[1].p.jc_dcdv, b[2].p.jc_dcdv, 0.0];
                let ((qbe4, cbe4), (qbc4, cbc4)) = if self.dep_lut {
                    // Lossy LUT path (fast/ultra/Medium). Lanes 0..2 = BJTs, lane 3 = padding
                    // (cje4[3]/cjc4[3] = 0 → guarded, the table ref is never sampled). `m` arrays
                    // are unused here — the table subsumes the `pow(f, ±m)` evaluation.
                    // Lane 3's table is a dummy (reuses index 0); correctness relies on the
                    // padding lane being guarded off by `cj0 <= 0`. Pin that invariant.
                    debug_assert!(
                        cje4[3] <= 0.0 && cjc4[3] <= 0.0,
                        "lane 3 must be guarded padding (cj0 <= 0); else its dummy LUT is sampled"
                    );
                    let dl = &self.dep_luts;
                    let be_luts = [&dl.be[0], &dl.be[1], &dl.be[2], &dl.be[0]];
                    let bc_luts = [&dl.bc[0], &dl.bc[1], &dl.bc[2], &dl.bc[0]];
                    (depletion_charge_pre_batch_lut(vbe4, cje4, vje4, je_c04, je_q04, je_dcdv4, be_luts),
                     depletion_charge_pre_batch_lut(vbc4, cjc4, vjc4, jc_c04, jc_q04, jc_dcdv4, bc_luts))
                } else if fma_on {
                    (depletion_charge_pre_batch::<true>(vbe4, cje4, vje4, mje4, je_c04, je_q04, je_dcdv4),
                     depletion_charge_pre_batch::<true>(vbc4, cjc4, vjc4, mjc4, jc_c04, jc_q04, jc_dcdv4))
                } else {
                    (depletion_charge_pre_batch::<false>(vbe4, cje4, vje4, mje4, je_c04, je_q04, je_dcdv4),
                     depletion_charge_pre_batch::<false>(vbc4, cjc4, vjc4, mjc4, jc_c04, jc_q04, jc_dcdv4))
                };
                (qbe4, cbe4, qbc4, cbc4)
            } else {
                ([0.0_f64; 4], [0.0_f64; 4], [0.0_f64; 4], [0.0_f64; 4])
            };

            // Post-exp Gummel-Poon algebra: batch the divide-/sqrt-heavy 16-field Jacobian for
            // all 3 BJTs into one width-4 SoA call (SLP → packed vdivpd/vsqrtpd), reusing the
            // already-batched ef4/er4/ec4. The scatter tail stays scalar (AoS stamps).
            // In-solve ADAA1 spike (`adaa-insolve`): antialias the transport exponentials by their
            // first-order antiderivative divided difference. `vₙ₋₁` is the previous sample's converged
            // junction voltage, rebuilt from the SAME node defs the forward stamp uses
            // (`node_v_prev(b) − node_v_prev(e/c)`) so it references the same junction even when the
            // intrinsic-base node is pinned. Only the `vbe < vcrit` exp regime is antialiased; if a
            // junction (or its prev) sits in the linearized overflow arm, fall back to the
            // instantaneous value+derivative (already affine there → nothing to antialias).
            #[cfg(feature = "adaa-insolve")]
            let bout = if self.adaa {
                let mut ef_eff4 = [0.0_f64; 4];
                let mut er_eff4 = [0.0_f64; 4];
                let mut gef4 = [0.0_f64; 4];
                let mut ger4 = [0.0_f64; 4];
                for bi in 0..3 {
                    let t = &self.bjts[bi];
                    let vt = t.p.vt;
                    let vcrit = t.p.vcrit;
                    let vbe_p = self.node_v_prev(t.b) - self.node_v_prev(t.e);
                    let vbc_p = self.node_v_prev(t.b) - self.node_v_prev(t.c);
                    if vbe4[bi] < vcrit && vbe_p < vcrit {
                        let ef_p = (vbe_p / vt).exp();
                        let (e, g) = adaa1_exp(ef4[bi], ef_p, vbe4[bi], vbe_p, vt);
                        ef_eff4[bi] = e;
                        gef4[bi] = g;
                    } else {
                        ef_eff4[bi] = ef4[bi];
                        gef4[bi] = ef4[bi] / vt; // instantaneous: matches is*(ef-1), is*(ef/vt)
                    }
                    if vbc4[bi] < vcrit && vbc_p < vcrit {
                        let er_p = (vbc_p / vt).exp();
                        let (e, g) = adaa1_exp(er4[bi], er_p, vbc4[bi], vbc_p, vt);
                        er_eff4[bi] = e;
                        ger4[bi] = g;
                    } else {
                        er_eff4[bi] = er4[bi];
                        ger4[bi] = er4[bi] / vt;
                    }
                }
                compute_bjt_preexp_batch_adaa(&vbe4, &vbc4, b, &ef_eff4, &er_eff4, &gef4, &ger4, &ec4)
            } else if fma_on {
                compute_bjt_preexp_batch::<true>(&vbe4, &vbc4, b, &ef4, &er4, &ec4)
            } else {
                compute_bjt_preexp_batch::<false>(&vbe4, &vbc4, b, &ef4, &er4, &ec4)
            };
            #[cfg(not(feature = "adaa-insolve"))]
            let bout = if fma_on {
                compute_bjt_preexp_batch::<true>(&vbe4, &vbc4, b, &ef4, &er4, &ec4)
            } else {
                compute_bjt_preexp_batch::<false>(&vbe4, &vbc4, b, &ef4, &er4, &ec4)
            };
            for bi in 0..3 {
                let t = self.bjts[bi];
                let vbe = vbe4[bi];
                let vbc = vbc4[bi];
                let o = BjtOut {
                    ib: bout.ib[bi], ic: bout.ic[bi], ie: bout.ie[bi],
                    itf: bout.itf[bi], gtf: bout.gtf[bi], itr: bout.itr[bi], gtr: bout.gtr[bi],
                    dib_b: bout.dib_b[bi], dib_e: bout.dib_e[bi], dib_c: bout.dib_c[bi],
                    dic_b: bout.dic_b[bi], dic_e: bout.dic_e[bi], dic_c: bout.dic_c[bi],
                    die_b: bout.die_b[bi], die_e: bout.die_e[bi], die_c: bout.die_c[bi],
                };
                let (rb, re, rc) = (self.urow[t.b as usize], self.urow[t.e as usize], self.urow[t.c as usize]);
                if rb >= 0 {
                    let rb = rb as usize;
                    self.f[rb] += o.ib;
                    self.j[rb * u + rb] += o.dib_b;
                    if re >= 0 {
                        self.j[rb * u + re as usize] += o.dib_e;
                    }
                    if rc >= 0 {
                        self.j[rb * u + rc as usize] += o.dib_c;
                    }
                }
                if re >= 0 {
                    let re = re as usize;
                    self.f[re] += o.ie;
                    if rb >= 0 {
                        self.j[re * u + rb as usize] += o.die_b;
                    }
                    self.j[re * u + re] += o.die_e;
                    if rc >= 0 {
                        self.j[re * u + rc as usize] += o.die_c;
                    }
                }
                if rc >= 0 {
                    let rc = rc as usize;
                    self.f[rc] += o.ic;
                    if rb >= 0 {
                        self.j[rc * u + rb as usize] += o.dic_b;
                    }
                    if re >= 0 {
                        self.j[rc * u + re as usize] += o.dic_e;
                    }
                    self.j[rc * u + rc] += o.dic_c;
                }

                // Charge-storage caps (depletion + diffusion), trapezoidal companion.
                if self.cap_tier != CapTier::Off {
                    let tf = if self.cap_tier == CapTier::Full { t.p.tf } else { 0.0 };
                    let tr = if self.cap_tier == CapTier::Full { t.p.tr } else { 0.0 };
                    // --freeze-caps: iters>=1 reuse the frozen linear-cap companion (Q = off +
                    // C*v) with the iter-0 C — no depletion_charge/POW. Iter 0 computes the full
                    // depletion charge and stores the companion (`main.cpp:782-798`).
                    let (qbe, cbe, qbc, cbc) = if use_frozen_dep {
                        let fc = self.frozen_cap[bi];
                        (fc.2 + fc.0 * vbe, fc.0, fc.3 + fc.1 * vbc, fc.1)
                    } else {
                        let qbe = qbe_dep4[bi] + tf * o.itf;
                        let cbe = cbe_dep4[bi] + tf * o.gtf;
                        let qbc = qbc_dep4[bi] + tr * o.itr;
                        let cbc = cbc_dep4[bi] + tr * o.gtr;
                        if self.freeze_caps {
                            self.frozen_cap[bi] = (cbe, cbc, qbe - cbe * vbe, qbc - cbc * vbc);
                        }
                        (qbe, cbe, qbc, cbc)
                    };
                    // Trailing-state reuse: remember this iter's charges. Only read back (post-loop)
                    // when the solve converged with freeze_caps off, where these are the true
                    // depletion+diffusion charges at the converged v (frozen path stores the
                    // companion approximation, never read because cap_cache_valid stays false).
                    self.cap_q_cache[bi] = (qbe, qbc);
                    let ibe = k2 * (qbe - t.qbe_prev) - t.ibe_prev; // b->e
                    let gbe = k2 * cbe;
                    let ibc = k2 * (qbc - t.qbc_prev) - t.ibc_prev; // b->c
                    let gbc = k2 * cbc;
                    if rb >= 0 {
                        let rb = rb as usize;
                        self.f[rb] += ibe + ibc;
                        self.j[rb * u + rb] += gbe + gbc;
                        if re >= 0 {
                            self.j[rb * u + re as usize] -= gbe;
                        }
                        if rc >= 0 {
                            self.j[rb * u + rc as usize] -= gbc;
                        }
                    }
                    if re >= 0 {
                        let re = re as usize;
                        self.f[re] -= ibe;
                        self.j[re * u + re] += gbe;
                        if rb >= 0 {
                            self.j[re * u + rb as usize] -= gbe;
                        }
                    }
                    if rc >= 0 {
                        let rc = rc as usize;
                        self.f[rc] -= ibc;
                        self.j[rc * u + rc] += gbc;
                        if rb >= 0 {
                            self.j[rc * u + rb as usize] -= gbc;
                        }
                    }
                }
            }

            // --freeze-caps: the iter that ran the full depletion_charge path has now populated
            // frozen_cap[] for every BJT; subsequent iters reuse the frozen C (`main.cpp:819`).
            if self.freeze_caps {
                self.frozen_caps_valid = true;
            }

            // gmin homotopy: conductance from each node to ground
            if gmin != 0.0 {
                for i in 0..NUM_NODES {
                    let r = self.urow[i];
                    if r >= 0 {
                        let r = r as usize;
                        self.f[r] += gmin * self.v[i];
                        self.j[r * u + r] += gmin;
                    }
                }
            }

            // 2-norm of residual; Newton step is a descent direction for this. This F is
            // evaluated at the current v, so in trim mode it also verifies the previous iter's
            // optimistic full step.
            let mut norm_f = 0.0;
            for i in 0..u {
                norm_f += self.f[i] * self.f[i];
            }
            norm_f = norm_f.sqrt();
            if norm_f < self.newton_tol {
                // Sub-rate spike: snapshot the converged J for the frozen-linear skipped samples.
                // Compiled away in the default build (no `capture_j` field without the feature).
                #[cfg(feature = "subrate-spike")]
                if self.capture_j {
                    self.j_frozen_raw[..u * u].copy_from_slice(&self.j[..u * u]);
                    self.j_captured = true;
                }
                // Trailing-state reuse: this iter built the device charges at the converged v and the
                // break skips the step, so `cap_q_cache` matches the returned v. Arm reuse only when
                // the cached charges are bit-identical to the scalar trailing recompute:
                //  - freeze_caps off — else the stored charges are the frozen companion, not true;
                //  - cheap_exp off — the batch always uses the accurate exp, but the scalar
                //    diode_branch would use ultra_exp, so they'd diverge.
                self.cap_cache_valid = !self.freeze_caps && !self.bjts[0].p.cheap_exp;
                break;
            }

            // Trim mode: retroactively reject a provisional step that increased the residual,
            // falling back to a real backtracking line search from the saved iterate. (Eager mode
            // never sets `provisional`, so this is skipped.) (`main.cpp:839-856`)
            if use_trim && provisional && norm_f > prev_norm_f {
                let mut lambda = 1.0;
                let mut accepted = 0.0;
                for _ls in 0..12 {
                    self.v.copy_from_slice(&v_before_step);
                    for i in 0..NUM_NODES {
                        let r = self.urow[i];
                        if r >= 0 {
                            self.v[i] += lambda * last_delta[r as usize];
                        }
                    }
                    let tn = self.residual_norm(gmin);
                    if tn < prev_norm_f {
                        accepted = lambda;
                        break;
                    }
                    lambda *= 0.5;
                }
                if accepted == 0.0 {
                    // no descent: restore and stop
                    self.v.copy_from_slice(&v_before_step);
                    break;
                }
                provisional = false; // corrected; rebuild + re-verify next iter
                iter += 1;
                continue;
            }

            for i in 0..u {
                self.delta[i] = -self.f[i];
            }
            // Solve J*delta = -F. Schur only on the gmin==0 hot path (gmin would perturb A_LL and
            // invalidate the per-sample factorization); falls back to dense if Schur prep failed
            // or the complement is singular this iter (`main.cpp:862-867`).
            let solved = if self.solver == Solver::Schur && gmin == 0.0 && self.schur_ready {
                self.schur_solve() || Self::gauss_solve(&mut self.j, &mut self.delta, u)
            } else if u == 16 {
                // Realtime tier's U: fixed-size, physical-swap specialization (bit-exact).
                Self::gauss_solve_16(&mut self.j, &mut self.delta)
            } else {
                Self::gauss_solve(&mut self.j, &mut self.delta, u)
            };
            if !solved {
                break;
            }

            // Bound step magnitude, preserving the Newton direction (ill-conditioned J).
            let mut max_abs = 0.0f64;
            for i in 0..u {
                max_abs = max_abs.max(self.delta[i].abs());
            }
            const MAX_STEP: f64 = 2.0; // volts
            if max_abs > MAX_STEP {
                let s = MAX_STEP / max_abs;
                for i in 0..u {
                    self.delta[i] *= s;
                }
            }

            if use_trim {
                // Save the iterate + step, take the full step optimistically, and defer
                // verification to the next iter's F build. No residual_norm call on the common
                // (monotone-converging) path (`main.cpp:878-892`).
                v_before_step.copy_from_slice(&self.v);
                last_delta[..u].copy_from_slice(&self.delta[..u]);
                prev_norm_f = norm_f;
                for i in 0..NUM_NODES {
                    let r = self.urow[i];
                    if r >= 0 {
                        self.v[i] += self.delta[r as usize];
                    }
                }
                provisional = true;
                let mut maxd = 0.0f64;
                for i in 0..u {
                    maxd = maxd.max(self.delta[i].abs());
                }
                if maxd < 1e-9 {
                    break;
                }
                iter += 1;
                continue;
            }

            if fixed {
                // Fixed-step: accept the full bounded Newton step, no `residual_norm` line search.
                let mut maxd = 0.0f64;
                for i in 0..NUM_NODES {
                    let r = self.urow[i];
                    if r >= 0 {
                        let d = self.delta[r as usize];
                        self.v[i] += d;
                        maxd = maxd.max(d.abs());
                    }
                }
                if maxd < 1e-9 {
                    break;
                }
                iter += 1;
                continue;
            }

            // Backtracking line search on 2-norm; scale whole vector (preserve direction).
            let mut v_save = [0.0f64; NUM_NODES];
            v_save.copy_from_slice(&self.v);
            let mut lambda = 1.0;
            let mut accepted = 0.0;
            for _ls in 0..12 {
                self.v.copy_from_slice(&v_save);
                for i in 0..NUM_NODES {
                    let r = self.urow[i];
                    if r >= 0 {
                        self.v[i] += lambda * self.delta[r as usize];
                    }
                }
                let tn = self.residual_norm(gmin);
                if tn < norm_f {
                    accepted = lambda;
                    break;
                }
                lambda *= 0.5;
            }
            if accepted == 0.0 {
                // no descent found: stay put, stop iterating
                self.v.copy_from_slice(&v_save);
                break;
            }

            let mut maxd = 0.0f64;
            for i in 0..u {
                maxd = maxd.max((accepted * self.delta[i]).abs());
            }
            if maxd < 1e-9 {
                break;
            }
            iter += 1;
        }
        // Trim mode: only when the loop hit the maxIters cap is the final optimistic step left
        // unverified (no following F build checked it). Verify once and roll back to the pre-step
        // iterate if the residual rose, so we never return a worse point than the step started
        // from (the eager path guarantees this per-step) (`main.cpp:922-925`).
        if use_trim && provisional && iter == self.max_iters {
            if self.residual_norm(gmin) > prev_norm_f {
                self.v.copy_from_slice(&v_before_step);
            }
        }
        iters_used
    }

}
