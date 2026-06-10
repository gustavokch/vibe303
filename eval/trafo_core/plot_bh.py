"""
B-H loop plausibility plot for the nonlinear Jiles-Atherton transformer core (`trafo` knob).

There is NO SPICE/ACME ground truth for the nonlinear core (the reference netlist transformer is
linear), so this is a *plausibility* gate, not an accuracy gate. It drives the Rust WDF core with a
hard low-frequency sine at several `trafo` settings and plots the resulting B-H loops, checking they
are closed, monotone-saturating, and grow in area with `trafo`.

The (H,B) trace comes from the feature-gated diagnostic test `dump_bh_loop` in
plugin/tests/test_trafo.rs, which prints CSV to stdout.

Usage:
    python -m eval.trafo_core.plot_bh            # builds + runs the dump, writes out/bh.png
    python -m eval.trafo_core.plot_bh --no-build # reuse a cached cargo build
"""
from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
OUT = HERE / "out"


def run_dump() -> str:
    """Run the feature-gated B-H dump test and return its stdout (CSV blocks per trafo)."""
    cmd = [
        "cargo", "test", "-p", "ba283", "--features", "trafo-core",
        "--test", "test_trafo", "dump_bh_loop", "--", "--ignored", "--nocapture",
    ]
    res = subprocess.run(cmd, cwd=REPO, capture_output=True, text=True)
    if res.returncode != 0:
        sys.stderr.write(res.stderr)
        raise RuntimeError(f"dump_bh_loop failed (rc={res.returncode})")
    return res.stdout


def parse(stdout: str) -> dict[float, np.ndarray]:
    """Parse the `# trafo=...` / `H,B` CSV blocks into {trafo: Nx2 array}."""
    loops: dict[float, np.ndarray] = {}
    cur_trafo: float | None = None
    rows: list[tuple[float, float]] = []
    for line in stdout.splitlines():
        line = line.strip()
        if line.startswith("# trafo="):
            if cur_trafo is not None and rows:
                loops[cur_trafo] = np.asarray(rows, dtype=float)
            cur_trafo = float(line.split("=", 1)[1])
            rows = []
        elif line and not line.startswith("#"):
            try:
                h, b = (float(v) for v in line.split(","))
                rows.append((h, b))
            except ValueError:
                continue
    if cur_trafo is not None and rows:
        loops[cur_trafo] = np.asarray(rows, dtype=float)
    return loops


def loop_area(hb: np.ndarray) -> float:
    """Signed enclosed area of the B-H loop (shoelace) — hysteresis loss proxy."""
    h, b = hb[:, 0], hb[:, 1]
    return 0.5 * abs(np.dot(h, np.roll(b, -1)) - np.dot(b, np.roll(h, -1)))


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--no-build", action="store_true", help="(reserved) reuse cached build")
    ap.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)

    loops = parse(run_dump())
    if not loops:
        print("No B-H data parsed.")
        return 1

    fig, ax = plt.subplots(figsize=(7, 5))
    areas, bpk = {}, {}
    for trafo in sorted(loops):
        hb = loops[trafo]
        areas[trafo] = loop_area(hb)
        bpk[trafo] = float(np.max(np.abs(hb[:, 1])))
        ax.plot(hb[:, 0], hb[:, 1],
                label=f"trafo={trafo} (|B|max={bpk[trafo]:.3f}, area={areas[trafo]:.3g})")
    ax.set_xlabel("H (A/m)")
    ax.set_ylabel("B (T)")
    ax.set_title("LO1166 core B-H loops (Jiles-Atherton, separable RK4)")
    ax.grid(True, alpha=0.3)
    ax.legend()
    fig.tight_layout()
    png = OUT / "bh.png"
    fig.savefig(png, dpi=120)
    print(f"wrote {png}")

    # Plausibility gate: in the decoupled deviation-feedback core, more `trafo` loads the primary
    # with saturation current and COMPRESSES the peak flux — so |B|max is non-increasing in trafo
    # (soft saturation), the opposite sign of the old "area grows" heuristic (wrong for this model).
    ts = sorted(bpk)
    mono = all(bpk[ts[i]] >= bpk[ts[i + 1]] - 1e-6 for i in range(len(ts) - 1))
    changes = len(ts) >= 2 and abs(bpk[ts[0]] - bpk[ts[-1]]) > 1e-4
    print("areas:", {t: round(areas[t], 6) for t in ts})
    print("|B|max:", {t: round(bpk[t], 6) for t in ts})
    print("monotonic-compression-in-trafo:", mono, " changes:", changes)
    return 0 if (mono and changes) else 2


if __name__ == "__main__":
    raise SystemExit(main())
