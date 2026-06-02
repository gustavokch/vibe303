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
