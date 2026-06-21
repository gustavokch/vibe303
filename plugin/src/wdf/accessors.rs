use super::*;

impl WdfModel {
    #[inline]
    pub fn node_v(&self, id: i32) -> f64 {
        if id < 0 {
            0.0
        } else {
            self.v[id as usize]
        }
    }
    #[inline]
    pub(in crate::wdf) fn node_v_prev(&self, id: i32) -> f64 {
        if id < 0 {
            0.0
        } else {
            self.v_prev[id as usize]
        }
    }
    /// GND-safe row lookup (`main.cpp:449`).
    #[inline]
    pub(in crate::wdf) fn ur(&self, id: i32) -> i32 {
        if id < 0 {
            -1
        } else {
            self.urow[id as usize]
        }
    }

    pub fn u(&self) -> usize {
        self.u
    }
    /// Size of the Schur nonlinear-coupled (N) partition. 6 for the linear circuit; the JA core
    /// (when active) adds N_PA → 7.
    pub fn n_sz(&self) -> usize {
        self.n_sz
    }
    pub fn n_res(&self) -> usize {
        self.n_res
    }
    pub fn n_caps(&self) -> usize {
        self.n_caps
    }
    pub fn n_bjts(&self) -> usize {
        self.bjts.len()
    }
    pub fn urow_of(&self, id: i32) -> i32 {
        self.ur(id)
    }

    /// ACTUAL group-delay latency (HOST samples) of the steep linear-phase FIR resampler on THIS
    /// tier: `FIR_LATENCY_HOST` when the FIR is live (`!legacy_resample` and `os > 1`), else 0
    /// (legacy resampler, or the os==1 tiers where `fir == None`). Drives the per-backend wet-path
    /// padding (`dsp.rs::resize_delays`) — the WDF wet already carries this much intrinsic delay.
    pub fn fir_latency_host(&self) -> usize {
        self.fir.as_ref().map_or(0, |f| f.latency_host())
    }

    /// TIER-INVARIANT FIR latency (HOST samples) the plugin REPORTS to the host: `FIR_LATENCY_HOST`
    /// for every non-legacy tier (including the os==1 tiers, whose `fir == None` actual latency is 0
    /// but which are padded up to this constant so the reported PDC never changes on a live tier
    /// switch), and 0 for the legacy resampler (golden-parity, genuine 0-PDC). The padding amount on
    /// the os==1 WDF wet path is exactly `fir_reported_latency_host() - fir_latency_host()`.
    pub fn fir_reported_latency_host(&self) -> usize {
        if self.legacy_resample {
            0
        } else {
            super::resample::FIR_LATENCY_HOST
        }
    }

    /// Flat row-major copy of the constant conductance matrix `g0` (first `u*u`
    /// entries). Diagnostic/test use only.
    pub fn g0_snapshot(&self) -> Vec<f64> {
        let u = self.u;
        self.g0[..u * u].to_vec()
    }
}
