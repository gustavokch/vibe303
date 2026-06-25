"""Drift guard: the committed generated backends must match the IR exactly.

If this fails, someone hand-edited a generated file or changed the IR without
regenerating. Fix with::

    uv run --no-project python -m tools.circuitgen

These tests are stdlib-only (no torch/project deps) so they run fast in CI.
"""

from __future__ import annotations

import pytest

from tools.circuitgen.ir import load
from tools.circuitgen.targets import IR_PATH, REPO_ROOT, TARGETS, render_all


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
