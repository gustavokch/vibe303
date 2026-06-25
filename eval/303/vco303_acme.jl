#!/usr/bin/env julia
# TB-303 VCO core (saw/pulse) modelled in ACME.jl — a 1:1 translation of
# circuit/vco303.toml (imported from vibe303/netlist.txt). Companion to the
# ngspice render in run_vco303_spice.py; compared by compare (ESR/freq).
#
# The MPF104 JFET source-follower buffer has no native ACME element, so it is
# modelled by the generated depletion-NMOS surrogate mpf104() (same gate/source/
# drain topology + square-law clipping).
#
# SPIKE FINDING (2026-06-25): ACME does NOT reproduce the relaxation oscillation.
# At a fixed audio-rate step it settles to a slowly-drifting quasi-static solution
# (0 cycles, convergence warnings; crashes at 384 kHz). Root cause: ACME's DK-method
# uses a FIXED dt, but the saw core's regenerative SCR switching (Q3/Q4/Q5/D1) has
# sub-microsecond edges that ngspice resolves only via ADAPTIVE timestepping.
# Conclusion: the VCO core is validated by ngspice ONLY (run_vco303_spice.py:
# oscillation + 1 V/oct). ACME-vs-SPICE parity is reserved for the diode-ladder VCF
# (a filter, not a relaxation oscillator). This file is kept as the spike artifact
# and a template for the VCF's @circuit body.
#
# CLI:  julia --project vco303_acme.jl [CV] [SR] [END_S] [OUT_OP_BIN] [OUT_OR_BIN]
#   CV    control voltage (default 5 ≈ middle C per the deck)
#   SR    sample rate (default 48000)
#   END_S transient length seconds (default 0.15)

using ACME

# Component values + device constructors generated from circuit/vco303.toml.
include(joinpath(@__DIR__, "_vco303_gen.jl"))
using .vco303Gen
const G = vco303Gen
const q2n3904 = vco303Gen.q2n3904
const q2n3906 = vco303Gen.q2n3906
const q1n4148 = vco303Gen.q1n4148
const mpf104  = vco303Gen.mpf104

function build_circuit()
    @circuit begin
        # --- Driven rails / CV inputs (ramped via the input matrix u) --------
        Vvp = voltagesource();  Vvp[+] ⟷ vp;   Vvp[-] ⟷ gnd
        Vvh = voltagesource();  Vvh[+] ⟷ vh;   Vvh[-] ⟷ gnd
        Vcvb = voltagesource(); Vcvb[+] ⟷ cvb; Vcvb[-] ⟷ gnd
        Vcv = voltagesource();  Vcv[+] ⟷ cv;   Vcv[-] ⟷ gnd

        # --- Resistors -------------------------------------------------------
        R77 = resistor(G.R77); R77[1] ⟷ cvb;  R77[2] ⟷ ii
        R7  = resistor(G.R7);  R7[1]  ⟷ cv;   R7[2]  ⟷ ii
        R8  = resistor(G.R8);  R8[1]  ⟷ ii;   R8[2]  ⟷ gnd
        R9  = resistor(G.R9);  R9[1]  ⟷ vp;   R9[2]  ⟷ ib
        R88 = resistor(G.R88); R88[1] ⟷ cp;   R88[2] ⟷ ir
        R6  = resistor(G.R6);  R6[1]  ⟷ n10;  R6[2]  ⟷ vh
        R1  = resistor(G.R1);  R1[1]  ⟷ bf;   R1[2]  ⟷ gnd
        R2  = resistor(G.R2);  R2[1]  ⟷ bf;   R2[2]  ⟷ sh_in
        R3  = resistor(G.R3);  R3[1]  ⟷ n17;  R3[2]  ⟷ sh_in
        R5  = resistor(G.R5);  R5[1]  ⟷ vp;   R5[2]  ⟷ psi
        R4  = resistor(G.R4);  R4[1]  ⟷ vh;   R4[2]  ⟷ pl
        R90 = resistor(G.R90); R90[1] ⟷ bf;   R90[2] ⟷ d_r
        R91 = resistor(G.R91); R91[1] ⟷ o_r;  R91[2] ⟷ gnd
        R80 = resistor(G.R80); R80[1] ⟷ pl;   R80[2] ⟷ d_p
        R81 = resistor(G.R81); R81[1] ⟷ o_p;  R81[2] ⟷ gnd

        # --- Capacitors ------------------------------------------------------
        C3  = capacitor(G.C3);  C3[1]  ⟷ vp;   C3[2]  ⟷ cp
        C1  = capacitor(G.C1);  C1[1]  ⟷ bf;   C1[2]  ⟷ n17
        C2  = capacitor(G.C2);  C2[1]  ⟷ psi;  C2[2]  ⟷ gnd
        C90 = capacitor(G.C90); C90[1] ⟷ d_r;  C90[2] ⟷ o_r
        C80 = capacitor(G.C80); C80[1] ⟷ d_p;  C80[2] ⟷ o_p

        # --- BJTs (c/b/e per the IR) ----------------------------------------
        Q6 = q2n3906(); Q6[collector] ⟷ gnd; Q6[base] ⟷ ii;    Q6[emitter] ⟷ ib
        Q7 = q2n3904(); Q7[collector] ⟷ ir;  Q7[base] ⟷ ib;    Q7[emitter] ⟷ gnd
        Q4 = q2n3906(); Q4[collector] ⟷ n4;  Q4[base] ⟷ n12;   Q4[emitter] ⟷ vp
        Q5 = q2n3904(); Q5[collector] ⟷ n5;  Q5[base] ⟷ n5;    Q5[emitter] ⟷ n10
        Q3 = q2n3904(); Q3[collector] ⟷ n12; Q3[base] ⟷ n5;    Q3[emitter] ⟷ cp
        Q2 = q2n3906(); Q2[collector] ⟷ pl;  Q2[base] ⟷ sh_in; Q2[emitter] ⟷ psi

        # --- Diode (anode n4, cathode n5) -----------------------------------
        D1 = q1n4148(); D1[+] ⟷ n4; D1[-] ⟷ n5

        # --- JFET buffer surrogate (depletion NMOS): g=cp s=bf d=vp ---------
        JQ1 = mpf104(); JQ1[gate] ⟷ cp; JQ1[source] ⟷ bf; JQ1[drain] ⟷ vp

        # --- Output probes ---------------------------------------------------
        pop = voltageprobe(); pop[+] ⟷ o_p; pop[-] ⟷ gnd
        por = voltageprobe(); por[+] ⟷ o_r; por[-] ⟷ gnd
    end
end

function main()
    CV    = length(ARGS) >= 1 ? parse(Float64, ARGS[1]) : 5.0
    sr    = length(ARGS) >= 2 ? parse(Float64, ARGS[2]) : 48000.0
    end_s = length(ARGS) >= 3 ? parse(Float64, ARGS[3]) : 0.15

    N = round(Int, end_s * sr)
    nramp = round(Int, 1e-3 * sr)             # 1 ms supply ramp -> self-start
    targets = (12.0, 5.333, -4.7062 + CV, 0.0)  # vp, vh, cvb, cv  (input order)
    u = zeros(Float64, 4, N)
    @inbounds for i in 1:N
        s = i <= nramp ? i / nramp : 1.0
        for r in 1:4
            u[r, i] = s * targets[r]
        end
    end

    circ = build_circuit()
    model = DiscreteModel(circ, 1.0 / sr)
    y = run!(model, u)                         # 2 x N : [o_p; o_r]

    settle = round(Int, 0.02 * sr)             # ignore first 20 ms
    op = view(y, 1, settle+1:N)
    orr = view(y, 2, settle+1:N)
    op_ptp = maximum(op) - minimum(op)
    or_ptp = maximum(orr) - minimum(orr)

    # zero-crossing frequency estimate on o_p
    m = sum(op) / length(op)
    zc = 0
    @inbounds for i in 1:length(op)-1
        if op[i] - m < 0 && op[i+1] - m >= 0
            zc += 1
        end
    end
    f = zc / ((N - settle) / sr)

    println("CV=$CV  o_p: ptp $(round(op_ptp, digits=3)) V  ~$(round(f, digits=1)) Hz")
    println("        o_r: ptp $(round(or_ptp, digits=3)) V")
    osc = op_ptp > 0.05 && or_ptp > 0.05
    println("RESULT: ", osc ? "OSCILLATING" : "FLAT (no oscillation)")

    if length(ARGS) >= 5
        write(ARGS[4], collect(view(y, 1, :)))
        write(ARGS[5], collect(view(y, 2, :)))
    end
    exit(osc ? 0 : 1)
end

main()
