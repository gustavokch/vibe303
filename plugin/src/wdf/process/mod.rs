use super::*;

impl WdfModel {

    /// Solve one sample. `v[N_A]`, `v[N_INPUT]` must be set before calling (`main.cpp:452-516`).
    pub fn process(&mut self) {
        use node::{N_A, N_PA, N_SA};
        let u = self.u;
        // Sub-rate Newton spike (behind `subrate-spike` feature). In the default build these two
        // lines are replaced by `let skipped = false` so the feature-gated solve block compiles away.
        #[cfg(feature = "subrate-spike")]
        let subrate_on = self.subrate > 1 && self.in_audio_pass;
        #[cfg(feature = "subrate-spike")]
        let skipped = subrate_on && self.subrate_valid && self.subrate_phase != 0;
        #[cfg(not(feature = "subrate-spike"))]
        let skipped = false;
        // Newton predictor: seed the solve with a 1st-order linear extrapolation of the last two
        // converged internal samples (2*sol(n-1) - sol(n-2)) for UNKNOWN nodes only. Same fixed
        // point + tol => identical converged v, just fewer iters. Off during DC warmup/rail-ramp/
        // settle (in_audio_pass false), where transient swings would make it overshoot
        // (`main.cpp:621-633`). Modifies only `v` (the guess); `hist` below reads `v_prev`.
        // Skipped sub-rate samples solve an affine system (guess-independent) or hold, so no predict.
        // Defensive: never extrapolate under fixed-step. The 2-iter fixed-step solve has no
        // `residual_norm` gate / `ls_trim` to correct a bad warm-start, so a predicted guess drifts
        // and diverges (see `with_precision`). `with_precision` already clears `predict` for Med/Low;
        // this guard makes any fixed_step config safe regardless of how it was assembled.
        if self.predict && !self.fixed_step && self.in_audio_pass && !skipped {
            if !self.predict_primed {
                self.v_pred_prev = self.v; // remember sol(n-1); no extrapolation yet
                self.predict_primed = true;
            } else {
                for i in 0..NUM_NODES {
                    if self.urow[i] < 0 {
                        continue; // skip known/pinned nodes (same mask as the solver)
                    }
                    let cur = self.v[i]; // sol(n-1)
                    self.v[i] = 2.0 * self.v[i] - self.v_pred_prev[i];
                    self.v_pred_prev[i] = cur; // becomes sol(n-2) for the next call
                }
            }
        }
        // Per-sample constant history offsets (do not depend on v[n]).
        for i in 0..u {
            self.hist[i] = 0.0;
        }
        for i in 0..self.n_caps {
            let c = self.caps[i];
            let ih = c.geq * (self.node_v_prev(c.a) - self.node_v_prev(c.b)) + c.i_prev;
            let (ra, rb) = (self.ur(c.a), self.ur(c.b));
            if ra >= 0 {
                self.hist[ra as usize] -= ih;
            }
            if rb >= 0 {
                self.hist[rb as usize] += ih;
            }
        }
        // trapezoidal inductor history: i[n-1] + g*v[n-1]
        let ind_hist_p = self.i_hist_p + self.gl[0][0] * self.v_pprev + self.gl[0][1] * self.v_sprev;
        let ind_hist_s = self.i_hist_s + self.gl[1][0] * self.v_pprev + self.gl[1][1] * self.v_sprev;
        let (rp, rs) = (self.ur(N_PA), self.ur(N_SA));
        if rp >= 0 {
            self.hist[rp as usize] += ind_hist_p;
        }
        if rs >= 0 {
            self.hist[rs as usize] += ind_hist_s;
        }
        // JA core (decoupled): inject the lagged magnetizing-deviation current as a known source on
        // N_PA's RHS row. Computed at the previous sample's commit (one os-step lag) — the J-A is
        // outside the Newton loop, so this is the ONLY place the saturation reaches the network.
        // N_A is a known rail (no unknown row), so only N_PA receives the injection.
        #[cfg(feature = "trafo-core")]
        if self.ja_active && self.in_audio_pass {
            if rp >= 0 {
                self.hist[rp as usize] += self.ja_i_dev;
            }
        }

        // Sub-rate spike solve path. With the feature, may skip Newton and reuse the frozen model.
        // Without the feature, this block is absent; the non-feature block below is the only path.
        #[cfg(feature = "subrate-spike")]
        if skipped {
            // Frozen sub-rate sample: reuse the captured Jacobian + LU. `solve_frozen` does one
            // back-substitution; `subrate_hold` is the naive baseline that freezes v instead.
            if !self.subrate_hold {
                self.solve_frozen();
            }
            // No Newton ran, so any armed charge cache is stale; force the trailing recompute.
            self.cap_cache_valid = false;
            // Instrumentation: counts as a sample with 0 device-Newton iters (the cost proxy the
            // harness reads — avgIters drops ~1/subrate when the freeze is doing real work).
            self.iter_samples += 1;
        } else {
            // Full solve. Snapshot the converged J for the following skipped samples when sub-rate
            // is engaged (`capture_j` gates the snapshot inside `newton_iterate`; off otherwise so
            // the bit-exact golden path is untouched).
            if subrate_on {
                self.j_captured = false;
                self.capture_j = true;
            }
            let mut sample_iters = self.newton_iterate(0.0);
            let mut sample_max = sample_iters;
            if subrate_on {
                self.capture_j = false; // gmin homotopy below must not overwrite the gmin=0 J
            }
            // Fixed-step (Med/Low precision) skips the per-sample convergence gate entirely: the
            // `residual_norm` re-stamp + gmin-homotopy fallback are the dominant fixed per-sample
            // cost, and the budget tolerates the occasional unconverged hard sample. HIGH/realtime
            // keep it.
            if !self.fixed_step {
                let resid = self.residual_norm(0.0);
                if resid > 1e-3 {
                    // hard sample: gmin-stepping homotopy
                    for g in [1e-1, 1e-2, 1e-3, 1e-4, 1e-5, 0.0] {
                        let it = self.newton_iterate(g);
                        sample_iters += it;
                        sample_max = sample_max.max(it);
                    }
                }
            }
            if subrate_on {
                self.capture_frozen();
            }
            // Instrumentation only (see field comment): never read back during the solve.
            self.iter_samples += 1;
            self.iter_total += sample_iters as u64;
            self.iter_max = self.iter_max.max(sample_max);
        }
        #[cfg(feature = "subrate-spike")]
        if subrate_on {
            self.subrate_phase = (self.subrate_phase + 1) % self.subrate;
        }
        // Default (no sub-rate spike): straight full-solve path with no extra branches.
        #[cfg(not(feature = "subrate-spike"))]
        {
            let mut sample_iters = self.newton_iterate(0.0);
            let mut sample_max = sample_iters;
            // Fixed-step (Med/Low precision) skips the per-sample convergence gate entirely: the
            // `residual_norm` re-stamp + gmin-homotopy fallback are the dominant fixed per-sample
            // cost, and the budget tolerates the occasional unconverged hard sample. HIGH/realtime
            // keep it.
            if !self.fixed_step {
                let resid = self.residual_norm(0.0);
                if resid > 1e-3 {
                    // hard sample: gmin-stepping homotopy
                    for g in [1e-1, 1e-2, 1e-3, 1e-4, 1e-5, 0.0] {
                        let it = self.newton_iterate(g);
                        sample_iters += it;
                        sample_max = sample_max.max(it);
                    }
                }
            }
            // Instrumentation only (see field comment): never read back during the solve.
            self.iter_samples += 1;
            self.iter_total += sample_iters as u64;
            self.iter_max = self.iter_max.max(sample_max);
        }

        // Fixed-step (Med/Low) drops the gmin-homotopy fallback, so a hard transient that fails to
        // converge in `max_iters` can leave `v` non-finite. WDF state feeds forward (`v_prev`, `hist`,
        // cap/BJT/inductor currents below), so a single NaN/Inf would poison every later sample —
        // permanent silence or blast in a realtime host, clearable only by reset. Recover by holding
        // the last converged iterate: restore `v` from `v_prev` so this sample freezes at the previous
        // output and forward state stays clean. HIGH/realtime never reach this (the gmin gate above
        // keeps `v` finite), and the finite path is a no-op, so the bit-exact contract is untouched.
        // Sub-rate skipped samples share the risk (a singular frozen solve could go non-finite and
        // poison forward state), so guard them the same way. Without `subrate-spike`, `skipped` is
        // always `false` and the optimizer elides the `|| skipped` term in release builds.
        if (self.fixed_step || skipped) && !self.v.iter().all(|x| x.is_finite()) {
            self.v.copy_from_slice(&self.v_prev);
        }

        // update cap currents (trapezoidal) for next sample, using old v_prev
        for i in 0..self.n_caps {
            let c = self.caps[i];
            let dv = self.node_v(c.a) - self.node_v(c.b);
            let dv_prev = self.node_v_prev(c.a) - self.node_v_prev(c.b);
            self.caps[i].i_prev = c.geq * (dv - dv_prev) - c.i_prev;
        }
        // update BJT charge-cap state (trapezoidal) for next sample. Reuse the converged charges the
        // Newton solve already built (cap_cache_valid) instead of re-running diode_branch +
        // depletion_charge_pre here — lossless, since on the armed path the cached charges are
        // bit-identical to this recompute (verified every sample in debug builds).
        if self.cap_tier != CapTier::Off {
            let k2 = 2.0 / self.dt;
            for bi in 0..self.bjts.len() {
                let t = self.bjts[bi];
                let (qbe, qbc) = if self.cap_cache_valid {
                    let cached = self.cap_q_cache[bi];
                    // Near-lossless, not bit-exact: the cached charges come from the batched device
                    // kernel (exp arg = vbe*(1/vt), multiply-by-reciprocal) while `junction_charges`
                    // uses scalar `diode_branch` (vbe/vt, true division) — they agree to a few ULP.
                    // That's ~1e-15 relative on a charge ~1e-12, far below the model's −23 dB floor
                    // and the 1e-5 golden gate. Debug builds verify the cache tracks the recompute
                    // on every sample (catches a wrong node/index/v bug — relative drift would be
                    // O(1), not O(ULP) — at near-zero release cost).
                    #[cfg(debug_assertions)]
                    {
                        let (rq_be, rq_bc) = self.junction_charges(&t);
                        let close = |a: f64, b: f64| {
                            (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1e-30)
                        };
                        debug_assert!(close(cached.0, rq_be), "qbe cache drift bjt {bi}: {} vs {}", cached.0, rq_be);
                        debug_assert!(close(cached.1, rq_bc), "qbc cache drift bjt {bi}: {} vs {}", cached.1, rq_bc);
                    }
                    cached
                } else {
                    self.junction_charges(&t)
                };
                let bjt = &mut self.bjts[bi];
                bjt.ibe_prev = k2 * (qbe - bjt.qbe_prev) - bjt.ibe_prev;
                bjt.qbe_prev = qbe;
                bjt.ibc_prev = k2 * (qbc - bjt.qbc_prev) - bjt.ibc_prev;
                bjt.qbc_prev = qbc;
            }
        }
        // update inductor winding currents (trapezoidal) for next sample
        let vp = self.node_v(N_PA) - self.node_v(N_A);
        let vs = self.node_v(N_SA);
        self.i_hist_p += self.gl[0][0] * (vp + self.v_pprev) + self.gl[0][1] * (vs + self.v_sprev);
        self.i_hist_s += self.gl[1][0] * (vp + self.v_pprev) + self.gl[1][1] * (vs + self.v_sprev);
        self.v_pprev = vp;
        self.v_sprev = vs;

        // JA core: commit magnetization + flux state for the next sample.
        // NOTE — `trafo-core` × `subrate-spike` is an UNTESTED combination. On a sub-rate-skipped
        // sample Newton did not run, so `node_v(N_PA)` is the frozen/held solve; `ja_commit` would
        // then integrate flux from that held voltage. Both are experimental spikes not meant to be
        // enabled together; if they ever are, validate this path (or gate `ja_commit` off when the
        // sample was skipped). See the PR #40 review plan, P3 #6.
        #[cfg(feature = "trafo-core")]
        if self.ja_active && self.in_audio_pass {
            self.ja_commit();
        }

        for i in 0..NUM_NODES {
            self.v_prev[i] = self.v[i];
        }
    }

    fn internal_sr(&self) -> f64 {
        1.0 / self.dt
    }

    /// Idle settle with tap-delta early-exit (`main.cpp:874-886`), capped at `seconds`.
    fn idle_settle(&mut self, seconds: f64) {
        let internal_sr = self.internal_sr();
        let chunk = (internal_sr * 0.25) as usize;
        let idle_max = (internal_sr * seconds) as usize;
        let mut done = 0;
        let mut prev_tap = 1e9;
        while done < idle_max {
            for _ in 0..chunk {
                self.process();
            }
            done += chunk;
            let tap = self.v[node::N_SEC_IN as usize];
            if (tap - prev_tap).abs() < 1e-4 {
                break; // settled
            }
            prev_tap = tap;
        }
    }

    /// Soft-start from cold: ramp the +24V rail (1 s homotopy), let the DC feedback loop self-bias
    /// (idle-settle, early-exit), then capture the output DC offset. Mirrors `main.cpp:865-916`
    /// Newton-iteration stats since the last [`reset_iter_stats`]: `(samples, avg_iters, max_iters)`.
    /// `samples` counts `process()` (internal-rate) solves; `avg` is total iters / samples. Pure
    /// instrumentation for the offline A/B harness — mirrors C++ `avgIters`/`maxItersUsed`.
    pub fn iter_stats(&self) -> (u64, f64, usize) {
        let avg = if self.iter_samples > 0 {
            self.iter_total as f64 / self.iter_samples as f64
        } else {
            0.0
        };
        (self.iter_samples, avg, self.iter_max)
    }

    /// Zero the Newton-iteration counters (call after warmup so the stats reflect the audio pass).
    pub fn reset_iter_stats(&mut self) {
        self.iter_samples = 0;
        self.iter_total = 0;
        self.iter_max = 0;
    }

    /// MINUS auto-bias — the bias is a direct param here, not bisected to a target idle current.
    pub fn warmup(&mut self) {
        let internal_sr = self.internal_sr();
        let ramp = internal_sr as usize; // 1 s
        self.v[node::N_INPUT as usize] = 0.0;
        for i in 0..ramp {
            self.v[node::N_A as usize] = 24.0 * (i as f64) / (ramp as f64);
            self.process();
        }
        self.v[node::N_A as usize] = 24.0;
        self.idle_settle(4.0);
        self.sec_in_dc = self.v[node::N_SEC_IN as usize];
        // Leave the settled state as the restore target so reset()/measure_gain() have a clean
        // operating point to fall back to even if the caller never calls save_snapshot() itself.
        self.save_snapshot();
    }

    /// Process one host-rate sample: linear-upsample to `OS` subsamples, Newton-solve each, decimate
    /// the DC-corrected tap through the two Butterworth biquads (`main.cpp:942-953`).
    #[inline]
    pub fn process_host_sample(&mut self, x: f32) -> f32 {
        // Audio pass: enable the Newton predictor (idempotent; warmup runs with it off).
        // JA core: on the warmup→audio transition, reset the flux/magnetization to a clean start
        // from the just-settled operating point (where v_m ≈ 0), so the rail-ramp transient never
        // saturates the core and poisons forward state.
        #[cfg(feature = "trafo-core")]
        if self.ja_active && !self.in_audio_pass {
            self.ja_reseed();
        }
        self.in_audio_pass = true;
        let cur_in = x as f64;
        let mut decimated = 0.0;
        for j in 0..self.os {
            let frac = (j + 1) as f64 / self.os as f64; // linear-interpolate upsample
            self.v[node::N_INPUT as usize] = self.prev_in + (cur_in - self.prev_in) * frac;
            self.process();
            let tap = self.v[node::N_SEC_IN as usize] - self.sec_in_dc;
            // At os>1 (Realtime, and UltraFast/Warp floored to os=2 under Med/Low fixed-step) the
            // biquads are the antialiasing decimator. At os=1 (UltraFast/Warp at High) there is
            // nothing to decimate and `decimate=false` is the live bypass, not just an A/B knob.
            decimated = if self.decimate {
                self.dec2.process(self.dec1.process(tap))
            } else {
                tap
            };
        }
        self.prev_in = cur_in;
        decimated as f32
    }

    /// Measurement-only: run the os-rate solve for every input sample and emit **every** internal
    /// subsample tap (the same DC-corrected `v[N_SEC_IN]` that `process_host_sample` decimates,
    /// line 321), pre-decimation, into `out` at `os × host` rate. A harness can then apply an
    /// arbitrary external decimator (4th vs 8th order) to one solver run and compare apples-to-apples,
    /// or FFT the raw stream to see where the alias energy sits (image band vs already-folded in-band).
    ///
    /// Mirrors `process_host_sample`'s loop exactly (predictor/JA reseed on the warmup→audio
    /// transition, `prev_in` linear-interpolation upsample) so the captured stream is identical to
    /// what the live path decimates. Not compiled into the shipped plugin (`measure` feature, off by
    /// default).
    #[cfg(feature = "measure")]
    pub fn capture_os2_raw(&mut self, input: &[f32], out: &mut Vec<f32>) {
        out.clear();
        out.reserve(input.len() * self.os);
        for &x in input {
            #[cfg(feature = "trafo-core")]
            if self.ja_active && !self.in_audio_pass {
                self.ja_reseed();
            }
            self.in_audio_pass = true;
            let cur_in = x as f64;
            for j in 0..self.os {
                let frac = (j + 1) as f64 / self.os as f64; // linear-interpolate upsample
                self.v[node::N_INPUT as usize] = self.prev_in + (cur_in - self.prev_in) * frac;
                self.process();
                out.push((self.v[node::N_SEC_IN as usize] - self.sec_in_dc) as f32);
            }
            self.prev_in = cur_in;
        }
    }

    /// Measure the settled circuit's small-signal voltage gain (output/input RMS) at a 1 kHz
    /// reference, on a SCRATCH CLONE so the live model is untouched. This is the one-shot static
    /// levelNorm probe: the plugin caches `1 / measure_gain()` and applies it as a constant trim,
    /// replacing the old continuous RMS estimator (no per-sample pumping). Block-rate only.
    ///
    /// In the dsp chain `wet/x == G_v` (the circuit voltage gain) for an undriven signal, so this
    /// ratio is exactly what the trim must invert. Driven nonlinearity is intentionally NOT probed —
    /// the trim is the clean operating-point gain, recomputed only when the operating point changes.
    pub fn measure_gain(&self) -> f32 {
        let mut m = self.clone();
        // Drain the live audio/saturation state before probing. The plugin re-probes this on every
        // glide block, and under Drive the live model is wound deep into saturation — a small-signal
        // probe on that state reads a gain wildly off the operating point (measured 22x -> 250x at
        // +18 dB) and jumps block-to-block, which makes the levelNorm trim jitter into loud peaks
        // during a Circuit-Gain sweep. Restoring the settled snapshot (clean DC operating point, the
        // model's G0 is still current) gives a deterministic probe regardless of program/Drive.
        m.reset();
        // Cheapen the probe's solver: this runs ON THE AUDIO THREAD on every operating-point change
        // (an automation lane steps the param every block), so the full-fidelity solver — measured at
        // ~30 ms/probe at realtime+High, ~23x a 64-sample block budget — is far too slow. The 0.05 V
        // stimulus keeps the circuit near-linear, where a 2-iter fixed-step solve converges to the
        // SAME small-signal gain as the full Newton solve. `os`/`dt`/`cap_tier` are left untouched —
        // they set the solve timestep + the AC frequency shaping that actually defines G_v.
        //
        // NOTE: this is the one place `fixed_step` runs at the tier's `os` (which is 1 for UltraFast/
        // Warp — at EVERY precision now, not only High: precision is device-model-only and no longer
        // floors the rate, so Med/Low also build at os=1 and the probe clones that). It mutates an
        // already-built model, bypassing `solve_os`. That is deliberate and safe: the old os-floor
        // existed because `fixed_step` + os=1 limit-cycles into loud peaks under program-level drive,
        // but this probe is a 0.05 V near-linear, deterministic measurement that never enters that
        // regime, and preserving `os`/`dt` is what keeps the measured gain correct. (Trapezoidal
        // frequency warp at the 1 kHz probe differs negligibly between os=1/os=2; the
        // `gain_probe_os1_tiers_track_high` test pins that Med/Low track High on the os=1 tiers.)
        m.fixed_step = true;
        m.predict = false;
        m.max_iters = m.max_iters.min(2);
        let host_sr = self.internal_sr() / self.os as f64;
        let f = 1000.0_f64;
        let amp = 0.05_f64; // small-signal probe level (volts); stays in the near-linear region
        let cycle = (host_sr / f).max(1.0);
        // Settle: the restored snapshot is the settled DC operating point and G0 is current, so the
        // 1 kHz AC steady-state (well above the coupling-cap corner) re-establishes within a few
        // cycles. Kept short to bound the audio-thread cost; `probe_is_deterministic_and_pure` and
        // `probe_jitter_under_drive` pin that this stays stable and the sweep tests pin accuracy.
        let n_settle = (cycle * 4.0) as usize; // let the AC steady-state re-establish
        let n_meas = (cycle * 4.0) as usize; // integrate several cycles for a clean RMS
        let w = 2.0 * std::f64::consts::PI * f / host_sr;
        let (mut in_sq, mut out_sq) = (0.0f64, 0.0f64);
        for i in 0..(n_settle + n_meas) {
            let x = amp * (w * i as f64).sin();
            let y = m.process_host_sample(x as f32) as f64;
            if i >= n_settle {
                in_sq += x * x;
                out_sq += y * y;
            }
        }
        if in_sq <= 0.0 || !out_sq.is_finite() {
            return 1.0;
        }
        (out_sq / in_sq).sqrt() as f32
    }

    /// Re-converge the DC operating point in place after a cheap G0 re-stamp (rgain/bias/age/trafo
    /// changed but the integrator state was kept), re-snapshot it as the probe's clean restore
    /// target, then return [`measure_gain`]. This is the per-request op of the OFF-THREAD levelNorm
    /// probe worker (see `Ba283Plugin::task_executor`): the worker holds one persistent warmed model
    /// and re-stamps it each request instead of paying the ~1 s cold `warmup`, so a single probe
    /// settles in the handful of `idle_settle` chunks a small operating-point step needs. Off the
    /// audio thread — never call this in `process`.
    pub fn resettle_and_measure_gain(&mut self) -> f32 {
        self.idle_settle(2.0); // early-exits once the tap stops moving (≈ a few chunks for a small step)
        self.sec_in_dc = self.v[node::N_SEC_IN as usize];
        self.save_snapshot();
        self.measure_gain()
    }

    pub fn sec_in_dc(&self) -> f64 {
        self.sec_in_dc
    }
}

#[cfg(test)]
mod tests;
