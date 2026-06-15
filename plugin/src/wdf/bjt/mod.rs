use super::*;

/// Gummel-Poon terminal currents (into the device) + node-form Jacobian. With Early effect
/// (VAF/VAR) and high-injection (IKF/IKR); reduces to plain transport when those are disabled
/// (`main.cpp:192-241`). `itf/gtf/itr/gtr` are returned for the diffusion-charge caps.
#[derive(Clone, Copy, Debug, Default)]
pub struct BjtOut {
    pub ib: f64,
    pub ic: f64,
    pub ie: f64,
    pub itf: f64,
    pub gtf: f64,
    pub itr: f64,
    pub gtr: f64,
    pub dib_b: f64,
    pub dib_e: f64,
    pub dib_c: f64,
    pub dic_b: f64,
    pub dic_e: f64,
    pub dic_c: f64,
    pub die_b: f64,
    pub die_e: f64,
    pub die_c: f64,
}

/// Compute Gummel-Poon BJT response (`main.cpp:197-241`). `sqrt` is exact (not fast math),
/// matching the C++ `std::sqrt`.
pub fn compute_bjt(vbe: f64, vbc: f64, p: &BjtParams) -> BjtOut {
    let mut o = BjtOut::default();
    let (itf, gtf) = diode_branch(vbe, p.is, p.vt, p.vcrit, p.cheap_exp); // forward transport + d/dvbe
    let (itr, gtr) = diode_branch(vbc, p.is, p.vt, p.vcrit, p.cheap_exp); // reverse transport + d/dvbc
    o.itf = itf;
    o.gtf = gtf;
    o.itr = itr;
    o.gtr = gtr;

    // base-charge factor qb = (q1/2)(1+sqrt(1+4 q2)); q1=Early, q2=high-injection
    let inv_af = if p.vaf > 0.0 { 1.0 / p.vaf } else { 0.0 };
    let inv_ar = if p.var > 0.0 { 1.0 / p.var } else { 0.0 };
    let mut inv1 = 1.0 - vbc * inv_af - vbe * inv_ar;
    if inv1 < 0.1 {
        inv1 = 0.1; // guard against blow-up in saturation
    }
    let q1 = 1.0 / inv1;
    let dq1_vbe = q1 * q1 * inv_ar; // d(1/inv1)/dvbe = q1^2 * inv_ar
    let dq1_vbc = q1 * q1 * inv_af;
    let inv_kf = if p.ikf > 0.0 { 1.0 / p.ikf } else { 0.0 };
    let inv_kr = if p.ikr > 0.0 { 1.0 / p.ikr } else { 0.0 };
    let q2 = itf * inv_kf + itr * inv_kr;
    let dq2_vbe = gtf * inv_kf;
    let dq2_vbc = gtr * inv_kr;
    let s = (1.0 + 4.0 * q2).sqrt();
    let qb = q1 * (1.0 + s) / 2.0;
    let dqb_vbe = dq1_vbe * (1.0 + s) / 2.0 + q1 * dq2_vbe / s;
    let dqb_vbc = dq1_vbc * (1.0 + s) / 2.0 + q1 * dq2_vbc / s;

    let diff = itf - itr;
    let it = diff / qb;
    let dit_vbe = (gtf * qb - diff * dqb_vbe) / (qb * qb);
    let dit_vbc = (-gtr * qb - diff * dqb_vbc) / (qb * qb);

    let inv_bf = 1.0 / p.bf;
    let inv_br = 1.0 / p.br;
    let ic = it - itr * inv_br;
    let ib = itf * inv_bf + itr * inv_br;
    let ie = -(ic + ib);
    o.ic = ic;
    o.ib = ib;
    o.ie = ie;

    // derivatives wrt vbe, vbc
    let dic_vbe = dit_vbe;
    let dic_vbc = dit_vbc - gtr * inv_br;
    let dib_vbe = gtf * inv_bf;
    let dib_vbc = gtr * inv_br;
    let die_vbe = -(dic_vbe + dib_vbe);
    let die_vbc = -(dic_vbc + dib_vbc);
    // node form: d/dVb = d/dvbe + d/dvbc, d/dVe = -d/dvbe, d/dVc = -d/dvbc
    o.dic_b = dic_vbe + dic_vbc;
    o.dic_e = -dic_vbe;
    o.dic_c = -dic_vbc;
    o.dib_b = dib_vbe + dib_vbc;
    o.dib_e = -dib_vbe;
    o.dib_c = -dib_vbc;
    o.die_b = die_vbe + die_vbc;
    o.die_e = -die_vbe;
    o.die_c = -die_vbc;
    o
}

/// Like [`compute_bjt`] but takes pre-computed `exp(vbe/vt)`, `exp(vbc/vt)`,
/// `exp(vcrit/vt)` — call after gathering ef4/er4/ec4 with `exp_bl` so LLVM
/// can SLP-fuse 3 independent exp_bl calls into AVX2 YMM ops before this
/// per-BJT arithmetic path. `#[inline(always)]` lets LLVM keep BjtOut in
/// registers (same behavior as the inlined baseline `compute_bjt`).
/// Valid on `EXACT=false, cheap_exp=false` path — caller is responsible.
/// Generic over `const FMA: bool`: `FMA=false` is bit-identical to the pre-T3 scalar reference
/// (every conversion is `coeff + a*b` ≡ `a*b + coeff`, or `c - a*b` ≡ `(-a)*b + c`, both exact
/// under FP add-commutativity / exact negation). `FMA=true` fuses to `vfmadd` (lossy fast/ultra).
///
/// Production-dead since the post-exp algebra was width-4 batched into [`compute_bjt_preexp_batch`];
/// retained under `#[cfg(test)]` as the scalar reference for `compute_bjt_preexp_batch_is_bit_exact`.
#[cfg(test)]
#[inline(always)]
fn compute_bjt_preexp<const FMA: bool>(vbe: f64, vbc: f64, p: &BjtParams, ef: f64, er: f64, ec: f64) -> BjtOut {
    let inv_vt_is = p.is / p.vt;
    let (itf, gtf) = if vbe < p.vcrit {
        (p.is * (ef - 1.0), inv_vt_is * ef)
    } else {
        let icrit = p.is * (ec - 1.0);
        let gcrit = inv_vt_is * ec;
        (fma::<FMA>(gcrit, vbe - p.vcrit, icrit), gcrit) // icrit + gcrit*(vbe-vcrit)
    };
    let (itr, gtr) = if vbc < p.vcrit {
        (p.is * (er - 1.0), inv_vt_is * er)
    } else {
        let icrit = p.is * (ec - 1.0);
        let gcrit = inv_vt_is * ec;
        (fma::<FMA>(gcrit, vbc - p.vcrit, icrit), gcrit) // icrit + gcrit*(vbc-vcrit)
    };
    let inv_af = if p.vaf > 0.0 { 1.0 / p.vaf } else { 0.0 };
    let inv_ar = if p.var > 0.0 { 1.0 / p.var } else { 0.0 };
    // 1.0 - vbc*inv_af - vbe*inv_ar = (1 - vbc*inv_af) - vbe*inv_ar
    let mut inv1 = fma::<FMA>(vbe, -inv_ar, fma::<FMA>(vbc, -inv_af, 1.0));
    if inv1 < 0.1 { inv1 = 0.1; }
    let q1 = 1.0 / inv1;
    let dq1_vbe = q1 * q1 * inv_ar;
    let dq1_vbc = q1 * q1 * inv_af;
    let inv_kf = if p.ikf > 0.0 { 1.0 / p.ikf } else { 0.0 };
    let inv_kr = if p.ikr > 0.0 { 1.0 / p.ikr } else { 0.0 };
    let q2 = fma::<FMA>(itf, inv_kf, itr * inv_kr); // itf*inv_kf + itr*inv_kr
    let dq2_vbe = gtf * inv_kf;
    let dq2_vbc = gtr * inv_kr;
    let s = fma::<FMA>(4.0, q2, 1.0).sqrt(); // (1 + 4*q2).sqrt()
    let qb = q1 * (1.0 + s) / 2.0;
    let dqb_vbe = dq1_vbe * (1.0 + s) / 2.0 + q1 * dq2_vbe / s;
    let dqb_vbc = dq1_vbc * (1.0 + s) / 2.0 + q1 * dq2_vbc / s;
    let diff = itf - itr;
    let it = diff / qb;
    let qbsq = qb * qb;
    let dit_vbe = fma::<FMA>(diff, -dqb_vbe, gtf * qb) / qbsq; // (gtf*qb - diff*dqb_vbe)/qbsq
    let dit_vbc = fma::<FMA>(diff, -dqb_vbc, -gtr * qb) / qbsq; // (-gtr*qb - diff*dqb_vbc)/qbsq
    let inv_bf = 1.0 / p.bf;
    let inv_br = 1.0 / p.br;
    let ic = fma::<FMA>(itr, -inv_br, it); // it - itr*inv_br
    let ib = fma::<FMA>(itf, inv_bf, itr * inv_br); // itf*inv_bf + itr*inv_br
    let ie = -(ic + ib);
    let dic_vbe = dit_vbe;
    let dic_vbc = fma::<FMA>(gtr, -inv_br, dit_vbc); // dit_vbc - gtr*inv_br
    let dib_vbe = gtf * inv_bf;
    let dib_vbc = gtr * inv_br;
    let die_vbe = -(dic_vbe + dib_vbe);
    let die_vbc = -(dic_vbc + dib_vbc);
    BjtOut {
        ib, ic, ie, itf, gtf, itr, gtr,
        dic_b: dic_vbe + dic_vbc, dic_e: -dic_vbe, dic_c: -dic_vbc,
        dib_b: dib_vbe + dib_vbc, dib_e: -dib_vbe, dib_c: -dib_vbc,
        die_b: die_vbe + die_vbc, die_e: -die_vbe, die_c: -die_vbc,
    }
}

/// Width-4 SoA mirror of [`compute_bjt_preexp`] — the post-exp Gummel-Poon algebra (currents +
/// 16-field Jacobian) for 3 BJTs at once (lane 3 = padding, never scattered). Reuses the
/// caller's already-batched `ef4/er4/ec4` (NO exp recompute — the reverted `b6e5d59` diode batch
/// lost precisely because it doubled the exp work; this batches only the divide-/sqrt-heavy tail).
/// Params are extracted into `[f64;4]` SoA (stride-8 loads) so LLVM SLP-fuses the 4 lanes into
/// AVX2 packed `vdivpd`/`vsqrtpd`/`vmulpd`. Bit-exact with `compute_bjt_preexp` at `FMA=false`
/// (same per-lane op tree); guarded by `compute_bjt_preexp_batch_is_bit_exact`.
#[inline]
pub(in crate::wdf) fn compute_bjt_preexp_batch<const FMA: bool>(
    vbe: &[f64; 4],
    vbc: &[f64; 4],
    bjts: &[Bjt; 3],
    ef: &[f64; 4],
    er: &[f64; 4],
    ec: &[f64; 4],
) -> BjtOutSoA {
    // Extract params to SoA. Lane 3 = safe defaults (finite, never scattered).
    let is4    = [bjts[0].p.is,    bjts[1].p.is,    bjts[2].p.is,    1e-14];
    let vt4    = [bjts[0].p.vt,    bjts[1].p.vt,    bjts[2].p.vt,    0.02585];
    let vcrit4 = [bjts[0].p.vcrit, bjts[1].p.vcrit, bjts[2].p.vcrit, 0.0];
    let vaf4   = [bjts[0].p.vaf,   bjts[1].p.vaf,   bjts[2].p.vaf,   0.0];
    let var4   = [bjts[0].p.var,   bjts[1].p.var,   bjts[2].p.var,   0.0];
    let ikf4   = [bjts[0].p.ikf,   bjts[1].p.ikf,   bjts[2].p.ikf,   0.0];
    let ikr4   = [bjts[0].p.ikr,   bjts[1].p.ikr,   bjts[2].p.ikr,   0.0];
    let bf4    = [bjts[0].p.bf,    bjts[1].p.bf,    bjts[2].p.bf,    1.0];
    let br4    = [bjts[0].p.br,    bjts[1].p.br,    bjts[2].p.br,    1.0];

    let mut o = BjtOutSoA::default();
    for l in 0..4 {
        let inv_vt_is = is4[l] / vt4[l];
        let icrit = is4[l] * (ec[l] - 1.0);
        let gcrit = inv_vt_is * ec[l];
        let (itf, gtf) = if vbe[l] < vcrit4[l] {
            (is4[l] * (ef[l] - 1.0), inv_vt_is * ef[l])
        } else {
            (fma::<FMA>(gcrit, vbe[l] - vcrit4[l], icrit), gcrit)
        };
        let (itr, gtr) = if vbc[l] < vcrit4[l] {
            (is4[l] * (er[l] - 1.0), inv_vt_is * er[l])
        } else {
            (fma::<FMA>(gcrit, vbc[l] - vcrit4[l], icrit), gcrit)
        };
        let inv_af = if vaf4[l] > 0.0 { 1.0 / vaf4[l] } else { 0.0 };
        let inv_ar = if var4[l] > 0.0 { 1.0 / var4[l] } else { 0.0 };
        let mut inv1 = fma::<FMA>(vbe[l], -inv_ar, fma::<FMA>(vbc[l], -inv_af, 1.0));
        if inv1 < 0.1 { inv1 = 0.1; }
        let q1 = 1.0 / inv1;
        let dq1_vbe = q1 * q1 * inv_ar;
        let dq1_vbc = q1 * q1 * inv_af;
        let inv_kf = if ikf4[l] > 0.0 { 1.0 / ikf4[l] } else { 0.0 };
        let inv_kr = if ikr4[l] > 0.0 { 1.0 / ikr4[l] } else { 0.0 };
        let q2 = fma::<FMA>(itf, inv_kf, itr * inv_kr);
        let dq2_vbe = gtf * inv_kf;
        let dq2_vbc = gtr * inv_kr;
        let s = fma::<FMA>(4.0, q2, 1.0).sqrt();
        let qb = q1 * (1.0 + s) / 2.0;
        let dqb_vbe = dq1_vbe * (1.0 + s) / 2.0 + q1 * dq2_vbe / s;
        let dqb_vbc = dq1_vbc * (1.0 + s) / 2.0 + q1 * dq2_vbc / s;
        let diff = itf - itr;
        let it = diff / qb;
        let qbsq = qb * qb;
        let dit_vbe = fma::<FMA>(diff, -dqb_vbe, gtf * qb) / qbsq;
        let dit_vbc = fma::<FMA>(diff, -dqb_vbc, -gtr * qb) / qbsq;
        let inv_bf = 1.0 / bf4[l];
        let inv_br = 1.0 / br4[l];
        let ic = fma::<FMA>(itr, -inv_br, it);
        let ib = fma::<FMA>(itf, inv_bf, itr * inv_br);
        let ie = -(ic + ib);
        let dic_vbe = dit_vbe;
        let dic_vbc = fma::<FMA>(gtr, -inv_br, dit_vbc);
        let dib_vbe = gtf * inv_bf;
        let dib_vbc = gtr * inv_br;
        let die_vbe = -(dic_vbe + dib_vbe);
        let die_vbc = -(dic_vbc + dib_vbc);
        o.ib[l] = ib; o.ic[l] = ic; o.ie[l] = ie;
        o.itf[l] = itf; o.gtf[l] = gtf; o.itr[l] = itr; o.gtr[l] = gtr;
        o.dic_b[l] = dic_vbe + dic_vbc; o.dic_e[l] = -dic_vbe; o.dic_c[l] = -dic_vbc;
        o.dib_b[l] = dib_vbe + dib_vbc; o.dib_e[l] = -dib_vbe; o.dib_c[l] = -dib_vbc;
        o.die_b[l] = die_vbe + die_vbc; o.die_e[l] = -die_vbe; o.die_c[l] = -die_vbc;
    }
    o
}

// ---------------------------------------------------------------------------
// Width-4 SoA batch device eval. Lane 3 = padding (vbe=vbc=0, params zeroed);
// only lanes 0-2 are ever scattered. Bit-exact with the scalar path on EXACT=false.
// Kept for test coverage; hot path uses compute_bjt_preexp + inline exp4 batching.
// ---------------------------------------------------------------------------

/// SoA output for a width-4 [`compute_bjt`] batch. Mirrors all 16 [`BjtOut`] fields.
/// Produced by `compute_bjt_preexp_batch` on the production path and by `compute_bjt_batch`
/// under `#[cfg(test)]` for its contract guard.
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::wdf) struct BjtOutSoA {
    pub(in crate::wdf) ib:    [f64; 4],
    pub(in crate::wdf) ic:    [f64; 4],
    pub(in crate::wdf) ie:    [f64; 4],
    pub(in crate::wdf) itf:   [f64; 4],
    pub(in crate::wdf) gtf:   [f64; 4],
    pub(in crate::wdf) itr:   [f64; 4],
    pub(in crate::wdf) gtr:   [f64; 4],
    pub(in crate::wdf) dib_b: [f64; 4],
    pub(in crate::wdf) dib_e: [f64; 4],
    pub(in crate::wdf) dib_c: [f64; 4],
    pub(in crate::wdf) dic_b: [f64; 4],
    pub(in crate::wdf) dic_e: [f64; 4],
    pub(in crate::wdf) dic_c: [f64; 4],
    pub(in crate::wdf) die_b: [f64; 4],
    pub(in crate::wdf) die_e: [f64; 4],
    pub(in crate::wdf) die_c: [f64; 4],
}

/// Evaluate 3 BJTs as a width-4 SoA batch (lane 3 = padding, never scattered).
///
/// Params are extracted into [f64;4] SoA arrays (contiguous stride=8) before
/// any arithmetic so LLVM can use vmovupd for loads and SLP-fuse the 4
/// independent computation chains into AVX2 ymm ops. Takes `&[Bjt;3]` directly
/// to avoid the ~832-byte intermediate `[BjtParams;4]` copy in `newton_iterate`.
/// Bit-exact with [`compute_bjt`] on the `EXACT=false, cheap_exp=false` path.
/// Production path uses `exp4` + [`compute_bjt_preexp_batch`] (reusing the already-batched exp).
/// This full-batch variant (which recomputes exp internally) is retained under `#[cfg(test)]`
/// for the `compute_bjt_batch_is_bit_exact` SoA contract guard.
#[cfg(test)]
#[inline(never)]
fn compute_bjt_batch(vbe: &[f64; 4], vbc: &[f64; 4], bjts: &[Bjt; 3]) -> BjtOutSoA {
    // Extract to [f64;4] SoA. AoS stride≈208 bytes blocks SLP; SoA stride=8 enables it.
    // Lane 3 = padding (0 / safe defaults), never scattered.
    let is4    = [bjts[0].p.is,    bjts[1].p.is,    bjts[2].p.is,    1e-14]; // safe: exp(v/vt) finite
    let vt4    = [bjts[0].p.vt,    bjts[1].p.vt,    bjts[2].p.vt,    0.02585];
    let vcrit4 = [bjts[0].p.vcrit, bjts[1].p.vcrit, bjts[2].p.vcrit, 0.0];
    let vaf4   = [bjts[0].p.vaf,   bjts[1].p.vaf,   bjts[2].p.vaf,   0.0];
    let var4   = [bjts[0].p.var,   bjts[1].p.var,   bjts[2].p.var,   0.0];
    let ikf4   = [bjts[0].p.ikf,   bjts[1].p.ikf,   bjts[2].p.ikf,   0.0];
    let ikr4   = [bjts[0].p.ikr,   bjts[1].p.ikr,   bjts[2].p.ikr,   0.0];
    let bf4    = [bjts[0].p.bf,    bjts[1].p.bf,    bjts[2].p.bf,    1.0]; // safe: 1/bf finite
    let br4    = [bjts[0].p.br,    bjts[1].p.br,    bjts[2].p.br,    1.0];

    // Transcendentals: 4 independent exp_bl calls, SLP-fused → vmulpd/vaddpd chains.
    let ef = [exp_bl::<false>(vbe[0]/vt4[0]), exp_bl::<false>(vbe[1]/vt4[1]), exp_bl::<false>(vbe[2]/vt4[2]), exp_bl::<false>(vbe[3]/vt4[3])];
    let er = [exp_bl::<false>(vbc[0]/vt4[0]), exp_bl::<false>(vbc[1]/vt4[1]), exp_bl::<false>(vbc[2]/vt4[2]), exp_bl::<false>(vbc[3]/vt4[3])];
    let ec = [exp_bl::<false>(vcrit4[0]/vt4[0]), exp_bl::<false>(vcrit4[1]/vt4[1]), exp_bl::<false>(vcrit4[2]/vt4[2]), exp_bl::<false>(vcrit4[3]/vt4[3])];

    let icrit = [is4[0]*(ec[0]-1.0), is4[1]*(ec[1]-1.0), is4[2]*(ec[2]-1.0), is4[3]*(ec[3]-1.0)];
    let isovt = [is4[0]/vt4[0],      is4[1]/vt4[1],      is4[2]/vt4[2],      is4[3]/vt4[3]];
    let gcrit = [isovt[0]*ec[0],      isovt[1]*ec[1],     isovt[2]*ec[2],     isovt[3]*ec[3]];
    let itf = [
        if vbe[0] < vcrit4[0] { is4[0]*(ef[0]-1.0) } else { icrit[0]+gcrit[0]*(vbe[0]-vcrit4[0]) },
        if vbe[1] < vcrit4[1] { is4[1]*(ef[1]-1.0) } else { icrit[1]+gcrit[1]*(vbe[1]-vcrit4[1]) },
        if vbe[2] < vcrit4[2] { is4[2]*(ef[2]-1.0) } else { icrit[2]+gcrit[2]*(vbe[2]-vcrit4[2]) },
        if vbe[3] < vcrit4[3] { is4[3]*(ef[3]-1.0) } else { icrit[3]+gcrit[3]*(vbe[3]-vcrit4[3]) },
    ];
    let gtf = [
        if vbe[0] < vcrit4[0] { isovt[0]*ef[0] } else { gcrit[0] },
        if vbe[1] < vcrit4[1] { isovt[1]*ef[1] } else { gcrit[1] },
        if vbe[2] < vcrit4[2] { isovt[2]*ef[2] } else { gcrit[2] },
        if vbe[3] < vcrit4[3] { isovt[3]*ef[3] } else { gcrit[3] },
    ];
    let itr = [
        if vbc[0] < vcrit4[0] { is4[0]*(er[0]-1.0) } else { icrit[0]+gcrit[0]*(vbc[0]-vcrit4[0]) },
        if vbc[1] < vcrit4[1] { is4[1]*(er[1]-1.0) } else { icrit[1]+gcrit[1]*(vbc[1]-vcrit4[1]) },
        if vbc[2] < vcrit4[2] { is4[2]*(er[2]-1.0) } else { icrit[2]+gcrit[2]*(vbc[2]-vcrit4[2]) },
        if vbc[3] < vcrit4[3] { is4[3]*(er[3]-1.0) } else { icrit[3]+gcrit[3]*(vbc[3]-vcrit4[3]) },
    ];
    let gtr = [
        if vbc[0] < vcrit4[0] { isovt[0]*er[0] } else { gcrit[0] },
        if vbc[1] < vcrit4[1] { isovt[1]*er[1] } else { gcrit[1] },
        if vbc[2] < vcrit4[2] { isovt[2]*er[2] } else { gcrit[2] },
        if vbc[3] < vcrit4[3] { isovt[3]*er[3] } else { gcrit[3] },
    ];

    let inv_af = [
        if vaf4[0] > 0.0 { 1.0/vaf4[0] } else { 0.0 },
        if vaf4[1] > 0.0 { 1.0/vaf4[1] } else { 0.0 },
        if vaf4[2] > 0.0 { 1.0/vaf4[2] } else { 0.0 },
        if vaf4[3] > 0.0 { 1.0/vaf4[3] } else { 0.0 },
    ];
    let inv_ar = [
        if var4[0] > 0.0 { 1.0/var4[0] } else { 0.0 },
        if var4[1] > 0.0 { 1.0/var4[1] } else { 0.0 },
        if var4[2] > 0.0 { 1.0/var4[2] } else { 0.0 },
        if var4[3] > 0.0 { 1.0/var4[3] } else { 0.0 },
    ];
    let inv1 = [
        (1.0 - vbc[0]*inv_af[0] - vbe[0]*inv_ar[0]).max(0.1),
        (1.0 - vbc[1]*inv_af[1] - vbe[1]*inv_ar[1]).max(0.1),
        (1.0 - vbc[2]*inv_af[2] - vbe[2]*inv_ar[2]).max(0.1),
        (1.0 - vbc[3]*inv_af[3] - vbe[3]*inv_ar[3]).max(0.1),
    ];
    let q1 = [1.0/inv1[0], 1.0/inv1[1], 1.0/inv1[2], 1.0/inv1[3]];
    let dq1_vbe = [q1[0]*q1[0]*inv_ar[0], q1[1]*q1[1]*inv_ar[1], q1[2]*q1[2]*inv_ar[2], q1[3]*q1[3]*inv_ar[3]];
    let dq1_vbc = [q1[0]*q1[0]*inv_af[0], q1[1]*q1[1]*inv_af[1], q1[2]*q1[2]*inv_af[2], q1[3]*q1[3]*inv_af[3]];
    let inv_kf = [
        if ikf4[0] > 0.0 { 1.0/ikf4[0] } else { 0.0 },
        if ikf4[1] > 0.0 { 1.0/ikf4[1] } else { 0.0 },
        if ikf4[2] > 0.0 { 1.0/ikf4[2] } else { 0.0 },
        if ikf4[3] > 0.0 { 1.0/ikf4[3] } else { 0.0 },
    ];
    let inv_kr = [
        if ikr4[0] > 0.0 { 1.0/ikr4[0] } else { 0.0 },
        if ikr4[1] > 0.0 { 1.0/ikr4[1] } else { 0.0 },
        if ikr4[2] > 0.0 { 1.0/ikr4[2] } else { 0.0 },
        if ikr4[3] > 0.0 { 1.0/ikr4[3] } else { 0.0 },
    ];
    let q2 = [
        itf[0]*inv_kf[0]+itr[0]*inv_kr[0], itf[1]*inv_kf[1]+itr[1]*inv_kr[1],
        itf[2]*inv_kf[2]+itr[2]*inv_kr[2], itf[3]*inv_kf[3]+itr[3]*inv_kr[3],
    ];
    let dq2_vbe = [gtf[0]*inv_kf[0], gtf[1]*inv_kf[1], gtf[2]*inv_kf[2], gtf[3]*inv_kf[3]];
    let dq2_vbc = [gtr[0]*inv_kr[0], gtr[1]*inv_kr[1], gtr[2]*inv_kr[2], gtr[3]*inv_kr[3]];
    let s = [(1.0+4.0*q2[0]).sqrt(), (1.0+4.0*q2[1]).sqrt(), (1.0+4.0*q2[2]).sqrt(), (1.0+4.0*q2[3]).sqrt()];
    let qb = [q1[0]*(1.0+s[0])/2.0, q1[1]*(1.0+s[1])/2.0, q1[2]*(1.0+s[2])/2.0, q1[3]*(1.0+s[3])/2.0];
    let dqb_vbe = [
        dq1_vbe[0]*(1.0+s[0])/2.0 + q1[0]*dq2_vbe[0]/s[0],
        dq1_vbe[1]*(1.0+s[1])/2.0 + q1[1]*dq2_vbe[1]/s[1],
        dq1_vbe[2]*(1.0+s[2])/2.0 + q1[2]*dq2_vbe[2]/s[2],
        dq1_vbe[3]*(1.0+s[3])/2.0 + q1[3]*dq2_vbe[3]/s[3],
    ];
    let dqb_vbc = [
        dq1_vbc[0]*(1.0+s[0])/2.0 + q1[0]*dq2_vbc[0]/s[0],
        dq1_vbc[1]*(1.0+s[1])/2.0 + q1[1]*dq2_vbc[1]/s[1],
        dq1_vbc[2]*(1.0+s[2])/2.0 + q1[2]*dq2_vbc[2]/s[2],
        dq1_vbc[3]*(1.0+s[3])/2.0 + q1[3]*dq2_vbc[3]/s[3],
    ];
    let diff = [itf[0]-itr[0], itf[1]-itr[1], itf[2]-itr[2], itf[3]-itr[3]];
    let it   = [diff[0]/qb[0], diff[1]/qb[1], diff[2]/qb[2], diff[3]/qb[3]];
    let qbsq = [qb[0]*qb[0],   qb[1]*qb[1],   qb[2]*qb[2],   qb[3]*qb[3]];
    let dit_vbe = [
        (gtf[0]*qb[0] - diff[0]*dqb_vbe[0]) / qbsq[0],
        (gtf[1]*qb[1] - diff[1]*dqb_vbe[1]) / qbsq[1],
        (gtf[2]*qb[2] - diff[2]*dqb_vbe[2]) / qbsq[2],
        (gtf[3]*qb[3] - diff[3]*dqb_vbe[3]) / qbsq[3],
    ];
    let dit_vbc = [
        (-gtr[0]*qb[0] - diff[0]*dqb_vbc[0]) / qbsq[0],
        (-gtr[1]*qb[1] - diff[1]*dqb_vbc[1]) / qbsq[1],
        (-gtr[2]*qb[2] - diff[2]*dqb_vbc[2]) / qbsq[2],
        (-gtr[3]*qb[3] - diff[3]*dqb_vbc[3]) / qbsq[3],
    ];
    let inv_bf  = [1.0/bf4[0], 1.0/bf4[1], 1.0/bf4[2], 1.0/bf4[3]];
    let inv_br  = [1.0/br4[0], 1.0/br4[1], 1.0/br4[2], 1.0/br4[3]];
    let ic = [it[0]-itr[0]*inv_br[0], it[1]-itr[1]*inv_br[1], it[2]-itr[2]*inv_br[2], it[3]-itr[3]*inv_br[3]];
    let ib = [
        itf[0]*inv_bf[0]+itr[0]*inv_br[0], itf[1]*inv_bf[1]+itr[1]*inv_br[1],
        itf[2]*inv_bf[2]+itr[2]*inv_br[2], itf[3]*inv_bf[3]+itr[3]*inv_br[3],
    ];
    let ie = [-(ic[0]+ib[0]), -(ic[1]+ib[1]), -(ic[2]+ib[2]), -(ic[3]+ib[3])];
    let dic_vbe = dit_vbe;
    let dic_vbc = [dit_vbc[0]-gtr[0]*inv_br[0], dit_vbc[1]-gtr[1]*inv_br[1], dit_vbc[2]-gtr[2]*inv_br[2], dit_vbc[3]-gtr[3]*inv_br[3]];
    let dib_vbe = [gtf[0]*inv_bf[0], gtf[1]*inv_bf[1], gtf[2]*inv_bf[2], gtf[3]*inv_bf[3]];
    let dib_vbc = [gtr[0]*inv_br[0], gtr[1]*inv_br[1], gtr[2]*inv_br[2], gtr[3]*inv_br[3]];
    let die_vbe = [-(dic_vbe[0]+dib_vbe[0]), -(dic_vbe[1]+dib_vbe[1]), -(dic_vbe[2]+dib_vbe[2]), -(dic_vbe[3]+dib_vbe[3])];
    let die_vbc = [-(dic_vbc[0]+dib_vbc[0]), -(dic_vbc[1]+dib_vbc[1]), -(dic_vbc[2]+dib_vbc[2]), -(dic_vbc[3]+dib_vbc[3])];
    BjtOutSoA {
        ib, ic, ie, itf, gtf, itr, gtr,
        dib_b: [dib_vbe[0]+dib_vbc[0], dib_vbe[1]+dib_vbc[1], dib_vbe[2]+dib_vbc[2], dib_vbe[3]+dib_vbc[3]],
        dib_e: [-dib_vbe[0], -dib_vbe[1], -dib_vbe[2], -dib_vbe[3]],
        dib_c: [-dib_vbc[0], -dib_vbc[1], -dib_vbc[2], -dib_vbc[3]],
        dic_b: [dic_vbe[0]+dic_vbc[0], dic_vbe[1]+dic_vbc[1], dic_vbe[2]+dic_vbc[2], dic_vbe[3]+dic_vbc[3]],
        dic_e: [-dic_vbe[0], -dic_vbe[1], -dic_vbe[2], -dic_vbe[3]],
        dic_c: [-dic_vbc[0], -dic_vbc[1], -dic_vbc[2], -dic_vbc[3]],
        die_b: [die_vbe[0]+die_vbc[0], die_vbe[1]+die_vbc[1], die_vbe[2]+die_vbc[2], die_vbe[3]+die_vbc[3]],
        die_e: [-die_vbe[0], -die_vbe[1], -die_vbe[2], -die_vbe[3]],
        die_c: [-die_vbc[0], -die_vbc[1], -die_vbc[2], -die_vbc[3]],
    }
}

/// One-lane branchless depletion charge, bit-exact with [`depletion_charge_pre`] on fast-math path.
/// `#[inline(always)]` forces inlining into [`depletion_charge_pre_batch`] for SLP fusion.
#[inline(always)]
fn dep_charge_lane_bl<const FMA: bool>(v: f64, cj0: f64, vj: f64, m: f64, c0: f64, q0: f64, dcdv: f64) -> (f64, f64) {
    const FC: f64 = 0.5;
    // pow arm: clamp f away from zero so log domain is safe even when v > vj (value discarded when !use_pow).
    let f = (1.0 - v / vj).max(1e-9);
    let c_pow = cj0 * pow_bl::<FMA>(f, -m);
    let q_pow = cj0 * vj / (1.0 - m) * (1.0 - pow_bl::<FMA>(f, 1.0 - m));
    // linear arm
    let d = v - FC * vj;
    let c_lin = fma::<FMA>(dcdv, d, c0); // c0 + dcdv*d
    // q0 + c0*d + 0.5*dcdv*d*d = fma(0.5*dcdv*d, d, fma(c0, d, q0))
    let q_lin = fma::<FMA>(0.5 * dcdv * d, d, fma::<FMA>(c0, d, q0));
    // select + cj0 guard (early-return → select so SLP can vectorize)
    let use_pow = v < FC * vj;
    let zero = cj0 <= 0.0;
    (
        if zero { 0.0 } else if use_pow { q_pow } else { q_lin },
        if zero { 0.0 } else if use_pow { c_pow } else { c_lin },
    )
}

/// Width-4 SoA depletion charge. Call ×2 (BE then BC) with the appropriate junction params.
/// Returns `(q[4], c[4])` bit-exact with 4 scalar [`depletion_charge_pre`] calls at `FMA=false`.
pub(in crate::wdf) fn depletion_charge_pre_batch<const FMA: bool>(
    v: [f64; 4],
    cj0: [f64; 4], vj: [f64; 4], m: [f64; 4],
    c0: [f64; 4], q0: [f64; 4], dcdv: [f64; 4],
) -> ([f64; 4], [f64; 4]) {
    let r0 = dep_charge_lane_bl::<FMA>(v[0], cj0[0], vj[0], m[0], c0[0], q0[0], dcdv[0]);
    let r1 = dep_charge_lane_bl::<FMA>(v[1], cj0[1], vj[1], m[1], c0[1], q0[1], dcdv[1]);
    let r2 = dep_charge_lane_bl::<FMA>(v[2], cj0[2], vj[2], m[2], c0[2], q0[2], dcdv[2]);
    let r3 = dep_charge_lane_bl::<FMA>(v[3], cj0[3], vj[3], m[3], c0[3], q0[3], dcdv[3]);
    (
        [r0.0, r1.0, r2.0, r3.0],
        [r0.1, r1.1, r2.1, r3.1],
    )
}

#[cfg(test)]
mod tests;
