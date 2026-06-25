"""Registry of generated backends: (emitter, output path relative to repo root).

Single source of truth for BOTH the regenerator (``python -m tools.circuitgen``)
and the drift-guard test, so they can never disagree about what is generated.
"""

from __future__ import annotations

from pathlib import Path

from . import emit_acme, emit_rust, emit_spice
from .ir import Circuit

# Repo root = three parents up from this file (tools/circuitgen/targets.py).
REPO_ROOT = Path(__file__).resolve().parents[2]
IR_PATH = REPO_ROOT / "circuit" / "ba283.toml"

# (label, generate(circuit)->str, output path)
TARGETS = [
    ("rust", emit_rust.generate, REPO_ROOT / "plugin" / "src" / "wdf" / "topology_gen.rs"),
    ("spice", emit_spice.generate, REPO_ROOT / "data_gen" / "_netlist_gen.py"),
    ("acme", emit_acme.generate, REPO_ROOT / "eval" / "acme_vs_spice" / "_ba283_gen.jl"),
]


def render_all(circuit: Circuit) -> dict[Path, str]:
    """Return {output_path: generated_text} for every backend."""
    return {path: gen(circuit) for _label, gen, path in TARGETS}
