use super::*;

// ---- Phase 1b: batch device kernel bit-exactness ----

#[test]
fn compute_bjt_batch_is_bit_exact() {
    let tr1 = bc184c();
    let tr3 = q2n3055();
    // TR1=BC184C, TR2=BC184C, TR3=Q2N3055
    let bjts: [Bjt; 3] = [
        Bjt::new(0, 1, 2, tr1),
        Bjt::new(3, 4, 5, tr1),
        Bjt::new(6, 7, 8, tr3),
    ];

    let mut cases = 0usize;
    // Sweep vbe/vbc over the active junction domain
    let mut vbe = -0.5_f64;
    while vbe <= 0.8 {
        let mut vbc = -0.5_f64;
        while vbc <= 0.8 {
            let vbe4 = [vbe, vbe + 0.05, vbe - 0.05, 0.0];
            let vbc4 = [vbc, vbc - 0.05, vbc + 0.05, 0.0];
            let soa = compute_bjt_batch(&vbe4, &vbc4, &bjts);
            for bi in 0..3 {
                let s = compute_bjt(vbe4[bi], vbc4[bi], &bjts[bi].p);
                macro_rules! check_field {
                    ($field:ident) => {
                        let got = soa.$field[bi].to_bits();
                        let want = s.$field.to_bits();
                        assert_eq!(got, want,
                            "compute_bjt_batch: field={} lane={bi} vbe={} vbc={} got=0x{got:016x} want=0x{want:016x}",
                            stringify!($field), vbe4[bi], vbc4[bi]);
                    }
                }
                check_field!(ib); check_field!(ic); check_field!(ie);
                check_field!(itf); check_field!(gtf); check_field!(itr); check_field!(gtr);
                check_field!(dib_b); check_field!(dib_e); check_field!(dib_c);
                check_field!(dic_b); check_field!(dic_e); check_field!(dic_c);
                check_field!(die_b); check_field!(die_e); check_field!(die_c);
            }
            cases += 1;
            vbc += 0.1;
        }
        vbe += 0.1;
    }
    eprintln!("compute_bjt_batch_is_bit_exact: {cases} cases all bit-identical");
}

#[test]
fn compute_bjt_preexp_batch_is_bit_exact() {
    // Production guard: the width-4 post-exp batch must be bit-identical to the scalar
    // `compute_bjt_preexp` reference at FMA=false, lane-for-lane, across the junction domain.
    let tr1 = bc184c();
    let tr3 = q2n3055();
    let bjts: [Bjt; 3] = [
        Bjt::new(0, 1, 2, tr1),
        Bjt::new(3, 4, 5, tr1),
        Bjt::new(6, 7, 8, tr3),
    ];
    let exp_lane = |v: f64, vt: f64| exp_bl::<false>(v / vt);

    let mut cases = 0usize;
    let mut vbe = -0.5_f64;
    while vbe <= 0.8 {
        let mut vbc = -0.5_f64;
        while vbc <= 0.8 {
            let vbe4 = [vbe, vbe + 0.05, vbe - 0.05, 0.0];
            let vbc4 = [vbc, vbc - 0.05, vbc + 0.05, 0.0];
            // ef/er/ec exactly as newton_iterate batches them (lane 3 = padding params).
            let mk = |sel: &dyn Fn(usize) -> f64| {
                [
                    exp_lane(sel(0), bjts[0].p.vt),
                    exp_lane(sel(1), bjts[1].p.vt),
                    exp_lane(sel(2), bjts[2].p.vt),
                    exp_lane(0.0, 0.02585),
                ]
            };
            let ef4 = mk(&|i| vbe4[i]);
            let er4 = mk(&|i| vbc4[i]);
            let ec4 = mk(&|i| bjts[i].p.vcrit);
            let soa = compute_bjt_preexp_batch::<false>(&vbe4, &vbc4, &bjts, &ef4, &er4, &ec4);
            for bi in 0..3 {
                let s = compute_bjt_preexp::<false>(vbe4[bi], vbc4[bi], &bjts[bi].p, ef4[bi], er4[bi], ec4[bi]);
                macro_rules! check_field {
                    ($field:ident) => {
                        let got = soa.$field[bi].to_bits();
                        let want = s.$field.to_bits();
                        assert_eq!(got, want,
                            "compute_bjt_preexp_batch: field={} lane={bi} vbe={} vbc={} got=0x{got:016x} want=0x{want:016x}",
                            stringify!($field), vbe4[bi], vbc4[bi]);
                    }
                }
                check_field!(ib); check_field!(ic); check_field!(ie);
                check_field!(itf); check_field!(gtf); check_field!(itr); check_field!(gtr);
                check_field!(dib_b); check_field!(dib_e); check_field!(dib_c);
                check_field!(dic_b); check_field!(dic_e); check_field!(dic_c);
                check_field!(die_b); check_field!(die_e); check_field!(die_c);
            }
            cases += 1;
            vbc += 0.1;
        }
        vbe += 0.1;
    }
    eprintln!("compute_bjt_preexp_batch_is_bit_exact: {cases} cases all bit-identical");
}

#[test]
fn depletion_charge_pre_batch_is_bit_exact() {
    let tr1 = bc184c();
    let tr3 = q2n3055();
    let params = [tr1, tr1, tr3, BjtParams::default()];

    let mut cases = 0usize;
    let mut v = -50.0_f64;
    while v <= 1.0 {
        let v4 = [v, v + 0.05, v - 0.05, 0.0];
        // BE junction
        let cje4 = [params[0].cje, params[1].cje, params[2].cje, 0.0];
        let vje4 = [params[0].vje, params[1].vje, params[2].vje, 0.75];
        let mje4 = [params[0].mje, params[1].mje, params[2].mje, 0.33];
        let je_c04  = [params[0].je_c0,  params[1].je_c0,  params[2].je_c0,  0.0];
        let je_q04  = [params[0].je_q0,  params[1].je_q0,  params[2].je_q0,  0.0];
        let je_dcdv4 = [params[0].je_dcdv, params[1].je_dcdv, params[2].je_dcdv, 0.0];
        let (qbe4, cbe4) = depletion_charge_pre_batch::<false>(v4, cje4, vje4, mje4, je_c04, je_q04, je_dcdv4);
        for bi in 0..3 {
            let (qw, cw) = depletion_charge_pre(v4[bi], params[bi].cje, params[bi].vje,
                params[bi].mje, params[bi].je_c0, params[bi].je_q0, params[bi].je_dcdv);
            assert_eq!(qbe4[bi].to_bits(), qw.to_bits(),
                "dep_batch BE q: lane={bi} v={} got={} want={}", v4[bi], qbe4[bi], qw);
            assert_eq!(cbe4[bi].to_bits(), cw.to_bits(),
                "dep_batch BE c: lane={bi} v={} got={} want={}", v4[bi], cbe4[bi], cw);
        }
        // BC junction
        let cjc4 = [params[0].cjc, params[1].cjc, params[2].cjc, 0.0];
        let vjc4 = [params[0].vjc, params[1].vjc, params[2].vjc, 0.75];
        let mjc4 = [params[0].mjc, params[1].mjc, params[2].mjc, 0.33];
        let jc_c04  = [params[0].jc_c0,  params[1].jc_c0,  params[2].jc_c0,  0.0];
        let jc_q04  = [params[0].jc_q0,  params[1].jc_q0,  params[2].jc_q0,  0.0];
        let jc_dcdv4 = [params[0].jc_dcdv, params[1].jc_dcdv, params[2].jc_dcdv, 0.0];
        let (qbc4, cbc4) = depletion_charge_pre_batch::<false>(v4, cjc4, vjc4, mjc4, jc_c04, jc_q04, jc_dcdv4);
        for bi in 0..3 {
            let (qw, cw) = depletion_charge_pre(v4[bi], params[bi].cjc, params[bi].vjc,
                params[bi].mjc, params[bi].jc_c0, params[bi].jc_q0, params[bi].jc_dcdv);
            assert_eq!(qbc4[bi].to_bits(), qw.to_bits(),
                "dep_batch BC q: lane={bi} v={} got={} want={}", v4[bi], qbc4[bi], qw);
            assert_eq!(cbc4[bi].to_bits(), cw.to_bits(),
                "dep_batch BC c: lane={bi} v={} got={} want={}", v4[bi], cbc4[bi], cw);
        }
        cases += 1;
        v += 0.1;
    }
    eprintln!("depletion_charge_pre_batch_is_bit_exact: {cases} cases all bit-identical");
}
