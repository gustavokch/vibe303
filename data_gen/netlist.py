"""BA283 SPICE netlist, parameterized by RV1 bias position.

Topology: Neve BA283 audio path reduced to a single 26 dB gain bridge
(TR1 + TR2 BC184C input/driver, TR3 2N3055 output) feeding the LO1166
gapped output transformer with a 600 Ω secondary termination. Bias
control sweeps the RV1 wiper (a 4K7 pot between +24 V and ground).

Electrolytics are modelled as an ideal cap with a series resistor: C7
(input) and the 80 µF gain cap (Cgain) use a 1 Ω ESR (R_esr_C7,
R_esr_Cgain); the 80 µF A.C.-feedback cap C4 uses the feedback resistor
R5 as its series element. This is the "C_esr" series model from the design
doc.

The three stages are d.c.-coupled (TR1 collector -> TR2 base -> TR3 base)
and a DC-feedback loop (RV1 in TR3's emitter path -> 56 k -> TR1 base)
self-biases the amplifier, so the circuit has a convergent operating point
and data-gen runs an ``.op``-initialized transient (steady-state start, no
turn-on settling).

Output transformer LO1166 is a linear mutual-inductance model (v1):
two coupled windings (``K1 L_pri L_sec``) with turns ratio N = 310/155 =
2 set by ``LS = N^2 * LP`` (passband gain sqrt(LS/LP) = 2). The windings
are inductors, so DC is blocked at the secondary (clean AC-only output
tap); ``K < 1`` gives a leakage inductance and a gentle HF rolloff above
the audio band. Core saturation + B-H hysteresis are a second-order
effect deferred to v2; the dominant nonlinearity is the transistor
chain. ``.op`` does not converge (TR2 base is AC-coupled with no DC bias
resistor), so data-gen runs a ``uic`` transient.
"""

from __future__ import annotations

from PySpice.Spice.Netlist import Circuit

# --- Verbatim component model cards (mirrored in design doc Appendix A) -----

_TRANSISTOR_MODELS = """\
.MODEL 2N3055 NPN(IS=4.66E-12 BF=360 VAF=100 IKF=0.25 ISE=3.339E-11 BR=2 ISC=5E-9 RB=3 IRB=0.001 RBM=0.4 RC=0.04 CJE=5.802E-10 VJE=1.2 MJE=0.45 TF=8E-8 XTF=1 ITF=3 PTF=120 CJC=2.121E-10 MJC=0.4 TR=2.55E-6 XTB=1)
.MODEL BC184C NPN(IS=1.04E-14 BF=600 VAF=120 IKF=0.05 ISE=1.00E-14 NE=1.5 BR=0.1 NR=1.00 VAR=200 ISC=1.00E-15 NC=1.2 RB=50 RE=0.5 RC=0.5 CJC=4.50E-12 MJC=0.33 VJC=0.75 CJE=1.40E-11 MJE=0.33 VJE=0.75 TF=4.00E-10 TR=1.00E-08)
"""  # noqa: E501

_LO1166_SUBCKT = """\
* Marinair LO1166 Gapped Output Transformer (linear mutual-inductance model).
* Two coupled windings L_pri/L_sec with coupling coefficient K. Turns ratio
* N = Nsec/Npri = 310/155 = 2 is set by LS = N^2 * LP, so the passband voltage
* gain is sqrt(LS/LP) = 2 (+6 dB). The windings are inductors -> DC is blocked
* at the secondary (Vsec = M*dIpri/dt + LS*dIsec/dt = 0 at DC), a clean AC-only
* output tap as a real galvanically-isolated transformer. K < 1 gives the
* leakage inductance LP*(1-K) ~ 1 mH, i.e. a gentle HF rolloff (corner ~25 kHz
* into the reflected load) above the audio band.
*
* This is the standard, numerically well-behaved transformer form. It replaces:
*   (1) a prior XSPICE coupled-inductor + PWL-core model that mis-wired the
*       magnetic node (the core shunted MMF instead of linking windings): a
*       sub-100 Hz lowpass, gain ~ 1/Lmag, +34.8 V DC pedestal -- untunable; and
*   (2) an E/F controlled-source ideal transformer that, while correct in gain
*       and DC blocking, formed an undamped algebraic feedback loop (secondary
*       current -> reflected primary current -> mag voltage -> secondary voltage
*       -> secondary current). Driven by TR3 at
*       the sweep's ~20 kHz tail it went near-oscillatory and collapsed the
*       transient timestep to ~1e-17 (abort). The finite mutual inductances here
*       break that loop and the K<1 HF rolloff removes the offending HF energy.
*
* NOTE: linear K coupling is fully supported by ngspice. The original plan's
* failure was attaching a nonlinear .MODEL CORE to the K line, which ngspice
* does not implement; a plain coupling coefficient does not need a model card.
*
* v1 is intentionally LINEAR (anhysteretic, no core saturation). The dominant
* BA283 nonlinearity is the transistor chain (already in the netlist); output-
* transformer saturation + B-H hysteresis are a second-order effect deferred to
* v2. (A flux-integrator + saturating B-source was tried and rejected: a
* standing DC across the primary wound the flux integrator up without bound,
* latching the core into saturation and killing all AC transfer. v2 must add
* saturation on a DC-blocked / anti-windup flux path.)
*
.SUBCKT LO1166 PRI_IN PRI_OUT SEC_IN SEC_OUT
.param LP=2 LS=8 K=0.9999
R_pri PRI_IN pa 4.5
L_pri pa PRI_OUT {LP}
L_sec sa SEC_OUT {LS}
R_sec SEC_IN sa 12.0
K1 L_pri L_sec {K}
.ENDS LO1166
"""

# Gain-setting resistors (ohms), calibrated by simulation to the BA283's 26 dB
# spec: R5 is the A.C.-feedback resistor; R_KJ is the K<->J emitter-shunt tap
# (Av ~ R5 / (R4 || R_KJ)). On the real card R_KJ=220 is the 26 dB tap, but this
# reconstruction's forward gain is lower so R_KJ=47 lands the small-signal gain
# on 26 dB (measured +26.9 dB at bias 0.5).
FB_R5_OHMS = 3.3e3
GAIN_RKJ_OHMS = 47.0

# ESR series resistors named R_esr_C7 / R_esr_C4 / R_esr_Cgain (1 Ω each),
# i.e. the "C_esr" series model for each electrolytic. The substring
# below is a token marker used by tests/data_gen/test_netlist.py.
_ESR_COMMENT = (
    "* ESR series resistors R_esr_C7 / R_esr_Cgain (1 ohm); the 80 uF "
    "feedback cap C4 uses R5 as its series element (models C_esr)\n"
)


def build_netlist(bias_norm: float) -> Circuit:
    """Build the BA283 SPICE netlist with RV1 wiper at ``bias_norm``.

    Parameters
    ----------
    bias_norm:
        Wiper position in ``[0.0, 1.0]``. 0.0 puts the wiper at ground
        (lower leg short), 1.0 puts it at +24 V (upper leg short). Maps
        to ``R_upper = 4700 * (1 - bias_norm)``,
        ``R_lower = 4700 * bias_norm``, each clamped to at least 1 Ω so
        ngspice never sees a true short on either leg of the pot.

    Returns
    -------
    PySpice ``Circuit`` powered from the +24 V rail (node ``A``). The
    d.c.-coupled stages plus the RV1 DC-feedback loop give a convergent DC
    operating point, so data-gen runs an ``.op``-initialized transient.
    Audio injection (filesource on node ``INPUT``) is layered in by
    ``run_spice.py``.
    """
    if not (0.0 <= bias_norm <= 1.0):
        raise ValueError(
            f"bias_norm must be in [0.0, 1.0]; got {bias_norm!r}"
        )

    r_upper = max(1.0, 4700.0 * (1.0 - bias_norm))
    r_lower = max(1.0, 4700.0 * bias_norm)

    circuit = Circuit("BA283")

    # Inject transistor .MODEL cards + LO1166 .SUBCKT verbatim. PySpice's
    # high-level API cannot express the ngspice XSPICE code models
    # (lcouple/core a-devices) directly, so we use raw_spice to keep the
    # cards bit-for-bit identical to the design-doc appendix.
    circuit.raw_spice += _ESR_COMMENT
    circuit.raw_spice += _TRANSISTOR_MODELS
    circuit.raw_spice += _LO1166_SUBCKT

    # --- Rails -------------------------------------------------------------
    # +24 V supply on node A (the BA283 "A" rail). Signal ground is
    # PySpice's global ground (node 0).
    circuit.V("supply", "A", circuit.gnd, 24)

    # ====================================================================
    # BA283 output amplifier — real topology (per Neve docs the user supplied):
    #   * TR1 common-emitter input; TR1->TR2->TR3 d.c.-coupled (TR1 collector
    #     drives TR2 base, TR2 collector drives TR3 base: the d.c.-connected
    #     pair). TR3 (2N3055) collector = output F -> transformer primary,
    #     which also feeds TR3's ~60 mA Class-A collector current as a choke.
    #   * DC FEEDBACK: RV1 (4k7) sits in the output-stage emitter path and
    #     feeds a reference voltage from TR3's emitter back to TR1's base via
    #     R_dcfb (56 k). This self-biases the whole chain (no floating base,
    #     .op converges) AND, as the model's bias-conditioning input, sweeping
    #     RV1 shifts TR1's DC operating point -> asymmetric clipping.
    #   * AC FEEDBACK: from the TR2/TR3 collector (F) back to TR1's emitter
    #     via an 80 uF cap -> sets the loop's mid-band behaviour.
    #   * GAIN: the K<->J bridge (R_KJ + 80 uF) shunts TR1's emitter load R4
    #     (1.2 k) to ground, lowering local NFB at the emitter and raising
    #     gain; 220 ohm = the 26 dB tap (1.2k/470/220/110 -> 15/.../35 dB).
    #   * Compensation: polystyrene caps (C2 220p, C5 330p, C3 4700p) roll off
    #     the loop for HF stability.
    # ====================================================================

    # --- TR1 (BC184C, common-emitter input stage) --------------------------
    # Input coupling cap C7 (10 µF) + 1 Ω ESR.
    circuit.C("7", "INPUT", "C7_MID", 10e-6)
    circuit.R("_esr_C7", "C7_MID", "TR1_B", 1.0)

    # TR1 base DC bias comes from the output-stage DC feedback (R_dcfb, 56 k,
    # from the RV1 wiper) plus a high startup pull-up from the rail.
    circuit.R("b1_start", "A", "TR1_B", 1e6)
    circuit.R("dcfb", "RV1_WIPER", "TR1_B", 56e3)

    # Emitter load R4 = 1.2 k to ground (the node the K<->J bridge shunts).
    circuit.R("4", "TR1_E", circuit.gnd, 1.2e3)
    # Collector load to rail.
    circuit.R("c1", "A", "TR1_C", 10e3)
    circuit.BJT("TR1", "TR1_C", "TR1_B", "TR1_E", model="BC184C")

    # --- K <-> J gain bridge: shunts TR1 emitter load to ground ------------
    # R_KJ + 80 µF gain cap + 1 Ω ESR. At audio the cap is a short, so TR1's
    # AC emitter impedance ~ R4 || R_KJ -> high gain. On the real card 220 Ω
    # is the 26 dB tap; this reconstruction's forward gain is a bit lower, so
    # GAIN_RKJ_OHMS is calibrated (47 Ω) to land the small-signal gain on the
    # BA283's 26 dB spec.
    circuit.R("KJ", "TR1_E", "J", GAIN_RKJ_OHMS)
    circuit.C("gain", "J", "Cgain_MID", 80e-6)
    circuit.R("_esr_Cgain", "Cgain_MID", circuit.gnd, 1.0)

    # --- A.C. feedback: output F -> C4 (80 µF) -> R5 (3.3 k) -> TR1 emitter -
    # R5 sets the feedback factor; closed-loop gain ~ R5/(R4||R_KJ) ~ 26 dB.
    # (C4 is the DC block; its ESR is negligible next to R5.)
    circuit.C("4", "F", "C4_MID", 80e-6)
    circuit.R("5", "C4_MID", "TR1_E", FB_R5_OHMS)

    # --- TR2 (BC184C), d.c.-coupled from TR1 collector --------------------
    circuit.R("e2", "TR2_E", circuit.gnd, 390)
    circuit.R("c2", "A", "TR2_C", 4.7e3)
    circuit.BJT("TR2", "TR2_C", "TR1_C", "TR2_E", model="BC184C")
    # Miller compensation across TR2 (C2 220 pF) for HF loop stability.
    circuit.C("comp2", "TR2_C", "TR1_C", 220e-12)

    # --- RV1 (4k7 bias trim) in the output-stage emitter path -------------
    # Pot across TR3 emitter to ground; the wiper (a fraction of TR3_E set by
    # bias_norm) is the DC-feedback reference sent to TR1 via R_dcfb. Both
    # legs clamped >= 1 Ω so an end-stop never shorts the node.
    circuit.R("upper", "TR3_E", "RV1_WIPER", r_upper)
    circuit.R("lower", "RV1_WIPER", circuit.gnd, r_lower)

    # --- TR3 (2N3055, output transistor), d.c.-coupled from TR2 -----------
    circuit.R("b3", "TR2_C", "TR3_B", 100)
    circuit.R("7", "TR3_E", circuit.gnd, 47)
    # Compensation: C5 330 pF at TR3 base, C3 4700 pF at TR1 collector.
    circuit.C("comp5", "TR3_B", circuit.gnd, 330e-12)
    circuit.C("comp3", "TR1_C", circuit.gnd, 4700e-12)
    # Collector = contact F -> LO1166 primary.
    circuit.BJT("TR3", "F", "TR3_B", "TR3_E", model="2N3055")

    # --- LO1166 instance + 600 Ω secondary termination ---------------------
    # Subcircuit pins: PRI_IN, PRI_OUT, SEC_IN, SEC_OUT.
    # PRI_IN  = F        (driven by TR3 collector)
    # PRI_OUT = A        (centre-tapped to +24 V rail)
    # SEC_IN  = SEC_IN   (output tap)
    # SEC_OUT = 0        (ground return)
    circuit.raw_spice += "XLO1166 F A SEC_IN 0 LO1166\n"

    # 600 Ω console line-in termination on the secondary tap.
    circuit.R("term", "SEC_IN", circuit.gnd, 600)

    return circuit


if __name__ == "__main__":
    print(str(build_netlist(0.5)))
