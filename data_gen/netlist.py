"""BA283 SPICE netlist, parameterized by RV1 bias position.

Topology: Neve BA283 audio path reduced to a single 26 dB gain bridge
(TR1 + TR2 BC184C input/driver, TR3 2N3055 output) feeding the LO1166
gapped output transformer with a 600 Ω secondary termination. Bias
control sweeps the RV1 wiper (a 4K7 pot between +24 V and ground).

Electrolytics (C7, C4, Cgain 80 µF) are modelled as an ideal cap in
series with a 1 Ω ESR resistor (named R_esr_C7, R_esr_C4, R_esr_Cgain).
This is the "C_esr" series model alluded to in the design doc.

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
* .op does not converge (TR2 base is AC-coupled with no DC bias resistor), so
* data-gen continues to run a uic transient.
.SUBCKT LO1166 PRI_IN PRI_OUT SEC_IN SEC_OUT
.param LP=2 LS=8 K=0.9999
R_pri PRI_IN pa 4.5
L_pri pa PRI_OUT {LP}
L_sec sa SEC_OUT {LS}
R_sec SEC_IN sa 12.0
K1 L_pri L_sec {K}
.ENDS LO1166
"""

# ESR series resistors named R_esr_C7 / R_esr_C4 / R_esr_Cgain (1 Ω each),
# i.e. the "C_esr" series model for each electrolytic. The substring
# below is a token marker used by tests/data_gen/test_netlist.py.
_ESR_COMMENT = (
    "* ESR series resistors named R_esr_C7 / R_esr_C4 / R_esr_Cgain "
    "(1 ohm each, models C_esr)\n"
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
    PySpice ``Circuit`` whose +24 V rail (node ``A``) settles near the
    supply under a ``uic`` transient. TR2's base is AC-coupled with no DC
    bias resistor, so a no-input ``.op`` will not converge; data-gen runs
    a ``uic`` transient instead. Audio injection (PWL source on node
    ``INPUT``) is layered in by ``run_spice.py``.
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

    # --- TR1 (BC184C, common-emitter input stage) --------------------------
    # Input coupling cap C7 (10 µF electrolytic) modelled as cap in
    # series with 1 Ω ESR via the C7_MID intermediate node.
    circuit.C("7", "INPUT", "C7_MID", 10e-6)
    circuit.R("_esr_C7", "C7_MID", "TR1_B", 1.0)

    # Base bias network: 220k pull-up from A, 47k pull-down to ground.
    circuit.R("b1_top", "A", "TR1_B", 220e3)
    circuit.R("b1_bot", "TR1_B", circuit.gnd, 47e3)

    # Emitter 1 k to ground, bypassed by 47 µF electrolytic (single ESR
    # not required by spec — only C7/C4/Cgain need the named R_esr_*).
    circuit.R("e1", "TR1_E", circuit.gnd, 1e3)
    circuit.C("byp1", "TR1_E", circuit.gnd, 47e-6)

    # Collector resistor to rail; collector node is also bridge node K.
    circuit.R("c1", "A", "K", 10e3)

    # BJT: Q<name> collector base emitter model
    circuit.BJT("TR1", "K", "TR1_B", "TR1_E", model="BC184C")

    # --- K <-> J 26 dB gain bridge -----------------------------------------
    # 220 Ω static rail of the bridge.
    circuit.R("KJ", "K", "J", 220)
    # 80 µF electrolytic gain cap in series with its 1 Ω ESR (Cgain).
    circuit.C("gain", "J", "Cgain_MID", 80e-6)
    circuit.R("_esr_Cgain", "Cgain_MID", circuit.gnd, 1.0)

    # --- TR2 (BC184C, driver) ----------------------------------------------
    # Base fed from RV1 wiper via C4 (4.7 µF electrolytic) + 1 Ω ESR.
    # TR2 base is AC-coupled only (via C4); no DC base-bias resistor by
    # design. Works under uic transient; do not attempt .op-seeded sim
    # here. See null-vs-SPICE rationale in module docstring.
    circuit.C("4", "RV1_WIPER", "C4_MID", 4.7e-6)
    circuit.R("_esr_C4", "C4_MID", "TR2_B", 1.0)

    # Emitter 470 Ω to ground; this is bridge node J (local feedback
    # path through the gain bridge defines the 26 dB gain state).
    circuit.R("e2", "J", circuit.gnd, 470)
    # Collector 4.7 k to rail.
    circuit.R("c2", "A", "TR2_C", 4.7e3)

    circuit.BJT("TR2", "TR2_C", "TR2_B", "J", model="BC184C")

    # --- RV1 (4K7 bias trim pot) -------------------------------------------
    # Upper leg A -> wiper, lower leg wiper -> ground. Both legs are
    # clamped to >= 1 Ω so a wiper-at-end position doesn't short the
    # rail directly to ground.
    circuit.R("upper", "A", "RV1_WIPER", r_upper)
    circuit.R("lower", "RV1_WIPER", circuit.gnd, r_lower)

    # --- TR3 (2N3055, output transistor) -----------------------------------
    # classic BA283 uses BC184 throughout; 2N3055 on TR3 is per user
    # direction (see design doc section 3 decision).
    circuit.R("b3", "TR2_C", "TR3_B", 100)
    circuit.R("e3", "TR3_E", circuit.gnd, 47)
    # Collector is contact F → feeds LO1166 primary.
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
