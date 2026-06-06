#!/usr/bin/env julia
# BA283 AM output/line-amp section (drawing EX/10283) modelled in ACME.jl.
#
# A 1:1 translation of data_gen/netlist.py (the authoritative ngspice/PySpice
# netlist) into ACME's DK-method circuit. Feasibility spike: prove ACME can
# faithfully model the real 3-BJT + LO1166-transformer circuit, compared to
# SPICE by eval/acme_vs_spice/compare.py.
#
# CLI:  julia --project ba283_acme.jl DRY_BIN OUT_BIN SR GAIN_RKJ R_UPPER R_LOWER
#   DRY_BIN  raw little-endian Float64 mono samples (the dry stimulus, ~0.5 V pk)
#   OUT_BIN  raw little-endian Float64 mono samples written at node SEC_IN (volts)
#   SR       sample rate (Hz), e.g. 96000
#   GAIN_RKJ K<->J gain resistor R_KJ in ohms (gain conditioning)
#   R_UPPER  RV1 upper-leg ohms (TR3_E -> RV1_WIPER), computed in run_acme.py
#   R_LOWER  RV1 lower-leg ohms (RV1_WIPER -> gnd), computed in run_acme.py
#
# Node names mirror netlist.py verbatim. BJT params are mapped from the SPICE
# .MODEL cards onto ACME's Ebers-Moll/Gummel-Poon bjt() (no junction caps in
# ACME — the dominant HF shaping is the external Miller comp caps C2/C5/C3,
# modelled explicitly below).

using ACME

# --- Warmup: ACME starts from zero state (no SPICE-style .op). Prepend silence
# so the slow electrolytics settle (C4 100uF / R5 3.3k -> tau ~= 0.33 s, plus
# Cgain 80uF, C6 80uF). compare.py additionally onset-skips 0.5 s when scoring.
const WARMUP_S = 2.0

# BJT model parameter sets, mapped from netlist.py _TRANSISTOR_MODELS.
# Dropped (unsupported by ACME bjt): CJE/CJC/TF/TR/MJ*/VJ*/XTB/IRB/RBM/PTF.
bc184c() = bjt(:npn;
    is = 1.04e-14, βf = 600.0, βr = 0.1, vaf = 120.0, var = 200.0,
    ikf = 0.05, ise = 1.0e-14, ηe = 1.5, isc = 1.0e-15, ηc = 1.2,
    rb = 50.0, re = 0.5, rc = 0.5)

q2n3055() = bjt(:npn;
    is = 4.66e-12, βf = 360.0, βr = 2.0, vaf = 100.0,
    ikf = 0.25, ise = 3.339e-11, isc = 5.0e-9,
    rb = 3.0, rc = 0.04)

function build_circuit(gain_rkj::Float64, r_upper::Float64, r_lower::Float64)
    # LO1166: linear mutual-inductance transformer, LP=1.9 H primary, LS=5.36 H
    # secondary (N=1.68 step-up via LS=N^2*LP), K=0.9995. Winding DCR added as
    # series resistors (R_pri 14.5, R_sec 44), as in the SPICE .SUBCKT.
    @circuit begin
        # --- Rails: +24 V on node A, C6 decouples the rail to gnd -----------
        Vsupply = voltagesource(24.0)
        Vsupply[+] ⟷ A
        Vsupply[-] ⟷ gnd
        C6 = capacitor(80e-6)
        C6[1] ⟷ A
        C6[2] ⟷ gnd

        # --- Signal input on node INPUT (driven by the dry stimulus) --------
        Vin = voltagesource()
        Vin[+] ⟷ INPUT
        Vin[-] ⟷ gnd

        # ================= TR1 (BC184C) common-emitter input ===============
        # Input coupling C1 (10 uF) + R_esr_C7 (1) + R1 (2k2) into the base.
        C1 = capacitor(10e-6)
        C1[1] ⟷ INPUT
        C1[2] ⟷ C1_MID
        R_esr_C7 = resistor(1.0)
        R_esr_C7[1] ⟷ C1_MID
        R_esr_C7[2] ⟷ R1_MID
        R1 = resistor(2.2e3)
        R1[1] ⟷ R1_MID
        R1[2] ⟷ TR1_B
        # Base bleed R3 (68k) and DC-bias feedback R2 (56k) from RV1 wiper.
        R3 = resistor(68e3)
        R3[1] ⟷ TR1_B
        R3[2] ⟷ gnd
        R2 = resistor(56e3)
        R2[1] ⟷ RV1_WIPER
        R2[2] ⟷ TR1_B
        # Emitter load R4 (1k2); emitter is contact J's source node TR1_E.
        R4 = resistor(1.2e3)
        R4[1] ⟷ TR1_E
        R4[2] ⟷ gnd
        # Collector load R8 (33k) to rail.
        R8 = resistor(33e3)
        R8[1] ⟷ A
        R8[2] ⟷ TR1_C
        # Compensation: C2 (220 pF) Miller B-C, C3 (4700 pF) on collector.
        C2 = capacitor(220e-12)
        C2[1] ⟷ TR1_B
        C2[2] ⟷ TR1_C
        C3 = capacitor(4700e-12)
        C3[1] ⟷ TR1_C
        C3[2] ⟷ gnd
        TR1 = bc184c()
        TR1[collector] ⟷ TR1_C
        TR1[base] ⟷ TR1_B
        TR1[emitter] ⟷ TR1_E

        # --- K<->J gain network: R_KJ + 80 uF gain cap + 1 ohm ESR to gnd ---
        R_KJ = resistor(gain_rkj)
        R_KJ[1] ⟷ TR1_E
        R_KJ[2] ⟷ J
        Cgain = capacitor(80e-6)
        Cgain[1] ⟷ J
        Cgain[2] ⟷ Cgain_MID
        R_esr_Cgain = resistor(1.0)
        R_esr_Cgain[1] ⟷ Cgain_MID
        R_esr_Cgain[2] ⟷ gnd

        # ====== TR2 (BC184C) + TR3 (2N3055) common-collector compound ======
        # TR2 base = TR1_C; TR2 emitter -> TR3 base; collectors tied at F.
        R6 = resistor(18e3)
        R6[1] ⟷ TR2_E
        R6[2] ⟷ gnd
        C5 = capacitor(330e-12)   # Miller across TR2 B-C (TR1_C -> F)
        C5[1] ⟷ TR1_C
        C5[2] ⟷ F
        TR2 = bc184c()
        TR2[collector] ⟷ F
        TR2[base] ⟷ TR1_C
        TR2[emitter] ⟷ TR2_E

        # RV1 (4k7 preset) sampling TR3_E; wiper -> R2 -> TR1_B (DC bias).
        Rupper = resistor(r_upper)
        Rupper[1] ⟷ TR3_E
        Rupper[2] ⟷ RV1_WIPER
        Rlower = resistor(r_lower)
        Rlower[1] ⟷ RV1_WIPER
        Rlower[2] ⟷ gnd
        R7 = resistor(47.0)       # TR3 emitter resistor to gnd
        R7[1] ⟷ TR3_E
        R7[2] ⟷ gnd
        TR3 = q2n3055()
        TR3[collector] ⟷ F
        TR3[base] ⟷ TR2_E
        TR3[emitter] ⟷ TR3_E

        # --- A.C. feedback: F -> C4 (100 uF) -> R5 (3k3) -> TR1_E ----------
        C4 = capacitor(100e-6)
        C4[1] ⟷ F
        C4[2] ⟷ C4_MID
        R5 = resistor(3.3e3)
        R5[1] ⟷ C4_MID
        R5[2] ⟷ TR1_E

        # --- LO1166 output transformer: primary F->A, secondary SEC_IN->0 --
        # primary: R_pri (14.5) F -> PRI_IN, winding PRI_IN..A.
        Rpri = resistor(14.5)
        Rpri[1] ⟷ F
        Rpri[2] ⟷ PRI_IN
        LO1166 = transformer(1.9, 5.36; coupling_coefficient = 0.9995)
        LO1166[primary1] ⟷ PRI_IN
        LO1166[primary2] ⟷ A
        LO1166[secondary1] ⟷ SEC_W
        LO1166[secondary2] ⟷ gnd
        # secondary: R_sec (44) SEC_IN -> winding node SEC_W.
        Rsec = resistor(44.0)
        Rsec[1] ⟷ SEC_IN
        Rsec[2] ⟷ SEC_W

        # Primary Zobel (1k5 + 0.01 uF) across F -> A damps the leakage ring.
        Rzobel = resistor(1.5e3)
        Rzobel[1] ⟷ F
        Rzobel[2] ⟷ ZOBEL_MID
        Czobel = capacitor(0.01e-6)
        Czobel[1] ⟷ ZOBEL_MID
        Czobel[2] ⟷ A
        # 600 ohm console line-in termination on the secondary tap.
        Rterm = resistor(600.0)
        Rterm[1] ⟷ SEC_IN
        Rterm[2] ⟷ gnd

        # Output probe at the balanced secondary tap SEC_IN (volts).
        out = voltageprobe()
        out[+] ⟷ SEC_IN
        out[-] ⟷ gnd
    end
end

function main()
    dry_bin  = ARGS[1]
    out_bin  = ARGS[2]
    sr       = parse(Float64, ARGS[3])
    gain_rkj = parse(Float64, ARGS[4])
    r_upper  = parse(Float64, ARGS[5])
    r_lower  = parse(Float64, ARGS[6])

    # Read dry samples (raw little-endian Float64, mono).
    raw = read(dry_bin)
    dry = reinterpret(Float64, raw)
    n = length(dry)

    # Prepend silence warmup so the DC operating point settles from zero state.
    nwarm = round(Int, WARMUP_S * sr)
    u = zeros(Float64, 1, nwarm + n)
    @inbounds for i in 1:n
        u[1, nwarm + i] = dry[i]
    end

    circ = build_circuit(gain_rkj, r_upper, r_lower)
    model = DiscreteModel(circ, 1.0 / sr)
    y = run!(model, u)                  # 1 x (nwarm+n)

    # Drop the warmup; keep the wet response aligned to the dry length.
    wet = collect(view(y, 1, nwarm+1:nwarm+n))
    open(out_bin, "w") do io
        write(io, wet)
    end
end

main()
