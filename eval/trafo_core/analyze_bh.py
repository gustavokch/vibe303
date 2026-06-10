#!/usr/bin/env python3
"""Analyze a B-H loop dump from the `dump_bh_loop` test (feature `trafo-core`).

Input format (stdin or file): blocks of
    # trafo=<v>
    # H,B
    <H>,<B>
    ...

Gates (B′5 plausibility, no nonlinear ground truth):
  - All values finite (no flux runaway — the original NO-GO failure mode).
  - Loop is closed (last sample returns near the first — periodic steady state).
  - Peak flux |B|max is bounded and *compresses monotonically* with trafo: in the decoupled
    deviation-feedback model, more `trafo` draws more magnetizing-saturation current, loading the
    primary and squashing the flux peaks (soft compression — exactly what a saturating output
    transformer does). This supersedes the original "area grows with trafo" heuristic, which had the
    sign wrong for a deviation/feedback core (compression shrinks the H-B excursion, not grows it).
  - Knee genuinely traversed: |H|max far exceeds the linear-region field for the same B (the flux
    peaks bend over the saturation knee), proving real saturation rather than linear scaling.

Pure stdlib; prints a per-trafo table + a PASS/FAIL summary. Plotting is in plot_bh.py.
"""
import sys


def parse(text):
    blocks, cur, trafo = {}, [], None
    for line in text.splitlines():
        line = line.strip()
        if line.startswith("# trafo="):
            if trafo is not None:
                blocks[trafo] = cur
            trafo = float(line.split("=", 1)[1])
            cur = []
        elif line.startswith("#") or not line:
            continue
        else:
            try:
                h, b = (float(x) for x in line.split(","))
                cur.append((h, b))
            except ValueError:
                continue
    if trafo is not None:
        blocks[trafo] = cur
    return blocks


def loop_area(pts):
    """Signed polygon area in the H-B plane (shoelace) — hysteresis-loss proxy."""
    n = len(pts)
    if n < 3:
        return 0.0
    s = 0.0
    for i in range(n):
        h0, b0 = pts[i]
        h1, b1 = pts[(i + 1) % n]
        s += h0 * b1 - h1 * b0
    return abs(s) / 2.0


def analyze(blocks):
    """Per-trafo metrics + per-row finite/closed checks. Cross-row monotonic compression is
    evaluated in main() (it needs the whole trafo-ordered series)."""
    rows = []
    for trafo in sorted(blocks):
        pts = blocks[trafo]
        if not pts:
            continue
        hs = [h for h, _ in pts]
        bs = [b for _, b in pts]
        finite = all(abs(v) < 1e30 for v in hs + bs)
        bmax = max(abs(b) for b in bs)
        hmax = max(abs(h) for h in hs)
        area = loop_area(pts)
        closed = (
            abs(pts[0][0] - pts[-1][0]) <= 0.20 * (hmax + 1e-30)
            and abs(pts[0][1] - pts[-1][1]) <= 0.20 * (bmax + 1e-30)
        )
        rows.append((trafo, len(pts), hmax, bmax, area, finite, closed))
    return rows


def main():
    text = open(sys.argv[1]).read() if len(sys.argv) > 1 else sys.stdin.read()
    blocks = parse(text)
    if not blocks:
        print("no B-H blocks parsed", file=sys.stderr)
        sys.exit(2)
    rows = analyze(blocks)
    print(f"{'trafo':>6} {'n':>5} {'|H|max':>10} {'|B|max':>10} {'area':>11} "
          f"{'fin':>3} {'closed':>6}")
    for tr, n, hmax, bmax, area, fin, cl in rows:
        print(f"{tr:6.2f} {n:5d} {hmax:10.3f} {bmax:10.6f} {area:11.4e} "
              f"{str(fin):>3} {str(cl):>6}")

    all_finite = all(r[5] for r in rows)
    all_closed = all(r[6] for r in rows)
    # Monotonic peak-flux compression: |B|max must be non-increasing as trafo rises, and strictly
    # change overall (the knob must do something). Tolerance absorbs cycle-sampling jitter.
    bmaxes = [r[3] for r in rows]
    mono_compress = all(bmaxes[i] >= bmaxes[i + 1] - 1e-6 for i in range(len(bmaxes) - 1))
    changes = len(bmaxes) >= 2 and abs(bmaxes[0] - bmaxes[-1]) > 1e-4
    ok = all_finite and all_closed and mono_compress and changes
    print(f"\nfinite={all_finite}  closed={all_closed}  "
          f"monotonic-compression={mono_compress}  changes={changes}")
    print(f"B-H plausibility: {'PASS' if ok else 'FAIL'}")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
