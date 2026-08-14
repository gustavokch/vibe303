use plugin303::dsp::filter::Filter303;
use plugin303::dsp::ladder_wdf::LadderVcf;
use plugin303::dsp::vco::Vco303;

#[test]
fn test_vco_thermal_noise_injection() {
    // With thermal noise off (0), two VCOs should behave exactly identically
    let mut vco_quiet1 = Vco303::new(48000.0);
    let mut vco_quiet2 = Vco303::new(48000.0);
    vco_quiet1.set_thermal(0);
    vco_quiet2.set_thermal(0);
    vco_quiet1.set_freq(440.0);
    vco_quiet2.set_freq(440.0);

    for _ in 0..100 {
        let s1 = vco_quiet1.tick();
        let s2 = vco_quiet2.tick();
        assert_eq!(s1, s2, "quiet VCOs should be identical");
    }

    // With thermal noise on (21), two VCOs with different instance IDs (seeds) should deviate
    let mut vco_noisy1 = Vco303::new(48000.0);
    let mut vco_noisy2 = Vco303::new(48000.0);
    vco_noisy1.set_instance_id(1);
    vco_noisy2.set_instance_id(2);
    vco_noisy1.set_thermal(21);
    vco_noisy2.set_thermal(21);
    vco_noisy1.set_freq(440.0);
    vco_noisy2.set_freq(440.0);

    let mut deviated = false;
    for _ in 0..100 {
        let s1 = vco_noisy1.tick();
        let s2 = vco_noisy2.tick();
        if s1 != s2 {
            deviated = true;
            break;
        }
    }
    assert!(deviated, "noisy VCOs should deviate due to phase jitter");
}

#[test]
fn test_vcf_thermal_noise_injection() {
    let mut vcf_quiet1 = LadderVcf::new(48000.0);
    let mut vcf_quiet2 = LadderVcf::new(48000.0);
    vcf_quiet1.set_thermal(0);
    vcf_quiet2.set_thermal(0);
    vcf_quiet1.set_cutoff(1000.0);
    vcf_quiet2.set_cutoff(1000.0);

    for _ in 0..100 {
        let s1 = vcf_quiet1.tick(0.5);
        let s2 = vcf_quiet2.tick(0.5);
        assert_eq!(s1, s2, "quiet VCFs should be identical");
    }

    let mut vcf_noisy1 = LadderVcf::new(48000.0);
    let mut vcf_noisy2 = LadderVcf::new(48000.0);
    vcf_noisy1.set_instance_id(1);
    vcf_noisy2.set_instance_id(2);
    vcf_noisy1.set_thermal(21);
    vcf_noisy2.set_thermal(21);
    vcf_noisy1.set_cutoff(1000.0);
    vcf_noisy2.set_cutoff(1000.0);

    let mut deviated = false;
    for _ in 0..100 {
        let s1 = vcf_noisy1.tick(0.5);
        let s2 = vcf_noisy2.tick(0.5);
        if s1 != s2 {
            deviated = true;
            break;
        }
    }
    assert!(
        deviated,
        "noisy VCFs should deviate due to companion current noise"
    );
}
