use super::*;
use super::topology_gen::{build_topology, Topology};
// Node constants are referenced only by the JFET voicing branch below (the IR-generated
// topology wires the stock BJTs by name); gate the import so the no-jfet build is warning-clean.
#[cfg(feature = "jfet")]
use super::node::*;

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
        let dt = 1.0 / internal_sr;

        // Topology (nodes, elements, materials, BJT wiring, transformer companion) is generated
        // from circuit/ba283.toml by tools/circuitgen — see topology_gen.rs. The solver, levers,
        // LUTs, aging and JA core below stay hand-written.
        let Topology {
            urow,
            u,
            res,
            n_res,
            res_base_g,
            res_material,
            caps,
            n_caps,
            cap_base_far,
            cap_material,
            bjts: topo_bjts,
            r_kj_idx,
            r_upper_idx,
            r_lower_idx,
            lp,
            ls,
            k,
            gl,
        } = build_topology(cfg.cheap_exp, dt, rgain, bias);

        // JFET voicing (feature `jfet`, cfg.jfet): the three transistor slots hold gate-junction
        // cards (Cgs/Cgd + gate leakage diode) and the square-law channel params are gathered into
        // `jfet_params`. `cfg.jfet=false` (and the no-feature build) keep the generated BJT cards
        // straight from the IR-derived topology (bit-exact with the pre-codegen build).
        #[cfg(feature = "jfet")]
        let (bjts, jfet_params) = if cfg.jfet {
            let (gc_s, jx_s) = jfet::jfet_small();
            let (gc_p, jx_p) = jfet::jfet_power();
            let mut s = gc_s;
            let mut p3 = gc_p;
            s.cheap_exp = cfg.cheap_exp;
            p3.cheap_exp = cfg.cheap_exp;
            (
                [
                    Bjt::new(N_TR1_B, N_TR1_E, N_TR1_C, s),
                    Bjt::new(N_TR1_C, N_TR2_E, N_F, s),
                    Bjt::new(N_TR2_E, N_TR3_E, N_F, p3),
                ],
                [jx_s, jx_s, jx_p],
            )
        } else {
            (topo_bjts, [JfetExtra::default(); 3])
        };
        #[cfg(not(feature = "jfet"))]
        let bjts = topo_bjts;

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
            #[cfg(feature = "jfet")]
            jfet: cfg.jfet,
            #[cfg(feature = "jfet")]
            jfet_params,
            r_kj_idx,
            r_upper_idx,
            r_lower_idx,
            res_base_g,
            cap_base_far,
            age_pattern: AgePattern::identity(),
            age: 0.0,
            age_seed: 0,
            res_material,
            cap_material,
            tol_grade: TolGrade::Standard,
            lp_base: lp,
            ls_base: ls,
            k_coupl_base: k,
            gl,
            i_hist_p: 0.0,
            i_hist_s: 0.0,
            v_pprev: 0.0,
            v_sprev: 0.0,
            #[cfg(feature = "trafo-core")]
            trafo_hyst: 0.5,
            #[cfg(feature = "trafo-core")]
            trafo_sat: 0.5,
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
            fir: None, // set in `new_with` (the only place host_sr/os are known together)
            legacy_resample: cfg.legacy_resample,
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
        // Legacy decimator biquads: 4th-order Butterworth at fc=0.45·host_sr (the C++ golden path).
        // Used only when `legacy_resample`; configured unconditionally so that path stays branch-free.
        let fc = 0.45 * host_sr;
        m.dec1.set_lowpass(fc, internal_sr, 0.54119610);
        m.dec2.set_lowpass(fc, internal_sr, 1.30656296);
        // Shipped path: steep linear-phase FIR up/down, built only when there is something to
        // resample (`os > 1`) and we are not in the golden-parity legacy mode.
        if !cfg.legacy_resample && cfg.solve_os() > 1 {
            m.fir = Some(FirResampler::new(cfg.solve_os()));
        }
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
