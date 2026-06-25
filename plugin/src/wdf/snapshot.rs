use super::*;

impl WdfModel {

    /// Capture the current settled state (`main.cpp` integrator + DC offset). Pure.
    pub fn snapshot(&self) -> WdfSnapshot {
        let mut caps_i_prev = [0.0; MAX_CAPS];
        for i in 0..self.n_caps {
            caps_i_prev[i] = self.caps[i].i_prev;
        }
        let mut bjt_state = [(0.0, 0.0, 0.0, 0.0); 3];
        for (i, b) in self.bjts.iter().enumerate() {
            bjt_state[i] = (b.qbe_prev, b.ibe_prev, b.qbc_prev, b.ibc_prev);
        }
        WdfSnapshot {
            v: self.v,
            v_prev: self.v_prev,
            caps_i_prev,
            bjt_state,
            i_hist_p: self.i_hist_p,
            i_hist_s: self.i_hist_s,
            v_pprev: self.v_pprev,
            v_sprev: self.v_sprev,
            sec_in_dc: self.sec_in_dc,
        }
    }

    /// Restore integrator state from a snapshot; clears decimation-biquad + upsample state (their
    /// transient is sub-millisecond).
    pub fn restore(&mut self, s: &WdfSnapshot) {
        self.v = s.v;
        self.v_prev = s.v_prev;
        for i in 0..self.n_caps {
            self.caps[i].i_prev = s.caps_i_prev[i];
        }
        for (i, b) in self.bjts.iter_mut().enumerate() {
            b.qbe_prev = s.bjt_state[i].0;
            b.ibe_prev = s.bjt_state[i].1;
            b.qbc_prev = s.bjt_state[i].2;
            b.ibc_prev = s.bjt_state[i].3;
        }
        self.i_hist_p = s.i_hist_p;
        self.i_hist_s = s.i_hist_s;
        self.v_pprev = s.v_pprev;
        self.v_sprev = s.v_sprev;
        self.sec_in_dc = s.sec_in_dc;
        // Re-prime the predictor on the first audio sample after a restore (the warmup snapshot
        // carries no extrapolation history; a stale seed would overshoot sample 0).
        self.predict_primed = false;
        self.in_audio_pass = false;
        self.clear_driver_state();
    }

    /// Store the current state as the snapshot reset() restores to (call after `warmup`).
    pub fn save_snapshot(&mut self) {
        self.last_snapshot = Some(self.snapshot());
    }

    /// Silently settle the operating point after a [`restore`](Self::restore), before this model is
    /// faded in. A restore seeds an operating point captured from a DIFFERENT model variant (e.g. a
    /// High snapshot into a Medium model whose depletion-charge LUT companion makes that point a
    /// non-fixed-point), so the first solves' `N_SEC_IN` tap steps away from the restored `sec_in_dc`
    /// as Newton re-converges. That step drives the freshly-`reset` FIR resampler's step response,
    /// which emerges ~one FIR group delay later as a large transient — the precision-swap click the
    /// FIR resampler regressed (only the Medium/charge-LUT tier; High/Low match `sec_in_dc` and are
    /// clean). Solving `n` silent host samples here drains that step THROUGH the FIR before the model
    /// is faded in, so the crossfade blends a settled model. `n` must exceed the FIR impulse length
    /// (`~2 * FIR_LATENCY_HOST`); call sites size it with margin. Mirrors the adaptive-os
    /// `PrimeHi`/`PrimeLo` priming. Bounded (`n` solves, `os` each); swaps are rare user-driven tier
    /// changes already masked by the 60 ms crossfade, so the one-time cost is inaudible.
    pub fn settle_after_restore(&mut self, n: usize) {
        for _ in 0..n {
            let _ = self.process_host_sample(0.0);
        }
    }

    fn clear_driver_state(&mut self) {
        self.prev_in = 0.0;
        for bq in [&mut self.dec1, &mut self.dec2] {
            bq.x1 = 0.0;
            bq.x2 = 0.0;
            bq.y1 = 0.0;
            bq.y2 = 0.0;
        }
        if let Some(fir) = self.fir.as_mut() {
            fir.reset();
        }
    }

    /// Restore to the last snapshot if present, else zero all state. Clears the decimation filters.
    pub fn reset(&mut self) {
        if let Some(s) = self.last_snapshot {
            self.restore(&s);
        } else {
            self.v = [0.0; NUM_NODES];
            self.v_prev = [0.0; NUM_NODES];
            for i in 0..self.n_caps {
                self.caps[i].i_prev = 0.0;
            }
            for b in self.bjts.iter_mut() {
                b.qbe_prev = 0.0;
                b.ibe_prev = 0.0;
                b.qbc_prev = 0.0;
                b.ibc_prev = 0.0;
            }
            self.i_hist_p = 0.0;
            self.i_hist_s = 0.0;
            self.v_pprev = 0.0;
            self.v_sprev = 0.0;
            self.sec_in_dc = 0.0;
            self.v_pred_prev = [0.0; NUM_NODES];
            self.predict_primed = false;
            self.in_audio_pass = false;
            self.clear_driver_state();
        }
    }
}
