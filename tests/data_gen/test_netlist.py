import re

import pytest

from data_gen.netlist import build_netlist


def test_build_netlist_returns_circuit_with_expected_nodes():
    circuit = build_netlist(bias_norm=0.5)
    netlist_str = str(circuit)
    for token in ("Vsupply", "BC184C", "2N3055", "LO1166", "Rterm", "C_esr"):
        assert token in netlist_str, f"netlist missing token {token!r}\n{netlist_str}"


def test_bias_changes_rv1_split():
    low = str(build_netlist(0.1))
    high = str(build_netlist(0.9))
    assert low != high


@pytest.mark.parametrize("bias", [-0.1, 1.1, 2.0])
def test_bias_out_of_range_raises(bias):
    with pytest.raises(ValueError):
        build_netlist(bias_norm=bias)


def _resistor_value(netlist_str: str, ref: str) -> float:
    """Return the resistance value (ohms) of resistor ``ref`` from a render.

    Matches a SPICE element line of the form
    ``<ref> <node1> <node2> <value>`` and parses the final token as a float.
    """
    pattern = rf"(?m)^{re.escape(ref)}\s+\S+\s+\S+\s+([0-9.eE+-]+)\s*$"
    match = re.search(pattern, netlist_str)
    assert match is not None, f"resistor {ref!r} not found in netlist:\n{netlist_str}"
    return float(match.group(1))


def test_esr_resistors_present():
    """The three real ESR resistors must appear as actual elements.

    Catches deletion of an ESR resistor, not just the C_esr comment line.
    """
    s = str(build_netlist(0.5))
    for ref in ("R_esr_C7", "R_esr_C4", "R_esr_Cgain"):
        # Assert the element line exists (ref at start of line, with nodes/value).
        assert re.search(
            rf"(?m)^{re.escape(ref)}\s+\S+\s+\S+\s+\S+", s
        ), f"ESR resistor element {ref!r} missing from netlist:\n{s}"


def test_rv1_bias_mapping_values():
    """RV1 leg resistances must follow R_upper=4700*(1-b), R_lower=4700*b.

    Catches an inverted or mis-scaled bias map. Read directly from the
    rendered Rupper/Rlower element lines.
    """
    tol = 1.0

    s0 = str(build_netlist(0.0))
    assert _resistor_value(s0, "Rupper") == pytest.approx(4700.0, abs=tol)
    assert _resistor_value(s0, "Rlower") == pytest.approx(1.0, abs=tol)

    s1 = str(build_netlist(1.0))
    assert _resistor_value(s1, "Rupper") == pytest.approx(1.0, abs=tol)
    assert _resistor_value(s1, "Rlower") == pytest.approx(4700.0, abs=tol)

    s_mid = str(build_netlist(0.5))
    assert _resistor_value(s_mid, "Rupper") == pytest.approx(2350.0, abs=tol)
    assert _resistor_value(s_mid, "Rlower") == pytest.approx(2350.0, abs=tol)


def test_clamp_keeps_legs_at_least_one_ohm():
    """Neither RV1 leg may render below 1 ohm (no rail-to-ground short)."""
    for bias in (0.0, 1.0):
        s = str(build_netlist(bias))
        assert _resistor_value(s, "Rupper") >= 1.0
        assert _resistor_value(s, "Rlower") >= 1.0


def test_lo1166_instance_line_present():
    """The hand-written transformer instance line must keep its pin order.

    Pins: PRI_IN=F, PRI_OUT=A, SEC_IN=SEC_IN, SEC_OUT=0, subckt LO1166.
    This is the single hand-written instance line and the most likely
    silent break point.
    """
    s = str(build_netlist(0.5))
    assert re.search(
        r"(?m)^XLO1166\s+F\s+A\s+SEC_IN\s+0\s+LO1166\s*$", s
    ), f"LO1166 instance line missing or pin order wrong:\n{s}"


def test_lo1166_linear_mutual_model():
    """Transformer must be the linear mutual-inductance (K-coupled) form (v1).

    Asserts the two coupled windings (L_pri/L_sec), the coupling line (K1) and
    the LP/LS params (turns ratio set by LS = N^2*LP). v1 is intentionally
    linear -- no flux integrator or tanh saturation source (those wound up on
    the primary DC and latched the core) and no E/F algebraic-loop sources
    (those went near-oscillatory at the sweep HF tail).
    """
    s = str(build_netlist(0.5))
    for token in ("L_pri", "L_sec", "K1", "LP=", "LS="):
        assert token in s, f"transformer token {token!r} missing:\n{s}"
    # The mis-wired XSPICE magnetics and PSpice CORE-model-on-K idiom must be gone.
    assert "lcouple" not in s
    assert "CORE(MS=" not in s
    assert "K1 L_pri L_sec LO1166_CORE" not in s
    # No flux-integrator/tanh windup machinery and no E/F ideal-xfmr sources.
    assert "tanh" not in s
    assert "E_sec" not in s
