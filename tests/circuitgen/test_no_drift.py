"""Drift guard: the committed generated backends must match the IR exactly.

If this fails, someone hand-edited a generated file or changed a `circuit/*.toml`
IR without regenerating. Fix with::

    uv run --no-project python -m tools.circuitgen

These tests are stdlib-only (no torch/project deps) so they run fast in CI.
Coverage = every circuit in the registry (currently the 303 VCO + VCF SPICE/ACME
backends); BA283 was removed when the repo became the standalone 303 project.
"""

from __future__ import annotations

import pytest

from tools.circuitgen.ir import load
from tools.circuitgen.targets import CIRCUITS, REPO_ROOT, render_circuits


def _registry_items():
    """(label, ir_name, dest_path) for every backend of every existing circuit."""
    items = []
    for ir_path, targets in CIRCUITS.items():
        if not ir_path.exists():
            continue
        for label, _gen, dest in targets:
            items.append((label, ir_path.name, dest))
    return items


_ITEMS = _registry_items()


@pytest.fixture(scope="module")
def rendered():
    return render_circuits()


@pytest.mark.parametrize(
    "label,ir_name,path",
    _ITEMS,
    ids=[f"{ir}:{lbl}" for lbl, ir, _p in _ITEMS],
)
def test_generated_backend_matches_ir(rendered, label, ir_name, path):
    assert path in rendered, f"{ir_name}:{label}: not produced by render_circuits()"
    assert path.exists(), f"{ir_name}:{label}: generated file missing: {path}"
    committed = path.read_text()
    expected = rendered[path]
    if committed != expected:
        rel = path.relative_to(REPO_ROOT)
        pytest.fail(
            f"{ir_name}:{label}: {rel} is stale — regenerate with "
            f"`uv run --no-project python -m tools.circuitgen`"
        )


def test_every_generated_file_carries_the_do_not_edit_marker(rendered):
    for path, text in rendered.items():
        assert "DO NOT EDIT" in text, f"{path} missing generated-file marker"


def test_registry_is_non_empty():
    # Guards against a future edit that silently drops every circuit from the
    # registry (which would make the drift guard vacuously pass).
    assert _ITEMS, "circuit registry is empty — no backends to drift-guard"


@pytest.mark.parametrize(
    "ir_path",
    [p for p in CIRCUITS if p.exists()],
    ids=[p.name for p in CIRCUITS if p.exists()],
)
def test_ir_validates(ir_path):
    # load() runs full structural validation; a raise here is a real defect.
    circ = load(ir_path)
    assert circ.meta["name"]
    assert len(circ.nodes) == circ.meta["num_nodes"]
