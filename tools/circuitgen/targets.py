"""Registry of generated backends: (emitter, output path relative to repo root).

Single source of truth for BOTH the regenerator (``python -m tools.circuitgen``)
and the drift-guard test, so they can never disagree about what is generated.
"""

from __future__ import annotations

from pathlib import Path

from . import emit_acme, emit_spice
from .ir import load

# Repo root = three parents up from this file (tools/circuitgen/targets.py).
REPO_ROOT = Path(__file__).resolve().parents[2]

# --- multi-circuit registry --------------------------------------------------
# Each circuit lists only the backends it targets. The 303 sub-circuits emit
# SPICE + ACME only (the diode-ladder WDF solver is deferred to Phase 2). A
# circuit whose IR file does not yet exist is skipped, so dropping a
# circuit/<name>.toml in auto-enables it.
EVAL_303 = REPO_ROOT / "eval" / "303"

CIRCUITS: dict[Path, list] = {
    REPO_ROOT / "circuit" / "vco303.toml": [
        ("spice", emit_spice.generate, EVAL_303 / "_vco303_netlist_gen.py"),
        ("acme", emit_acme.generate, EVAL_303 / "_vco303_gen.jl"),
    ],
    REPO_ROOT / "circuit" / "vcf303.toml": [
        ("spice", emit_spice.generate, EVAL_303 / "_vcf303_netlist_gen.py"),
        ("acme", emit_acme.generate, EVAL_303 / "_vcf303_gen.jl"),
    ],
}


def render_circuits() -> dict[Path, str]:
    """Return {output_path: generated_text} across every registered circuit whose
    IR file exists. Used by the regenerator + drift guard."""
    out: dict[Path, str] = {}
    for ir_path, targets in CIRCUITS.items():
        if not ir_path.exists():
            continue
        circ = load(ir_path)
        for _label, gen, dest in targets:
            out[dest] = gen(circ)
    return out
