use super::*;

impl WdfModel {

    /// Build the circuit at the given **internal** sample rate (host × OS), gain shunt `rgain`
    /// (R_KJ ohms), and `bias` (RV1 pot [0,1]) at the realtime tier (all levers off).
    /// Thin wrapper over [`build_with`]; mirrors `BA283::build` (`main.cpp:314-409`).
    pub fn build(internal_sr: f64, rgain: f64, bias: f64) -> Self {
        Self::build_with(QualityConfig::realtime(), internal_sr, rgain, bias)
    }

    /// Build the circuit with an explicit [`QualityConfig`] (tier/lever selection). The topology
    /// is fixed (RB off, U=16); only the solver levers vary. Mirrors `BA283::build` +
    /// `qualityPreset` (`main.cpp:314-409`, `main.cpp:92`).
    pub fn build_with(cfg: QualityConfig, internal_sr: f64, rgain: f64, bias: f64) -> Self {
        use node::*;
        let dt = 1.0 / internal_sr;

        let mut known = [false; NUM_NODES];
        known[N_A as usize] = true;
        known[N_INPUT as usize] = true;
        // RB off: pin the 3 intrinsic-base nodes (V=0) so they're excluded from the unknown map.
        known[N_TR1_BI as usize] = true;
        known[N_TR2_BI as usize] = true;
        known[N_TR3_BI as usize] = true;

        let mut urow = [-1i32; NUM_NODES];
        let mut u = 0usize;
        for i in 0..NUM_NODES {
            urow[i] = if known[i] {
                -1
            } else {
                let r = u as i32;
                u += 1;
                r
            };
        }

        // --- resistors (builder may use Vec; the process path is array-only) ---
        let mut rv: Vec<Res> = Vec::new();
        let push_r = |rv: &mut Vec<Res>, a: i32, b: i32, ohms: f64| rv.push(Res { a, b, g: 1.0 / ohms });
        let (r_upper, r_lower) = rv1_split(bias);
        // TR1 (BC184C) common-emitter input
        push_r(&mut rv, N_C1_MID, N_R1_MID, 1.0); // R_esr_C7 (C1 ESR)
        push_r(&mut rv, N_R1_MID, N_TR1_B, 2.2e3); // R1 input series
        push_r(&mut rv, N_TR1_B, GND, 68.0e3); // R3 base bleed
        push_r(&mut rv, N_RV1_W, N_TR1_B, 56.0e3); // R2 DC-bias feedback
        push_r(&mut rv, N_TR1_E, GND, 1.2e3); // R4 emitter load
        let r_kj_idx = rv.len();
        push_r(&mut rv, N_TR1_E, N_J, rgain); // R_KJ (gain knob)
        push_r(&mut rv, N_CGAIN_MID, GND, 1.0); // R_esr_Cgain
        push_r(&mut rv, N_C4_MID, N_TR1_E, 3.3e3); // R5 AC feedback
        push_r(&mut rv, N_A, N_TR1_C, 33.0e3); // R8 collector load
        // TR2+TR3 compound common-collector pair
        push_r(&mut rv, N_TR2_E, GND, 18.0e3); // R6 TR2 emitter load
        push_r(&mut rv, N_TR3_E, GND, 47.0); // R7 TR3 emitter resistor
        let r_upper_idx = rv.len();
        push_r(&mut rv, N_TR3_E, N_RV1_W, r_upper); // RV1 upper
        let r_lower_idx = rv.len();
        push_r(&mut rv, N_RV1_W, GND, r_lower); // RV1 lower
        // LO1166 + primary Zobel + secondary termination
        push_r(&mut rv, N_F, N_PA, 14.5); // R_pri winding DCR
        push_r(&mut rv, N_SEC_IN, N_SA, 44.0); // R_sec winding DCR
        push_r(&mut rv, N_SEC_IN, GND, 600.0); // R_term (console line in)
        push_r(&mut rv, N_F, N_ZOBEL, 1.5e3); // R_zobel (primary Boucherot snubber)
        // RB off: bases wire directly (no RB resistors).

        let mut res = [Res::default(); MAX_RES];
        let n_res = rv.len();
        res[..n_res].copy_from_slice(&rv);

        // Capture pristine resistor conductances for aging (re-derived per age apply).
        let mut res_base_g = [0.0f64; MAX_RES];
        for i in 0..n_res {
            res_base_g[i] = res[i].g;
        }

        // --- caps ---
        let mut cv: Vec<Cap> = Vec::new();
        let push_c = |cv: &mut Vec<Cap>, a: i32, b: i32, farads: f64| {
            cv.push(Cap { a, b, c: farads, geq: 2.0 * farads / dt, i_prev: 0.0 })
        };
        push_c(&mut cv, N_INPUT, N_C1_MID, 10.0e-6); // C1 input coupling
        push_c(&mut cv, N_J, N_CGAIN_MID, 80.0e-6); // Cgain
        push_c(&mut cv, N_F, N_C4_MID, 100.0e-6); // C4 AC-feedback DC block
        push_c(&mut cv, N_TR1_C, GND, 4700.0e-12); // C3 on TR1 collector
        push_c(&mut cv, N_TR1_B, N_TR1_C, 220.0e-12); // C2 Miller across TR1 B-C
        push_c(&mut cv, N_TR1_C, N_F, 330.0e-12); // C5 Miller across TR2 B-C
        push_c(&mut cv, N_ZOBEL, N_A, 0.01e-6); // Zobel cap (primary, to rail)

        let mut caps = [Cap::default(); MAX_CAPS];
        let n_caps = cv.len();
        caps[..n_caps].copy_from_slice(&cv);

        // Capture pristine cap farads for aging (geq re-derived per age apply).
        let mut cap_base_far = [0.0f64; MAX_CAPS];
        for i in 0..n_caps {
            cap_base_far[i] = caps[i].c;
        }

        // --- BJTs (RB off: external base directly) ---
        let mut bc = bc184c();
        let mut q = q2n3055();
        bc.cheap_exp = cfg.cheap_exp; // ultra-fast tier: low-order transport exp
        q.cheap_exp = cfg.cheap_exp;
        let bjts = [
            Bjt::new(N_TR1_B, N_TR1_E, N_TR1_C, bc), // TR1
            Bjt::new(N_TR1_C, N_TR2_E, N_F, bc),     // TR2 (base = TR1_C, collector = F)
            Bjt::new(N_TR2_E, N_TR3_E, N_F, q),      // TR3 (base = TR2_E, collector = F)
        ];

        // coupled inductor companion: L = [Lp M; M Ls], M = K*sqrt(Lp*Ls)
        let (lp, ls, k): (f64, f64, f64) = (1.9, 5.36, 0.9995);
        let m = k * (lp * ls).sqrt();
        let det = lp * ls - m * m;
        let l_inv = [[ls / det, -m / det], [-m / det, lp / det]];
        let h = dt * 0.5;
        let gl = [
            [h * l_inv[0][0], h * l_inv[0][1]],
            [h * l_inv[1][0], h * l_inv[1][1]],
        ];

        let mut model = WdfModel {
            v: [0.0; NUM_NODES],
            v_prev: [0.0; NUM_NODES],
            urow,
            u,
            res,
            n_res,
            caps,
            n_caps,
            // Depletion-charge LUTs from the fixed junction cards. Built here (off the audio
            // thread) only when the tier actually samples them (`dep_lut`); the bit-exact `pow`
            // tiers (realtime/warp/Low) skip the fill via `empty()`. Shared via Arc so model clones
            // (e.g. measure_gain) don't deep-copy. Tier switches go through a full `new_with`
            // rebuild, so this stays consistent with `cfg.dep_lut` — never toggled in place.
            dep_luts: Arc::new(if cfg.dep_lut {
                DepLutSet::build(&bjts)
            } else {
                DepLutSet::empty()
            }),
            bjts,
            r_kj_idx,
            r_upper_idx,
            r_lower_idx,
            res_base_g,
            cap_base_far,
            age_pattern: AgePattern::identity(),
            age: 0.0,
            age_seed: 0,
            gl,
            i_hist_p: 0.0,
            i_hist_s: 0.0,
            v_pprev: 0.0,
            v_sprev: 0.0,
            #[cfg(feature = "trafo-core")]
            ja_active: false,
            #[cfg(feature = "trafo-core")]
            ja: JaCore::lo1166(),
            #[cfg(feature = "trafo-core")]
            ja_m: 0.0,
            #[cfg(feature = "trafo-core")]
            ja_h_prev: 0.0,
            #[cfg(feature = "trafo-core")]
            ja_phi_prev: 0.0,
            #[cfg(feature = "trafo-core")]
            ja_vm_prev: 0.0,
            #[cfg(feature = "trafo-core")]
            ja_delta: 1.0,
            #[cfg(feature = "trafo-core")]
            ja_i_dev: 0.0,
            dt,
            newton_tol: cfg.newton_tol,
            max_iters: cfg.max_iters,
            cap_tier: cfg.cap_tier,
            hist: [0.0; NUM_NODES],
            f: [0.0; NUM_NODES],
            delta: [0.0; NUM_NODES],
            rn: [0.0; NUM_NODES],
            j: [0.0; NUM_NODES * NUM_NODES],
            g0: [0.0; NUM_NODES * NUM_NODES],
            predict: cfg.predict,
            solver: cfg.solver,
            freeze_caps: cfg.freeze_caps,
            ls_trim: cfg.ls_trim,
            fma: cfg.fma,
            dep_lut: cfg.dep_lut,
            fixed_step: cfg.fixed_step,
            // Oversampling is a pure config value: os=1 is the ultra-fast/warp speed lever, os=2 the
            // antialiasing rate. `solve_os`/`solve_decimate` remain the single source of truth (also
            // used by `new_with`); precision is device-model-only so there is no longer an os floor.
            os: cfg.solve_os(),
            decimate: cfg.solve_decimate(),
            #[cfg(feature = "adaa-insolve")]
            adaa: false,
            #[cfg(feature = "adaa-port")]
            adaa_port: false,
            v_pred_prev: [0.0; NUM_NODES],
            predict_primed: false,
            in_audio_pass: false,
            frozen_cap: [(0.0, 0.0, 0.0, 0.0); 3],
            frozen_caps_valid: false,
            cap_q_cache: [(0.0, 0.0); 3],
            cap_cache_valid: false,
            n_idx: [0; NUM_NODES],
            l_idx: [0; NUM_NODES],
            n_sz: 0,
            l_sz: 0,
            schur_ready: false,
            s_ll_fac: [0.0; NUM_NODES * NUM_NODES],
            s_ll_piv: [0; NUM_NODES],
            s_w: [0.0; NUM_NODES * NUM_NODES],
            s_sconst: [0.0; NUM_NODES * NUM_NODES],
            s_anl: [0.0; NUM_NODES * NUM_NODES],
            s_aln: [0.0; NUM_NODES * NUM_NODES],
            s_annlin: [0.0; NUM_NODES * NUM_NODES],
            s_s: [0.0; NUM_NODES * NUM_NODES],
            dec1: Biquad::default(),
            dec2: Biquad::default(),
            sec_in_dc: 0.0,
            prev_in: 0.0,
            last_snapshot: None,
            iter_samples: 0,
            iter_total: 0,
            iter_max: 0,
            #[cfg(feature = "subrate-spike")]
            subrate: cfg.subrate.max(1),
            #[cfg(feature = "subrate-spike")]
            subrate_hold: cfg.subrate_hold,
            #[cfg(feature = "subrate-spike")]
            subrate_phase: 0,
            #[cfg(feature = "subrate-spike")]
            subrate_valid: false,
            #[cfg(feature = "subrate-spike")]
            capture_j: false,
            #[cfg(feature = "subrate-spike")]
            j_captured: false,
            #[cfg(feature = "subrate-spike")]
            j_frozen_raw: [0.0; NUM_NODES * NUM_NODES],
            #[cfg(feature = "subrate-spike")]
            j_dev: [0.0; NUM_NODES * NUM_NODES],
            #[cfg(feature = "subrate-spike")]
            lu_frozen: [0.0; NUM_NODES * NUM_NODES],
            #[cfg(feature = "subrate-spike")]
            piv_frozen: [0; NUM_NODES],
            #[cfg(feature = "subrate-spike")]
            i0_const: [0.0; NUM_NODES],
        };
        model.build_g0();
        model.build_schur_partition(); // sets n/l partition; runs first Schur prep if enabled
        model
    }

    /// Construct at a **host** sample rate (internal solve rate = `host_sr * OS`) and configure the
    /// 4th-order Butterworth decimation filters (two biquads, `fc = 0.45*host_sr`, C++ Q values).
    /// Mirrors the C++ offline driver setup (`main.cpp:859-936`).
    pub fn new(host_sr: f64, rgain: f64, bias: f64) -> Self {
        Self::new_with(QualityConfig::realtime(), host_sr, rgain, bias)
    }

    /// Like [`new`], but with an explicit [`QualityConfig`] (tier/lever selection).
    pub fn new_with(cfg: QualityConfig, host_sr: f64, rgain: f64, bias: f64) -> Self {
        // Use the same effective os as build_with so dt (= 1/internal_sr) matches the per-host-sample
        // subsample count.
        let internal_sr = host_sr * cfg.solve_os() as f64;
        let mut m = Self::build_with(cfg, internal_sr, rgain, bias);
        // Decimation biquads are always configured, used when `cfg.decimate` (see
        // `process_host_sample`). At os=2 they are the antialiasing decimator and stay on; configuring
        // them unconditionally keeps the path branch-free.
        let fc = 0.45 * host_sr;
        m.dec1.set_lowpass(fc, internal_sr, 0.54119610);
        m.dec2.set_lowpass(fc, internal_sr, 1.30656296);
        m
    }

    /// Precompute the constant linear conductance matrix G0 (resistors + cap Geq + coupled-inductor
    /// companion). Only BJT entries change per Newton iteration (`main.cpp:413-428`).
    pub(in crate::wdf) fn build_g0(&mut self) {
        use node::{N_PA, N_SA};
        let u = self.u;
        for x in self.g0[..u * u].iter_mut() {
            *x = 0.0;
        }
        // stamp a symmetric linear conductance G between nodes a, b
        let stamp_lin = |g0: &mut [f64], ra: i32, rb: i32, g: f64| {
            if ra >= 0 {
                g0[ra as usize * u + ra as usize] += g;
                if rb >= 0 {
                    g0[ra as usize * u + rb as usize] -= g;
                }
            }
            if rb >= 0 {
                g0[rb as usize * u + rb as usize] += g;
                if ra >= 0 {
                    g0[rb as usize * u + ra as usize] -= g;
                }
            }
        };
        for i in 0..self.n_res {
            let r = self.res[i];
            let (ra, rb) = (self.ur(r.a), self.ur(r.b));
            stamp_lin(&mut self.g0, ra, rb, r.g);
        }
        for i in 0..self.n_caps {
            let c = self.caps[i];
            let (ra, rb) = (self.ur(c.a), self.ur(c.b));
            stamp_lin(&mut self.g0, ra, rb, c.geq);
        }
        // coupled inductor (asymmetric)
        let r_p = self.ur(N_PA);
        let r_s = self.ur(N_SA);
        if r_p >= 0 {
            self.g0[r_p as usize * u + r_p as usize] += self.gl[0][0];
            if r_s >= 0 {
                self.g0[r_p as usize * u + r_s as usize] += self.gl[0][1];
            }
        }
        if r_s >= 0 {
            if r_p >= 0 {
                self.g0[r_s as usize * u + r_p as usize] += self.gl[1][0];
            }
            self.g0[r_s as usize * u + r_s as usize] += self.gl[1][1];
        }
        // G0 is constant across a sample's Newton iters (changes only on set_bias/set_rgain),
        // so refresh the Schur per-sample factorization here. Guarded until the partition
        // exists (the first build_g0 in build_with runs before build_schur_partition).
        if self.solver == Solver::Schur && self.n_sz > 0 {
            self.prepare_schur();
        }
    }
}
