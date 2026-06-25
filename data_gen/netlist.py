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

# Component model cards, the LO1166 subckt and all element stamping are generated from
# circuit/ba283.toml by tools/circuitgen (single source of truth); see data_gen/_netlist_gen.py.
from data_gen import _netlist_gen

# Gain-setting K<->J network: R_KJ (220 Ohm = the real card's 26 dB tap) in
# series with the 80 uF gain cap shunts TR1's emitter load R4 for a.c. R5 is the
# a.c.-feedback resistor (common collector F -> C4 -> R5 -> TR1 emitter).
FB_R5_OHMS = 3.3e3
GAIN_RKJ_OHMS = 220.0

# --- Gain-sweep dataset parameters -----------------------------------------
# The conditioning axis is GAIN, not bias: RV1 is the real card's set-once bias
# trim, so it is pinned at the ~65 mA / symmetric-clipping calibration while the
# K<->J gain resistor R_KJ is swept. BIAS_NORM_FIXED maps (via the RV1 remap) to
# a wiper of ~0.84 -> ~63 mA TR3 idle (within the 60-70 mA spec).
BIAS_NORM_FIXED = 0.77

# Gain knob -> R_KJ map. The K<->J slot accepts any external R; gain_norm in
# [0,1] maps log-linearly (≈ constant dB/step) from ~open (min gain ~14 dB at
# the secondary) to 82 Ω (max gain ~33 dB). A LOWER R_KJ bypasses more of TR1's
# emitter degeneration -> higher closed-loop gain.
GAIN_RKJ_MAX = 10_000.0   # gain_norm 0.0 -> ~14 dB (K<->J ~ open)
GAIN_RKJ_MIN = 82.0       # gain_norm 1.0 -> ~33 dB


# STEPPED (WS4) -------------------------------------------------------------
# The 5 hardware gain-switch detents the plugin now exposes, as R_KJ (ohms),
# idx 0..4 = 15/20/25/30/35 dB (balanced). idx 0 is the "open" sentinel (1e9 Ω).
# These replace the continuous gain_norm sweep for the stepped retrain; the
# continuous gain_norm_to_rkj path above is left intact for the legacy dataset.
STEPPED_RKJ_OHMS: tuple[float, ...] = (1.0e9, 1200.0, 470.0, 220.0, 110.0)

# Embedded WDF makeup-gain table (idx 0..4) = 1 / G_i, the small-signal output
# scale per position that level-norm divides out. Recorded here so the WDF and
# the NN share ONE reference. The data-gen wet normalization (see
# data_gen.run_spice.run_stepped_batch) should MEASURE G_i per position and
# assert it matches 1 / STEPPED_MAKEUP, so a netlist drift is caught instead of
# silently re-scaling the targets away from the WDF's reference.
STEPPED_MAKEUP: tuple[float, ...] = (
    0.202892,
    0.118732,
    0.073245,
    0.043799,
    0.026136,
)


def gain_norm_to_rkj(gain_norm: float) -> float:
    """Map a normalized gain knob in ``[0, 1]`` to the K<->J resistor (ohms).

    Log-linear, so equal knob steps give roughly equal dB steps:
    ``0.0 -> GAIN_RKJ_MAX`` (minimum gain), ``1.0 -> GAIN_RKJ_MIN`` (maximum).
    """
    g = min(1.0, max(0.0, gain_norm))
    return GAIN_RKJ_MAX * (GAIN_RKJ_MIN / GAIN_RKJ_MAX) ** g


def rkj_to_gain_norm(rkj_ohms: float) -> float:
    """Inverse of :func:`gain_norm_to_rkj`: map a K<->J resistor (ohms) back onto
    the normalized gain axis ``[0, 1]`` the conditioned model is trained on.

    Log-linear, clamped to ``[0, 1]``: the open-detent sentinel (``1e9 Ω``) and
    anything above ``GAIN_RKJ_MAX`` saturate to ``0.0`` (lowest gain); anything
    below ``GAIN_RKJ_MIN`` saturates to ``1.0``. Mirrors the Rust plugin's
    ``rgain_to_gain_norm`` (same ``GAIN_RKJ_MAX``/``MIN`` endpoints) so the data,
    the eval, and the plugin all feed the model ONE conditioning value per R_KJ.
    """
    import math
    r = max(1.0, float(rkj_ohms))
    g = math.log(r / GAIN_RKJ_MAX) / math.log(GAIN_RKJ_MIN / GAIN_RKJ_MAX)
    return min(1.0, max(0.0, g))


# Input-gain conditioning axis (WS4 dual conditioning). Mirrors the plugin's
# ``input_gain`` FloatParam range (lib.rs: Linear -24..+24 dB). The conditioned
# model takes a SECOND continuous channel = the pre-circuit input trim, normalized
# to ``[0, 1]`` over this range so 0 dB (unity) maps to 0.5.
INPUT_GAIN_DB_MIN = -24.0
INPUT_GAIN_DB_MAX = 24.0


def input_gain_db_to_norm(db: float) -> float:
    """Map an input-gain in dB onto ``[0, 1]`` over ``[-24, +24] dB`` (clamped).
    0 dB (unity / level-norm-ON default) -> 0.5."""
    g = (float(db) - INPUT_GAIN_DB_MIN) / (INPUT_GAIN_DB_MAX - INPUT_GAIN_DB_MIN)
    return min(1.0, max(0.0, g))

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


def build_netlist(bias_norm: float, gain_rkj: float = GAIN_RKJ_OHMS) -> Circuit:
    """Build the BA283 AM netlist with RV1 wiper at ``bias_norm``.

    Parameters
    ----------
    bias_norm:
        Bias-conditioning position in ``[0.0, 1.0]``, remapped to the stable
        portion of the RV1 pot (``RV1_WIPER_MIN``..1.0). 0.0 = hottest usable
        TR3 idle, 1.0 = coolest; the real card's ~65 mA symmetric-clipping
        setting falls near the upper end. Each pot leg is clamped to at least
        1 Ω so ngspice never sees a true short.
    gain_rkj:
        The K<->J gain-shunt resistor R_KJ in ohms (default
        ``GAIN_RKJ_OHMS`` = 220 = the real card's 26 dB tap). This is the
        gain-conditioning knob: a LOWER R_KJ shunts more of TR1's emitter
        degeneration for a.c., cutting local feedback and RAISING closed-loop
        gain (and drive into the TR2/TR3 nonlinearity); a higher R_KJ lowers
        it. Bias is held fixed while this is swept for the gain dataset.

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

    # Model cards, LO1166 subckt and all passive/active elements come from the generated
    # module (single source of truth in circuit/ba283.toml). build_netlist keeps only the
    # parametric plumbing: the RV1 bias split, the gain resistor, and the +24 V rail source.
    circuit.raw_spice += _ESR_COMMENT
    circuit.raw_spice += _netlist_gen.TRANSISTOR_MODELS
    circuit.raw_spice += _netlist_gen.LO1166_SUBCKT
    # +24 V supply on node A; PySpice global ground = node 0. Audio injection (filesource on
    # node INPUT) is layered in by run_spice.py.
    circuit.V("supply", "A", circuit.gnd, 24)
    _netlist_gen.add_elements(circuit, r_upper=r_upper, r_lower=r_lower, gain_rkj=gain_rkj)

    return circuit


if __name__ == "__main__":
    print(str(build_netlist(0.5)))
