"""Drift guard: the committed generated backends must match the IR exactly.

If this fails, someone hand-edited a generated file or changed the IR without
regenerating. Fix with::

    uv run --no-project python -m tools.circuitgen

These tests are stdlib-only (no torch/project deps) so they run fast in CI.
"""

from __future__ import annotations

import re

import pytest

from tools.circuitgen.emit_acme import _model_fn_name
from tools.circuitgen.ir import load
from tools.circuitgen.targets import IR_PATH, REPO_ROOT, TARGETS, render_all

COMPONENTS_RS = REPO_ROOT / "plugin" / "src" / "wdf" / "components.rs"
WDF_MOD_RS = REPO_ROOT / "plugin" / "src" / "wdf" / "mod.rs"


@pytest.fixture(scope="module")
def rendered():
    return render_all(load(IR_PATH))


@pytest.mark.parametrize("label,_gen,path", TARGETS, ids=[t[0] for t in TARGETS])
def test_generated_backend_matches_ir(rendered, label, _gen, path):
    assert path.exists(), f"{label}: generated file missing: {path}"
    committed = path.read_text()
    expected = rendered[path]
    if committed != expected:
        rel = path.relative_to(REPO_ROOT)
        pytest.fail(
            f"{label}: {rel} is stale — regenerate with "
            f"`uv run --no-project python -m tools.circuitgen`"
        )


def test_every_generated_file_carries_the_do_not_edit_marker(rendered):
    for path, text in rendered.items():
        assert "DO NOT EDIT" in text, f"{path} missing generated-file marker"


def test_ir_validates():
    # load() runs full structural validation; a raise here is a real defect.
    circ = load(IR_PATH)
    assert circ.meta["name"] == "BA283"
    assert len(circ.nodes) == circ.meta["num_nodes"]


def _parse_bjt_ctor(text: str, fn: str) -> dict[str, float]:
    """Extract the numeric literal fields of a `pub fn <fn>() -> BjtParams` ctor in
    components.rs. Non-numeric fields (e.g. the `vcrit` expression, the spread, the
    derived dep-charge consts) are skipped — we only guard the card-derived values."""
    m = re.search(rf"pub fn {re.escape(fn)}\(\) -> BjtParams \{{(.*?)\n\}}", text, re.S)
    assert m, f"ctor {fn}() not found in components.rs"
    body = m.group(1)
    vals: dict[str, float] = {}
    for name, expr in re.findall(r"let (\w+)\s*=\s*([^;]+);", body):  # e.g. `let is = 1.04e-14;`
        try:
            vals[name] = float(expr)
        except ValueError:
            pass
    for name, expr in re.findall(r"(\w+):\s*([^,\n]+),", body):  # e.g. `bf: 600.0,`
        try:
            vals[name] = float(expr.strip())
        except ValueError:
            pass
    return vals


def test_rust_bjt_params_match_ir_card():
    """The hand-written `components.rs` BJT constructors are NOT generated, so the
    drift guard cannot see them. This asserts that the card-derived params stay equal
    to the IR `[model.*]` cards — editing the IR card and forgetting components.rs (or
    vice-versa) is exactly the 3-way drift this harness exists to kill."""
    circ = load(IR_PATH)
    text = COMPONENTS_RS.read_text()
    # fields copied verbatim from the SPICE .MODEL card into BjtParams
    DIRECT = ["is", "bf", "br", "vaf", "ikf", "cje", "vje", "mje", "cjc", "mjc", "tf", "tr"]
    vt_ir = float(circ.params["bjt_vt"])
    for name, model in circ.models.items():
        fn = _model_fn_name(name)
        vals = _parse_bjt_ctor(text, fn)
        card = model.params
        for k in DIRECT:
            assert k in card, f"IR card {name} missing {k}"
            assert k in vals, f"components.rs {fn}() missing {k}"
            assert vals[k] == card[k], f"{fn}().{k}={vals[k]} != IR card {k}={card[k]}"
        # card-optional fields fall back to the SPICE defaults the ctor hard-codes
        assert vals.get("var", 0.0) == card.get("var", 0.0), f"{fn}().var drift"
        assert vals.get("vjc", 0.75) == card.get("vjc", 0.75), f"{fn}().vjc drift"
        assert vals.get("ikr", 0.0) == 0.0, f"{fn}().ikr must be 0 (card has no IKR)"
        assert vals["vt"] == vt_ir, f"{fn}().vt={vals['vt']} != params.bjt_vt={vt_ir}"
        # alpha_f/alpha_r are rounded hand-derivations of bf/(bf+1), br/(br+1)
        assert vals["alpha_f"] == pytest.approx(card["bf"] / (card["bf"] + 1.0), abs=5e-4)
        assert vals["alpha_r"] == pytest.approx(card["br"] / (card["br"] + 1.0), abs=5e-4)


def test_rust_node_enum_order_matches_ir():
    """The node enum order is load-bearing (it fixes the MNA matrix layout) but the
    `node::N_*` enum in mod.rs is hand-written, not generated. Assert its order is
    identical to the IR node list so a silent reorder on either side is caught."""
    circ = load(IR_PATH)
    block = re.search(r"pub mod node \{(.*?)\n\}", WDF_MOD_RS.read_text(), re.S)
    assert block, "node module not found in mod.rs"
    enum = [
        (int(idx), name)
        for name, idx in re.findall(r"pub const N_(\w+): i32 = (\d+);", block.group(1))
    ]
    enum.sort()
    enum_names = [name for _i, name in enum]
    ir_names = [n.name for n in circ.nodes]
    assert enum_names == ir_names, (
        "node enum order drifted from circuit/ba283.toml:\n"
        f"  mod.rs: {enum_names}\n  IR:     {ir_names}"
    )
