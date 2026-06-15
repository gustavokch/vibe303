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
    pub(crate) fn node_v_prev(&self, id: i32) -> f64 {
        if id < 0 {
            0.0
        } else {
            self.v_prev[id as usize]
        }
    }
    /// GND-safe row lookup (`main.cpp:449`).
    #[inline]
    pub(crate) fn ur(&self, id: i32) -> i32 {
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

    /// Flat row-major copy of the constant conductance matrix `g0` (first `u*u`
    /// entries). Diagnostic/test use only.
    pub fn g0_snapshot(&self) -> Vec<f64> {
        let u = self.u;
        self.g0[..u * u].to_vec()
    }
}
