"""Closed-loop validation of a draft IR (Masala-CHAI V2 idea).

Two gates, cheapest first:
  1. STRUCTURAL — load the draft through tools.circuitgen.ir (node-reference
     integrity, no dangling nets, model refs, wdf_drop consistency). Stdlib only.
  2. SIMULATION (optional) — emit a flat SPICE deck and run ``ngspice -b`` for a
     DC operating point; flag non-convergence. Skipped if ngspice is absent.

A draft that fails either gate is rejected before a human reviews it.
"""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path

from tools.circuitgen import emit_spice
from tools.circuitgen.ir import GROUND, Circuit, load
from tools.circuitgen.rustfmt import py_f64


@dataclass
class ValidationResult:
    ok: bool
    structural_ok: bool
    sim_ran: bool
    sim_converged: bool
    messages: list[str]


def _flat_cir(c: Circuit) -> str:
    """A self-contained ngspice deck for a DC operating-point check."""
    sp = lambda n: "0" if n == GROUND else c.node(n).spice_name()
    lines = [".title BA283 draft validation", ""]
    # model cards + subckt (reuse the generated card formatter)
    for m in c.models.values():
        lines.append(emit_spice._spice_card(m))
    t = c.transformer
    r_pri = next(r for r in c.resistors if r.role == "xfmr_pri_dcr")
    r_sec = next(r for r in c.resistors if r.role == "xfmr_sec_dcr")
    lines += [
        f".subckt {t.name} PRI_IN PRI_OUT SEC_IN SEC_OUT",
        f".param LP={py_f64(t.lp)} LS={py_f64(t.ls)} K={py_f64(t.k)}",
        f"R_pri PRI_IN pa {py_f64(r_pri.ohms)}",
        "L_pri pa PRI_OUT {LP}",
        "L_sec sa SEC_OUT {LS}",
        f"R_sec SEC_IN sa {py_f64(r_sec.ohms)}",
        "K1 L_pri L_sec {K}",
        f".ends {t.name}",
    ]
    # rail
    lines.append(f"Vsupply {sp(c.meta['rail_node'])} 0 {py_f64(c.meta['rail_voltage'])}")
    # Driven inputs need a DC path for the operating point (they couple through a
    # series cap that is open at DC, else the node floats -> singular matrix). The
    # real backends drive these at runtime (ACME Vin / SPICE filesource); for the
    # .op check, tie each input node to a 0 V source.
    for n in c.nodes:
        if n.role == "input":
            lines.append(f"Vstim_{n.name} {sp(n.name)} 0 0")
    skip = {"xfmr_pri_dcr", "xfmr_sec_dcr"}
    for r in c.resistors:
        if r.role in skip:
            continue
        ohms = py_f64(r.ohms)  # params get their shipped default in the DC check
        lines.append(f"{r.name} {sp(r.nodes[0])} {sp(r.nodes[1])} {ohms}")
    for cap in c.capacitors:
        lines.append(f"{cap.name} {sp(cap.nodes[0])} {sp(cap.nodes[1])} {py_f64(cap.farads)}")
    for bj in c.bjts:
        lines.append(
            f"Q{bj.name} {sp(bj.collector)} {sp(bj.base)} {sp(bj.emitter)} {bj.model}"
        )
    # transformer instance
    pri_in = next(n for n in r_pri.nodes if n != t.pri_nodes[0])
    sec_in = next(n for n in r_sec.nodes if n != t.sec_nodes[0])
    pins = [pri_in, t.pri_nodes[1], sec_in, t.sec_nodes[1]]
    lines.append(f"X{t.name} {' '.join(sp(p) for p in pins)} {t.name}")
    lines += [".op", ".end", ""]
    return "\n".join(lines)


def validate(draft_path: str | Path, *, run_sim: bool = True) -> ValidationResult:
    msgs: list[str] = []
    # gate 1: structural
    try:
        circ = load(draft_path)
        structural_ok = True
        msgs.append(f"structural OK: {len(circ.nodes)} nodes, {len(circ.resistors)} R, "
                    f"{len(circ.capacitors)} C, {len(circ.bjts)} BJT")
    except Exception as e:  # noqa: BLE001 — surface any IR defect
        return ValidationResult(False, False, False, False, [f"structural FAIL: {e}"])

    # gate 2: ngspice .op (optional)
    sim_ran = sim_converged = False
    ngspice = shutil.which("ngspice")
    if not run_sim:
        msgs.append("simulation gate skipped (run_sim=False)")
    elif not ngspice:
        msgs.append("simulation gate skipped (ngspice not found on PATH)")
    else:
        with tempfile.TemporaryDirectory() as td:
            deck = Path(td) / "draft.cir"
            deck.write_text(_flat_cir(circ))
            proc = subprocess.run(
                [ngspice, "-b", str(deck)], capture_output=True, text=True, timeout=120
            )
            sim_ran = True
            out = (proc.stdout + proc.stderr).lower()
            bad = ("singular matrix" in out or "timestep too small" in out
                   or "no convergence" in out or "iteration limit reached" in out)
            sim_converged = proc.returncode == 0 and not bad
            msgs.append(f"ngspice .op: {'converged' if sim_converged else 'FAILED'} "
                        f"(rc={proc.returncode})")
            if not sim_converged:
                msgs.append(out[-800:])

    ok = structural_ok and (sim_converged or not (run_sim and ngspice))
    return ValidationResult(ok, structural_ok, sim_ran, sim_converged, msgs)
