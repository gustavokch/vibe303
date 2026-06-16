#!/usr/bin/env python3
"""Voicing-sweep gate for the split HYST + SAT transformer dials (feature `trafo-core`).

Consumes the `dump_voicing_grid` diagnostic (plugin/tests/test_trafo.rs), which emits one block per
HYST×SAT cell:
    # hyst=<h> sat=<s> thd=<t>
    # H,B
    <H>,<B>
    ...

No nonlinear ground truth exists (the reference netlist transformer is linear), so this is a
*plausibility + monotonicity* gate that also prints the per-cell metrics used to finalize the dial
endpoint constants in `JaCore::set_voicing`:

  • SATURATION axis (fixed HYST=0.5): tightening the knee must raise THD and compress peak flux
    (|B|max non-increasing) monotonically — the saturating-transformer signature.
  • HYSTERESIS axis (fixed SAT=0.5): the loop must do work — area must change monotonically and the
    dial must actually move the output (non-degenerate).
  • DEFAULT cell (0.5, 0.5) is reported verbatim as the "today" anchor for the equivalence check.

Pure stdlib. Prints a grid table + PASS/FAIL. Run via:
    python -m eval.trafo_core.sweep_voicing            # builds + runs the dump
    python -m eval.trafo_core.sweep_voicing FILE       # analyze a saved dump
"""
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]


def run_dump():
    cmd = [
        "cargo", "test", "-p", "ba283", "--features", "trafo-core",
        "--test", "test_trafo", "dump_voicing_grid", "--", "--ignored", "--nocapture",
    ]
    res = subprocess.run(cmd, cwd=REPO, capture_output=True, text=True)
    if res.returncode != 0:
        sys.stderr.write(res.stderr)
        raise SystemExit(f"dump_voicing_grid failed (rc={res.returncode})")
    return res.stdout


def parse(text):
    """{(hyst,sat): {'thd':float, 'pts':[(H,B)...]}}, insertion order preserved."""
    blocks, key = {}, None
    for line in text.splitlines():
        line = line.strip()
        if line.startswith("# hyst="):
            toks = dict(t.split("=") for t in line[2:].split())
            key = (float(toks["hyst"]), float(toks["sat"]))
            blocks[key] = {"thd": float(toks["thd"]), "pts": []}
        elif line.startswith("#") or not line:
            continue
        elif key is not None:
            try:
                h, b = (float(x) for x in line.split(","))
                blocks[key]["pts"].append((h, b))
            except ValueError:
                continue
    return blocks


def loop_area(pts):
    n = len(pts)
    if n < 3:
        return 0.0
    s = sum(pts[i][0] * pts[(i + 1) % n][1] - pts[(i + 1) % n][0] * pts[i][1] for i in range(n))
    return abs(s) / 2.0


def metrics(cell):
    pts = cell["pts"]
    bs = [b for _, b in pts]
    hs = [h for h, _ in pts]
    return {
        "thd": cell["thd"],
        "bmax": max(abs(b) for b in bs) if bs else 0.0,
        "hmax": max(abs(h) for h in hs) if hs else 0.0,
        "area": loop_area(pts),
        "finite": all(abs(v) < 1e30 for v in bs + hs),
        "n": len(pts),
    }


def mono_nonincr(xs):
    return all(xs[i] >= xs[i + 1] - 1e-6 for i in range(len(xs) - 1))


def mono_change(xs):
    return len(xs) >= 2 and abs(xs[0] - xs[-1]) > 1e-9


def main():
    text = open(sys.argv[1]).read() if len(sys.argv) > 1 else run_dump()
    blocks = parse(text)
    if not blocks:
        print("no voicing-grid blocks parsed", file=sys.stderr)
        return 2
    m = {k: metrics(v) for k, v in blocks.items()}

    print(f"{'hyst':>5} {'sat':>5} {'thd':>10} {'|B|max':>10} {'|H|max':>10} "
          f"{'area':>11} {'fin':>4} {'n':>4}")
    for (h, s), v in sorted(m.items()):
        print(f"{h:5.2f} {s:5.2f} {v['thd']:10.5f} {v['bmax']:10.6f} {v['hmax']:10.3f} "
              f"{v['area']:11.4e} {str(v['finite']):>4} {v['n']:4d}")

    all_finite = all(v["finite"] for v in m.values())

    # SATURATION axis at HYST=0.5: knee tightens with sat → THD up, |B|max compresses.
    sat_axis = [m[(0.5, s)] for s in (0.0, 0.5, 1.0) if (0.5, s) in m]
    sat_thd = [c["thd"] for c in sat_axis]
    sat_bmax = [c["bmax"] for c in sat_axis]
    sat_thd_up = mono_nonincr([-t for t in sat_thd]) and mono_change(sat_thd)
    sat_compress = mono_nonincr(sat_bmax) and mono_change(sat_bmax)

    # HYSTERESIS axis at SAT=0.5: loop area must move monotonically and be non-degenerate.
    hyst_axis = [m[(h, 0.5)] for h in (0.0, 0.5, 1.0) if (h, 0.5) in m]
    hyst_area = [c["area"] for c in hyst_axis]
    hyst_area_mono = (mono_nonincr(hyst_area) or mono_nonincr(hyst_area[::-1])) and mono_change(hyst_area)

    default = m.get((0.5, 0.5))
    ok = all_finite and sat_thd_up and sat_compress and hyst_area_mono

    print()
    print(f"SAT axis  thd  = {[round(t, 5) for t in sat_thd]}  -> rises:    {sat_thd_up}")
    print(f"SAT axis  |B|max = {[round(b, 5) for b in sat_bmax]}  -> compresses: {sat_compress}")
    print(f"HYST axis area = {['%.3e' % a for a in hyst_area]}  -> monotone:  {hyst_area_mono}")
    if default:
        print(f"DEFAULT (0.5,0.5) anchor: thd={default['thd']:.5f} "
              f"|B|max={default['bmax']:.5f} area={default['area']:.4e}")
    print(f"finite={all_finite}")
    print(f"\nVoicing sweep: {'PASS' if ok else 'FAIL'}")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
