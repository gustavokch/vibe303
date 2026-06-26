#!/usr/bin/env julia
# TB-303 diode-ladder VCF — ACME.jl SPIKE ARTIFACT documenting a NO-GO.
# This is the bare-ladder-core ACME rig built for step-4 ACME-vs-SPICE parity of
# circuit/vcf303.toml. It is kept (like vco303_acme.jl) as the reproducer of a
# DEFINITIVE NO-GO, NOT as a passing validator. Run by compare_vcf303.py, which
# shows the ACME response is degenerate while the identical ngspice rig is correct.
#
# ★ FINDING (2026-06-26, NO-GO — see VCF_MAP §12):
#   ACME-vs-SPICE parity for the vcf303 is NOT achievable — for the diode LADDER
#   CORE *or* the full active circuit. The 303 VCF sets its cutoff with an IDEAL
#   BIAS CURRENT (the expo-converter output; here per-rail current sources, like
#   SPICE step 1). ACME's DK-method has no SPICE-style gmin, so the current-source-
#   driven nodes are singular at the homotopy's u=0 start and ACME aborts with
#   "Failed to find initial solution". Adding gmin reference leaks (needed to break
#   the singularity) makes ACME CONVERGE — but to a NONPHYSICAL branch: the homotopy
#   routes the entire ideal bias current through the leaks instead of through the
#   diodes, so every node runs away to |V| = I_bias * R_leak (probed: tl = -64 kV,
#   dl = +64 kV, every ladder diode reverse-biased ~64 kV = OFF at I=64µA/1GΩ; ±6.4
#   MV at 100GΩ). The ladder is dead; the small-signal transimpedance collapses to
#   ~1e-3 Ω (numerical noise) vs the correct 20 kΩ ngspice gives on the identical
#   netlist. The SAME mechanism kills the full active circuit (the T38 long-tailed-
#   pair tail current escapes the leak -> T38 OFF -> bias-INDEPENDENT dead filter).
#   ngspice's Newton + gmin/source-stepping finds the physical "devices-ON" branch;
#   ACME's homotopy does not. This EXTENDS the VCO ACME NO-GO (vco303_acme.jl):
#   ACME's solver is systematically less robust than ngspice's for the 303's
#   current-source-biased nonlinear topologies. ⇒ ngspice stays the VCF ground truth
#   (run_vcf303_spice.py, steps 1-3, committed + gated); there is no ACME VCF parity.
#
# The rig below (bare 8-diode ladder + 4 rail caps, per-rail ideal bias current,
# differential AC current drive, differential Rtop readout, 1GΩ common-mode leaks)
# builds and runs but yields the degenerate response above — that IS the artifact.
#
# CLI:  julia --project vcf303_acme.jl OUTDIR SR T_S IUA1 [IUA2 ...]
#   OUTDIR  dir for ac.bin + out_<uA>.bin + meta.txt (Float64 LE)   (required)
#   SR      sample rate Hz                                           (default 192000)
#   T_S     chirp length seconds                                     (default 0.5)
#   IUAk    per-rail bias current(s) in µA (cutoff; one render each) (default 16)

using ACME

include(joinpath(@__DIR__, "_vcf303_gen.jl"))
using .vcf303Gen
const G = vcf303Gen
const q1n4148 = vcf303Gen.q1n4148

const GMIN_R = 1e9   # common-mode reference leak (SPICE-gmin surrogate)
const RTOP = 20000.0 # differential readout load tl-tr (≈ R95 pair) — the AC load
const IAC = 1e-7     # differential AC drive amplitude (A) — small-signal

# All ten ladder nodes get a common-mode leak (both rails are current-driven).
const LEAK_NODES = [:tl,:al,:bl,:cl,:dl,:tr,:ar,:br,:cr,:dr]

function build_ladder(i_bias::Float64)
    c = Circuit()
    # ladder series diodes (anode top -> cathode bottom)
    for (nm, a, k) in [(:D1L,:tl,:al),(:D1R,:tr,:ar),(:D2L,:al,:bl),(:D2R,:ar,:br),
                       (:D3L,:bl,:cl),(:D3R,:br,:cr),(:D4L,:cl,:dl),(:D4R,:cr,:dr)]
        add!(c, nm, q1n4148()); connect!(c, (nm,:+), a); connect!(c, (nm,:-), k)
    end
    # shunt caps bridging the rails (4 poles)
    for (nm, v, n1, n2) in [(:C98,G.C98,:al,:ar),(:C99,G.C99,:bl,:br),(:C100,G.C100,:cl,:cr),(:C101,G.C101,:dl,:dr)]
        add!(c, nm, capacitor(v)); connect!(c, (nm,1), n1); connect!(c, (nm,2), n2)
    end
    # DC bias current: push i_bias into each rail top, sink it from each bottom
    # (flows top -> bottom through the 4 series diodes = sets the diode r = Vt/I).
    add!(c, :Itl, currentsource(i_bias)); connect!(c, (:Itl,:+), :gnd); connect!(c, (:Itl,:-), :tl)
    add!(c, :Itr, currentsource(i_bias)); connect!(c, (:Itr,:+), :gnd); connect!(c, (:Itr,:-), :tr)
    add!(c, :Idl, currentsource(i_bias)); connect!(c, (:Idl,:+), :dl); connect!(c, (:Idl,:-), :gnd)
    add!(c, :Idr, currentsource(i_bias)); connect!(c, (:Idr,:+), :dr); connect!(c, (:Idr,:-), :gnd)
    # differential AC signal current at the bottom = the ONE driven input
    add!(c, :Iac, currentsource()); connect!(c, (:Iac,:+), :dl); connect!(c, (:Iac,:-), :dr)
    # differential readout load at the top (carries no DC — rails balanced — so it
    # does not steal the bias current; gives the differential AC its return path)
    add!(c, :Rtop, resistor(RTOP)); connect!(c, (:Rtop,1), :tl); connect!(c, (:Rtop,2), :tr)
    # common-mode reference leaks
    for n in LEAK_NODES
        nm = Symbol("Rg_", n); add!(c, nm, resistor(GMIN_R)); connect!(c, (nm,1), n); connect!(c, (nm,2), :gnd)
    end
    # differential readout at the ladder top
    add!(c, :pout, voltageprobe()); connect!(c, (:pout,:+), :tl); connect!(c, (:pout,:-), :tr)
    return c
end

function render(i_bias::Float64, sr::Float64, ac::Vector{Float64}, nset::Int)
    model = DiscreteModel(build_ladder(i_bias), 1.0 / sr; decompose_nonlinearity=false)
    u = reshape(ac, 1, length(ac))            # input = differential AC current
    out = vec(run!(model, u))
    win = @view out[nset+1:end]
    ptp = maximum(win) - minimum(win)
    println("  I_bias=$(round(i_bias*1e6,digits=1))uA  out diff ptp $(round(ptp,digits=5)) V  finite=$(all(isfinite,out))")
    return out
end

function main()
    length(ARGS) >= 1 || error("usage: vcf303_acme.jl OUTDIR [SR T_S IUA...]")
    outdir = ARGS[1]
    sr   = length(ARGS) >= 2 ? parse(Float64, ARGS[2]) : 192000.0
    t_s  = length(ARGS) >= 3 ? parse(Float64, ARGS[3]) : 0.5
    iuas = length(ARGS) >= 4 ? [parse(Float64, a) for a in ARGS[4:end]] : [16.0]
    mkpath(outdir)

    nset = round(Int, 0.03 * sr)              # 30 ms settle before the chirp
    nchirp = round(Int, t_s * sr)
    N = nset + nchirp
    f0, f1 = 2.0, min(24000.0, 0.45 * sr)   # f0 low: the diode-ladder dominant pole is tens of Hz
    ac = zeros(Float64, N)
    @inbounds for k in 1:nchirp
        t = (k - 1) / sr
        inst = f0 + (f1 - f0) * (t / t_s) * 0.5   # linear sweep, phase = 2π·inst·t
        ac[nset + k] = IAC * sin(2π * inst * t)
    end
    write(joinpath(outdir, "ac.bin"), ac)
    open(joinpath(outdir, "meta.txt"), "w") do io
        println(io, "sr=$sr"); println(io, "nset=$nset"); println(io, "N=$N")
        println(io, "iac=$IAC"); println(io, "f0=$f0"); println(io, "f1=$f1")
        println(io, "rig=ladder")
    end

    println("ACME ladder render: sr=$(round(Int,sr))  T=$(t_s)s  N=$N  bias(uA)=$iuas")
    for iua in iuas
        out = render(iua * 1e-6, sr, ac, nset)
        write(joinpath(outdir, "out_$(Int(round(iua))).bin"), out)
    end
    println("wrote ac.bin + out_<uA>.bin to $outdir")
end

main()
