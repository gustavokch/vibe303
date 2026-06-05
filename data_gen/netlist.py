"""BA283 SPICE netlist, parameterized by RV1 bias position.

Real Neve BA283 AM output/line-amp section (drawing EX/10283), reconstructed
to the actual schematic topology. The audio path is a three-transistor
d.c.-coupled amplifier between card contacts A (+24 V rail) and M (0 V):

  * TR1 (BC184C) common-emitter input. Signal in via C1 (10 µF) + R1 (2k2).
    Emitter load R4 (1k2) to ground; the emitter is card contact **J**.
  * TR2 (BC184C) + TR3 (2N3055) are a **common-collector compound pair**: TR1's
    collector drives TR2's base, TR2's **emitter** drives TR3's base, and the
    **collectors of TR2 and TR3 are tied together** (node ``F``) and driven into
    the LO1166 primary, which returns to the +24 V rail (the primary doubles as
    the collector choke). Contact F is the unbalanced output; the transformer
    secondary (node ``SEC_IN``) is the balanced output we sample.
  * GAIN: the external K<->J network — R_KJ (220 Ω for the 26 dB tap) in series
    with an 80 µF cap — shunts TR1's emitter load R4 for a.c., cutting local
    feedback and setting closed-loop gain. The cap blocks d.c. so the gain
    setting never disturbs the bias.
  * A.C. FEEDBACK: fixed loop from the common collector F via C4 (100 µF,
    d.c. block) + R5 (3k3) back to TR1's emitter.
  * D.C. BIAS: RV1 (4k7 preset) samples TR3's emitter and feeds R2 (56 k) to
    TR1's base, self-biasing the triplet. RV1 is the real card's trim — set
    once for ~65 mA TR3 idle / symmetric clipping. Here it is the model's
    bias-conditioning input: sweeping it shifts TR3 idle current and the
    clipping character. ``bias_norm`` in [0,1] is remapped to the non-degenerate
    portion of the pot (very low wiper runs TR3 idle away to saturation), see
    ``RV1_WIPER_MIN``.
  * COMPENSATION: C2 (220 pF) across TR1 base-collector, C5 (330 pF) across the
    TR2 base-collector (Miller), C3 (4700 pF) on TR1's collector — the BA283's
    actual stabilising caps; they hold phase margin so the high-gain loop does
    not oscillate at large signal.

Output transformer LO1166 is a linear mutual-inductance model: two coupled
windings (``K1 L_pri L_sec``) with turns ratio N = 1.68 set by ``LS = N^2*LP``,
primary magnetizing ``LP`` ~ 1.9 H, real winding DCR (R_pri 14.5 Ω, R_sec 44 Ω).
The windings block DC at the secondary (clean AC-only tap). A primary Zobel
(1k5 + 0.01 µF across F->A) damps the leakage-inductance HF top-ring, exactly as
on the real card. Core saturation + B-H hysteresis are a v2 refinement; the
dominant nonlinearity is the transistor chain. The d.c.-coupled feedback bias
loop gives a convergent operating point, so data-gen runs an ``.op``-initialized
transient (steady-state start, no turn-on settling).
"""

from __future__ import annotations

from PySpice.Spice.Netlist import Circuit

# --- Verbatim component model cards (mirrored in design doc Appendix A) -----

_TRANSISTOR_MODELS = """\
.MODEL 2N3055 NPN(IS=4.66E-12 BF=360 VAF=100 IKF=0.25 ISE=3.339E-11 BR=2 ISC=5E-9 RB=3 IRB=0.001 RBM=0.4 RC=0.04 CJE=5.802E-10 VJE=1.2 MJE=0.45 TF=8E-8 XTF=1 ITF=3 PTF=120 CJC=2.121E-10 MJC=0.4 TR=2.55E-6 XTB=1)
.MODEL BC184C NPN(IS=1.04E-14 BF=600 VAF=120 IKF=0.05 ISE=1.00E-14 NE=1.5 BR=0.1 NR=1.00 VAR=200 ISC=1.00E-15 NC=1.2 RB=50 RE=0.5 RC=0.5 CJC=4.50E-12 MJC=0.33 VJC=0.75 CJE=1.40E-11 MJE=0.33 VJE=0.75 TF=4.00E-10 TR=1.00E-08)
"""  # noqa: E501

# Marinair/Carnhill LO1166 gapped single-ended output transformer, linear
# mutual-inductance model. Turns ratio N = Nsec/Npri = 1.68 (1:1.68 step-up);
# LS = N^2 * LP gives the 1.68x passband voltage gain. LP ~ 1.9 H magnetizing
# (20 Hz LF corner into the reflected ~600 Ohm line). Real winding DC
# resistances: primary 14.5 Ohm (7+8 across the windings), secondary 44 Ohm.
# K < 1 gives the leakage inductance; the primary Zobel (added in build_netlist)
# damps its HF ring. Windings block DC -> clean AC-only secondary tap.
_LO1166_SUBCKT = """\
.SUBCKT LO1166 PRI_IN PRI_OUT SEC_IN SEC_OUT
.param LP=1.9 LS=5.36 K=0.9995
R_pri PRI_IN pa 14.5
L_pri pa PRI_OUT {LP}
L_sec sa SEC_OUT {LS}
R_sec SEC_IN sa 44.0
K1 L_pri L_sec {K}
.ENDS LO1166
"""

# Gain-setting K<->J network: R_KJ (220 Ohm = the real card's 26 dB tap) in
# series with the 80 uF gain cap shunts TR1's emitter load R4 for a.c. R5 is the
# a.c.-feedback resistor (common collector F -> C4 -> R5 -> TR1 emitter).
FB_R5_OHMS = 3.3e3
GAIN_RKJ_OHMS = 220.0

# RV1 (4k7 preset) bias map. The very-low-wiper end runs TR3 idle current away
# to saturation (output collapses), so bias_norm in [0,1] is mapped to the
# stable class-A portion of the pot: wiper fraction = RV1_WIPER_MIN..1.0. Over
# that range TR3 idle sweeps ~160 mA -> ~55 mA, passing the real card's
# 65-70 mA / symmetric-clipping setting near the top of the range.
RV1_WIPER_MIN = 0.3

# ESR series resistors named R_esr_C7 / R_esr_Cgain (1 Ω each), i.e. the
# "C_esr" series model for the input and gain electrolytics. The a.c.-feedback
# cap C4 uses the feedback resistor R5 as its series element. The substring
# below is a token marker used by tests/data_gen/test_netlist.py.
_ESR_COMMENT = (
    "* ESR series resistors R_esr_C7 / R_esr_Cgain (1 ohm); the 100 uF "
    "feedback cap C4 uses R5 as its series element (models C_esr)\n"
)


def build_netlist(bias_norm: float) -> Circuit:
    """Build the BA283 AM netlist with RV1 wiper at ``bias_norm``.

    Parameters
    ----------
    bias_norm:
        Bias-conditioning position in ``[0.0, 1.0]``, remapped to the stable
        portion of the RV1 pot (``RV1_WIPER_MIN``..1.0). 0.0 = hottest usable
        TR3 idle, 1.0 = coolest; the real card's ~65 mA symmetric-clipping
        setting falls near the upper end. Each pot leg is clamped to at least
        1 Ω so ngspice never sees a true short.

    Returns
    -------
    PySpice ``Circuit`` powered from the +24 V rail (node ``A``). The
    d.c.-coupled triplet plus the RV1 DC-bias loop give a convergent DC
    operating point, so data-gen runs an ``.op``-initialized transient. Audio
    injection (filesource on node ``INPUT``) is layered in by ``run_spice.py``.
    """
    if not (0.0 <= bias_norm <= 1.0):
        raise ValueError(
            f"bias_norm must be in [0.0, 1.0]; got {bias_norm!r}"
        )

    # Map the [0,1] conditioning value onto the stable wiper range.
    wiper = RV1_WIPER_MIN + (1.0 - RV1_WIPER_MIN) * bias_norm
    r_upper = max(1.0, 4700.0 * (1.0 - wiper))
    r_lower = max(1.0, 4700.0 * wiper)

    circuit = Circuit("BA283")

    # Inject transistor .MODEL cards + LO1166 .SUBCKT verbatim via raw_spice to
    # keep the cards bit-for-bit identical to the design-doc appendix.
    circuit.raw_spice += _ESR_COMMENT
    circuit.raw_spice += _TRANSISTOR_MODELS
    circuit.raw_spice += _LO1166_SUBCKT

    # --- Rails -------------------------------------------------------------
    # +24 V supply on node A (the BA283 "A" rail). Signal ground is PySpice's
    # global ground (node 0). C6 decouples the rail.
    circuit.V("supply", "A", circuit.gnd, 24)
    circuit.C("6", "A", circuit.gnd, 80e-6)

    # ====================================================================
    # TR1 (BC184C) — common-emitter input stage.
    # ====================================================================
    # Input coupling C1 (10 µF) + R1 (2k2) + 1 Ω ESR into the base.
    circuit.C("1", "INPUT", "C1_MID", 10e-6)
    circuit.R("_esr_C7", "C1_MID", "R1_MID", 1.0)
    circuit.R("1", "R1_MID", "TR1_B", 2.2e3)
    # Base bleed R3 (68 k) and the DC-bias feedback R2 (56 k) from the RV1 wiper.
    circuit.R("3", "TR1_B", circuit.gnd, 68e3)
    circuit.R("2", "RV1_WIPER", "TR1_B", 56e3)
    # Emitter load R4 (1k2); the emitter is contact J.
    circuit.R("4", "TR1_E", circuit.gnd, 1.2e3)
    # Collector load R8 (33 k) to the rail.
    circuit.R("8", "A", "TR1_C", 33e3)
    # Compensation: C2 (220 pF) Miller across TR1 B-C, C3 (4700 pF) on TR1_C.
    circuit.C("2", "TR1_B", "TR1_C", 220e-12)
    circuit.C("3", "TR1_C", circuit.gnd, 4700e-12)
    circuit.BJT("TR1", "TR1_C", "TR1_B", "TR1_E", model="BC184C")

    # --- K <-> J gain network: shunts TR1 emitter load to ground for a.c. ---
    # R_KJ (220 Ω, the 26 dB tap) + 80 µF gain cap + 1 Ω ESR. The cap blocks DC,
    # so the gain setting does not disturb the RV1 bias.
    circuit.R("KJ", "TR1_E", "J", GAIN_RKJ_OHMS)
    circuit.C("gain", "J", "Cgain_MID", 80e-6)
    circuit.R("_esr_Cgain", "Cgain_MID", circuit.gnd, 1.0)

    # ====================================================================
    # TR2 (BC184C) + TR3 (2N3055) — common-collector compound output pair.
    #   TR2 base = TR1 collector; TR2 emitter -> TR3 base; TR2 & TR3 collectors
    #   tied together at node F (driven into the transformer primary).
    # ====================================================================
    # TR2 emitter resistor R6 (18 k); C5 (330 pF) Miller across TR2 B-C (TR1_C->F)
    circuit.R("6", "TR2_E", circuit.gnd, 18e3)
    circuit.C("5", "TR1_C", "F", 330e-12)
    circuit.BJT("TR2", "F", "TR1_C", "TR2_E", model="BC184C")

    # --- RV1 (4k7 bias preset) sampling TR3's emitter ----------------------
    # Pot from TR3_E to ground; the wiper (a fraction of TR3_E set by bias_norm)
    # is the DC-bias reference fed to TR1 via R2. Both legs clamped >= 1 Ω.
    circuit.R("upper", "TR3_E", "RV1_WIPER", r_upper)
    circuit.R("lower", "RV1_WIPER", circuit.gnd, r_lower)
    # TR3 emitter resistor R7 (47 Ω) to ground.
    circuit.R("7", "TR3_E", circuit.gnd, 47.0)
    circuit.BJT("TR3", "F", "TR2_E", "TR3_E", model="2N3055")

    # --- A.C. feedback: common collector F -> C4 (100 µF) -> R5 (3k3) -> TR1_E
    circuit.C("4", "F", "C4_MID", 100e-6)
    circuit.R("5", "C4_MID", "TR1_E", FB_R5_OHMS)

    # --- LO1166 instance + primary Zobel + 600 Ω secondary termination -----
    # Subcircuit pins: PRI_IN=F (common collector), PRI_OUT=A (rail, primary as
    # collector choke), SEC_IN (balanced output tap), SEC_OUT=0.
    circuit.raw_spice += "XLO1166 F A SEC_IN 0 LO1166\n"
    # Primary Zobel (Boucherot): 1k5 + 0.01 µF across F->A damps the leakage-
    # inductance HF top-ring (the real card's snubber).
    circuit.R("zobel", "F", "ZOBEL_MID", 1.5e3)
    circuit.C("zobel", "ZOBEL_MID", "A", 0.01e-6)
    # 600 Ω console line-in termination on the secondary tap.
    circuit.R("term", "SEC_IN", circuit.gnd, 600)

    return circuit


if __name__ == "__main__":
    print(str(build_netlist(0.5)))
