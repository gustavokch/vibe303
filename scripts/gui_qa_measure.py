#!/usr/bin/env python3
"""Numeric measurement toolkit for the TECHNEO 303 GUI visual audit.

Reads capture PNGs and reports what vision alone cannot measure reliably:
panel geometry, dead space, knob arc-vs-needle agreement, text contrast and
glyph sizes, accent/selection presence, widget edge alignment, and per-state
pixel diffs.

Dependencies: numpy and Pillow only.

Usage:
    scripts/gui_qa_measure.py report    target/gui-qa
    scripts/gui_qa_measure.py knobs     target/gui-qa/latest.png
    scripts/gui_qa_measure.py colors    target/gui-qa/accurate.png
    scripts/gui_qa_measure.py text      target/gui-qa/latest.png --box 2555,110,2680,160
    scripts/gui_qa_measure.py accent    target/gui-qa/latest.png target/gui-qa/accurate.png
    scripts/gui_qa_measure.py deadspace target/gui-qa/latest.png --min 600
    scripts/gui_qa_measure.py diff      target/gui-qa/latest.png target/gui-qa/running.png
    scripts/gui_qa_measure.py edges     target/gui-qa/latest.png --box 2110,1418,2711,1702
    scripts/gui_qa_measure.py strokes   target/gui-qa/running.png --box 800,1370,1070,1660
    scripts/gui_qa_measure.py crop      target/gui-qa/latest.png --box 1440,190,1980,380 --scale 2 -o /tmp/env.png

Every command accepts --json to emit machine-readable output instead of text.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import re
import sys

import numpy as np
from PIL import Image

# ---------------------------------------------------------------------------
# Theme tokens, mirrored from plugin303/src/gui/theme.rs. Update together.
# ---------------------------------------------------------------------------

DEFAULT_THEME_RS = "plugin303/src/gui/theme.rs"

# Fallback if theme.rs cannot be read. Prefer parsing the real file: a hand
# copy of the palette drifts silently, and a stale entry turns a genuine palette
# drift finding into "matches a token".
THEME_FALLBACK = {
    "PANEL_BG": (0x1A, 0x1A, 0x1E),
    "WELL_BG": (0x16, 0x16, 0x19),
    "SURFACE_ELEVATED": (0x22, 0x22, 0x28),
    "BORDER_DEFAULT": (0x2B, 0x2B, 0x34),
    "TEXT_PRIMARY": (0xFF, 0xFF, 0xFF),
    "TEXT_MUTED": (0x8F, 0x8F, 0xA4),
    "ACCENT_ORANGE": (0xFF, 0x7A, 0x00),
}

THEME = dict(THEME_FALLBACK)


def load_theme_tokens(path: str = DEFAULT_THEME_RS):
    """Parse `pub const NAME: Color32 = Color32::from_rgb(..)` out of theme.rs."""
    global THEME
    if not os.path.exists(path):
        return THEME
    pattern = re.compile(
        r"pub const (\w+): Color32 = Color32::from_rgb\(\s*"
        r"(0x[0-9a-fA-F]+|\d+),\s*(0x[0-9a-fA-F]+|\d+),\s*(0x[0-9a-fA-F]+|\d+)\s*\)"
    )
    parsed = {}
    with open(path) as handle:
        for match in pattern.finditer(handle.read()):
            name = match.group(1)
            parsed[name] = tuple(int(match.group(i), 0) for i in (2, 3, 4))
    if parsed:
        THEME = parsed
    return THEME


# The knob sweep runs from 135 deg to 405 deg in egui screen space, which is
# -135 deg to +135 deg measured clockwise from 12 o'clock.
SWEEP_MIN_DEG = -135.0
SWEEP_MAX_DEG = 135.0

EXPECTED_WIDTH = 2732  # a wrong window id silently yields a different width

# Tokens that name a surface, a fill or a frame. Contrast floors apply to
# foreground colours only, so these are listed but never flagged.
SURFACE_TOKENS = {
    "CHASSIS_BG",
    "PANEL_BG",
    "PANEL_BG_SOUPED",
    "WELL_BG",
    "SURFACE_ELEVATED",
    "SURFACE_HOVER",
    "BORDER_DEFAULT",
    "BORDER_SUBTLE",
    "BORDER_FOCUS",
    "ACCENT_SLATE",
}


# ---------------------------------------------------------------------------
# Basics
# ---------------------------------------------------------------------------


def load(path: str) -> np.ndarray:
    img = Image.open(path).convert("RGB")
    arr = np.asarray(img).astype(np.int16)
    return arr


def relative_luminance(rgb) -> float:
    def channel(u: float) -> float:
        u = u / 255.0
        return u / 12.92 if u <= 0.04045 else ((u + 0.055) / 1.055) ** 2.4

    r, g, b = rgb
    return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)


def contrast_ratio(fg, bg) -> float:
    l1, l2 = sorted((relative_luminance(fg), relative_luminance(bg)), reverse=True)
    return (l1 + 0.05) / (l2 + 0.05)


def angle_from_12(dx: float, dy: float) -> float:
    """Degrees clockwise from 12 o'clock, in (-180, 180]. Screen y grows down."""
    return (math.degrees(math.atan2(dx, -dy)) + 540.0) % 360.0 - 180.0


def runs_of_zero(values: np.ndarray, min_len: int):
    """Yield (start_index, length) for each run of zeros at least min_len long."""
    out = []
    start = None
    for i, v in enumerate(values):
        if v == 0:
            if start is None:
                start = i
        else:
            if start is not None and i - start >= min_len:
                out.append((start, i - start))
            start = None
    if start is not None and len(values) - start >= min_len:
        out.append((start, len(values) - start))
    return out


# ---------------------------------------------------------------------------
# Panels
# ---------------------------------------------------------------------------


def find_panels(arr: np.ndarray, top_skip: int = 75, coverage: float = 0.85):
    """Detect panel frames from full-width border rows and full-height border cols.

    Returns a list of dicts with the panel's bounding box and interior box.
    """
    lum = arr.max(axis=2)
    height, width = lum.shape
    border = (lum > 40) & (lum < 100)

    row_hits = border[top_skip:].mean(axis=1)
    border_rows = [y + top_skip for y, frac in enumerate(row_hits) if frac > coverage]

    # Collapse adjacent rows (frames are 2 physical px at 2x DPI).
    grouped = []
    for y in border_rows:
        if grouped and y - grouped[-1][-1] <= 2:
            grouped[-1].append(y)
        else:
            grouped.append([y])
    edges = [g[0] for g in grouped]

    # Group edges into panels. A gutter (the blank strip the layout puts between
    # panels) ends a panel. Panels that share a single border line with no gutter
    # are split by their background colour instead, which differs per panel.
    def bg_at(y0, y1):
        band = arr[max(0, y0) : max(1, y1), :]
        return np.median(band.reshape(-1, 3), axis=0)

    spans = []
    current = [edges[0]]
    for prev, nxt in zip(edges, edges[1:]):
        if nxt - prev <= 20:  # gutter: panel ends at prev, next starts at nxt
            spans.append(current + [prev] if current[-1] != prev else current)
            current = [nxt]
        else:
            current.append(nxt)
    spans.append(current)

    panels = []
    for group in spans:
        if len(group) < 2:
            continue
        # Split the group wherever the panel background colour changes.
        cut_points = [0]
        for idx in range(1, len(group) - 1):
            e = group[idx]
            above = bg_at(e - 14, e - 4)
            below = bg_at(e + 4, e + 14)
            if np.abs(above - below).max() > 1.5:
                cut_points.append(idx)
        cut_points.append(len(group) - 1)

        for a, b in zip(cut_points, cut_points[1:]):
            top, bottom = group[a], group[b]
            h_dividers = group[a + 1 : b]
            if bottom - top < 40:
                continue
            band = border[top:bottom]
            col_hits = band.mean(axis=0)
            cols = [x for x, frac in enumerate(col_hits) if frac > coverage]
            if len(cols) < 2:
                continue
            panels.append(
                {
                    "top": top,
                    "bottom": bottom,
                    "left": cols[0],
                    "right": cols[-1],
                    "height": bottom - top,
                    "interior": [cols[0] + 8, top + 8, cols[-1] - 8, bottom - 8],
                    "inner_dividers": [x for x in cols[1:-1] if cols[0] + 4 < x < cols[-1] - 4],
                    "horizontal_dividers": h_dividers,
                }
            )
    return panels


def cmd_panels(args):
    arr = load(args.image)
    panels = find_panels(arr)
    if args.json:
        print(json.dumps(panels, indent=2))
        return
    print(f"{len(panels)} panels in {os.path.basename(args.image)}")
    prev_bottom = None
    for p in panels:
        gap = "" if prev_bottom is None else f"  gap_above={p['top'] - prev_bottom}"
        div = f"  v-dividers={p['inner_dividers']}" if p["inner_dividers"] else ""
        hdiv = f"  h-dividers={p['horizontal_dividers']}" if p["horizontal_dividers"] else ""
        print(
            f"  y {p['top']:5d}-{p['bottom']:5d}  x {p['left']:5d}-{p['right']:5d}"
            f"  h={p['height']:4d}{gap}{div}{hdiv}"
        )
        prev_bottom = p["bottom"]
    gaps = []
    prev = None
    for p in panels:
        if prev is not None:
            gaps.append(p["top"] - prev)
        prev = p["bottom"]
    if gaps and len(set(gaps)) > 1:
        print(f"  INCONSISTENT inter-panel gaps: {gaps}")


# ---------------------------------------------------------------------------
# Dead space
# ---------------------------------------------------------------------------


def content_mask(arr: np.ndarray, box) -> np.ndarray:
    x0, y0, x1, y1 = box
    sub = arr[y0:y1, x0:x1]
    bg = np.median(sub.reshape(-1, 3), axis=0)
    return np.abs(sub - bg).sum(axis=2) > 26


def analyze_region(arr, box, min_gap, min_rows):
    """Dead-space report for one rectangle: banded column gaps and empty rows."""
    mask = content_mask(arr, box)
    row = mask.sum(axis=1)

    # Split into content bands (a row of controls, a label row). A dead strip
    # only reads as dead relative to the band it sits in: a gap spanning the
    # whole panel is usually just the space under a short row.
    bands = []
    start = None
    for i, v in enumerate(row):
        if v > 0 and start is None:
            start = i
        elif v == 0 and start is not None:
            if i - start >= 8:
                bands.append((start, i - 1))
            start = None
    if start is not None:
        bands.append((start, len(row) - 1))
    merged = []
    for b in bands:
        if merged and b[0] - merged[-1][1] <= 12:
            merged[-1] = (merged[-1][0], b[1])
        else:
            merged.append(b)

    band_reports = []
    for by0, by1 in merged:
        col = mask[by0 : by1 + 1].sum(axis=0)
        gaps = [
            {"x": a + box[0], "width": n, "pct": round(100 * n / len(col), 1)}
            for a, n in runs_of_zero(col, min_gap)
        ]
        if gaps:
            band_reports.append(
                {
                    "y0": by0 + box[1],
                    "y1": by1 + box[1],
                    "title_row": (by1 - by0) <= 24,
                    "gaps": gaps,
                }
            )

    empty_rows = [
        {"y": a + box[1], "height": n, "pct": round(100 * n / len(row), 1)}
        for a, n in runs_of_zero(row, min_rows)
    ]
    return {
        "box": list(box),
        "fill_pct": round(100 * float(mask.mean()), 1),
        "bands": band_reports,
        "empty_rows": empty_rows,
    }


def cmd_deadspace(args):
    arr = load(args.image)
    panels = find_panels(arr)
    results = []
    for p in panels:
        x0, y0, x1, y1 = p["interior"]
        regions = [("panel", (x0, y0, x1, y1))]
        # Vertical dividers split a panel into columns that are laid out
        # independently, so a column can be half empty while the panel is not.
        bounds = [x0] + sorted(p["inner_dividers"]) + [x1]
        if len(bounds) > 2:
            for a, b in zip(bounds, bounds[1:]):
                if b - a > 120:
                    regions.append((f"column x{a}-{b}", (a + 6, y0, b - 6, y1)))
        entry = {"panel": {"top": p["top"], "bottom": p["bottom"]}, "regions": []}
        for label, box in regions:
            r = analyze_region(arr, box, args.min, args.min_rows)
            r["label"] = label
            entry["regions"].append(r)
        results.append(entry)

    if args.json:
        print(json.dumps(results, indent=2))
        return
    for entry in results:
        pt = entry["panel"]
        head = entry["regions"][0]
        print(f"panel y {pt['top']}-{pt['bottom']}  fill={head['fill_pct']}%")
        for r in entry["regions"]:
            prefix = "    " if r["label"] == "panel" else f"    [{r['label']}] "
            for band in r["bands"]:
                note = "  (title row, expected)" if band["title_row"] else ""
                for c in band["gaps"]:
                    print(
                        f"{prefix}DEAD COLUMNS x {c['x']}..{c['x'] + c['width']}  {c['width']}px"
                        f" ({c['pct']}% of width)  in band y {band['y0']}-{band['y1']}{note}"
                    )
            for c in r["empty_rows"]:
                print(
                    f"{prefix}DEAD ROWS    y {c['y']}..{c['y'] + c['height']}  {c['height']}px"
                    f" ({c['pct']}% of height)"
                )


# ---------------------------------------------------------------------------
# Knobs
# ---------------------------------------------------------------------------


def detect_knob_centers(arr: np.ndarray, radii, threshold: float = 0.55, samples: int = 72):
    """Ring-template Hough over the border-coloured mask.

    Each knob is painted as circle_stroke(radius, BORDER_DEFAULT), so a ring of
    that radius scores near 1.0 at the true centre. Sampling the ring at a fixed
    number of angles keeps this to a few array adds per radius.

    The border colour band also covers button fills, so a ring drawn anywhere
    inside a large button would score 1.0 too. The second accumulator, at
    radius + 5, rejects those: outside a real knob stroke is panel background,
    while outside a ring inside a button fill is more button fill.
    """
    lum = arr.max(axis=2)
    sat = arr.max(axis=2) - arr.min(axis=2)
    mask = ((lum > 38) & (lum < 105) & (sat < 40)).astype(np.float32)
    height, width = mask.shape

    def ring_accumulator(radius):
        acc = np.zeros_like(mask)
        for k in range(samples):
            a = 2.0 * math.pi * k / samples
            dx = int(round(radius * math.sin(a)))
            dy = int(round(-radius * math.cos(a)))
            ys0, ys1 = max(0, dy), min(height, height + dy)
            xs0, xs1 = max(0, dx), min(width, width + dx)
            acc[ys0 - dy : ys1 - dy, xs0 - dx : xs1 - dx] += mask[ys0:ys1, xs0:xs1]
        return acc / samples

    found = []
    for radius in radii:
        on_ring = ring_accumulator(radius)
        outside = ring_accumulator(radius + 5)
        # The stroke score is what separates a knob from any other round-ish
        # shape: on the ring, border colour; five pixels out, panel background.
        # Real knobs score 0.65-1.0 here; button corners and pill gaps score
        # below 0.45.
        stroke = on_ring - outside
        cand = np.argwhere(stroke >= threshold)
        for y, x in cand:
            found.append((float(stroke[y, x]), int(x), int(y), int(radius)))

    # Non-maximum suppression. Prefer the LARGEST radius in a neighbourhood: the
    # inactive arc track is itself a ring at 0.75 * radius and scores well, so a
    # score-first sort would report every knob at three quarters of its size and
    # then look for the arc in the wrong band.
    found.sort(key=lambda f: (-f[3], -f[0]))
    kept = []
    for score, x, y, radius in found:
        if any(abs(x - kx) < radius and abs(y - ky) < radius for _, kx, ky, _ in kept):
            continue
        kept.append((score, x, y, radius))
    kept.sort(key=lambda k: (k[2] // 40, k[1]))
    return kept


def measure_knob(arr: np.ndarray, cx: int, cy: int, radius: int):
    reach = radius + 8
    sub = arr[cy - reach : cy + reach + 1, cx - reach : cx + reach + 1]
    if sub.shape[0] != 2 * reach + 1 or sub.shape[1] != 2 * reach + 1:
        return None
    yy, xx = np.mgrid[-reach : reach + 1, -reach : reach + 1]
    dist = np.hypot(xx, yy)
    r, g, b = sub[..., 0], sub[..., 1], sub[..., 2]

    arc_r = radius * 0.75
    orange = (r > 150) & ((r - b) > 90) & (g < 190)
    white = (r > 170) & (g > 170) & (b > 170)
    arc_mask = orange & (dist >= arc_r - 6) & (dist <= arc_r + 6)
    needle_mask = white & (dist >= 5) & (dist <= radius - 6)

    def extent(mask):
        if mask.sum() < 3:
            return None
        angles = np.array([angle_from_12(x, y) for x, y in zip(xx[mask], yy[mask])])
        # Rotate into the sweep's own frame so the 180 deg wrap is outside it.
        shifted = np.sort((angles + 180.0) % 360.0) - 180.0
        return float(shifted[0]), float(shifted[-1]), int(mask.sum())

    arc = extent(arc_mask)
    needle = extent(needle_mask)
    out = {"center": [cx, cy], "radius": radius}
    if needle is not None:
        out["needle_deg"] = round((needle[0] + needle[1]) / 2.0, 1)
        out["needle_fraction"] = round(
            (out["needle_deg"] - SWEEP_MIN_DEG) / (SWEEP_MAX_DEG - SWEEP_MIN_DEG), 3
        )
    if arc is not None:
        out["arc_start_deg"] = round(arc[0], 1)
        out["arc_end_deg"] = round(arc[1], 1)
        out["arc_px"] = arc[2]
        if "needle_deg" in out:
            # A unipolar arc runs min -> needle; a bipolar arc runs centre -> needle.
            # Either way the arc end nearest the needle must coincide with it.
            delta = min(
                abs(out["arc_end_deg"] - out["needle_deg"]),
                abs(out["arc_start_deg"] - out["needle_deg"]),
            )
            out["arc_needle_delta_deg"] = round(delta, 1)
            out["fills_from"] = (
                "min"
                if abs(out["arc_start_deg"] - SWEEP_MIN_DEG) < 6
                else "centre"
                if abs(out["arc_start_deg"]) < 6
                else "other"
            )
    else:
        out["arc_px"] = 0
    return out


def cmd_knobs(args):
    arr = load(args.image)
    radii = parse_radii(args.radii)
    centers = detect_knob_centers(arr, radii, threshold=args.threshold)
    knobs = []
    for score, x, y, radius in centers:
        m = measure_knob(arr, x, y, radius)
        if m is None:
            continue
        m["ring_score"] = round(score, 3)
        knobs.append(m)
    if args.json:
        print(json.dumps(knobs, indent=2))
        return
    print(f"{len(knobs)} knobs in {os.path.basename(args.image)}  (sweep {SWEEP_MIN_DEG}..{SWEEP_MAX_DEG} deg)")
    no_arc = 0
    for k in knobs:
        needle = f"{k['needle_deg']:7.1f}" if "needle_deg" in k else "   none"
        frac = k.get("needle_fraction", "-")
        if k["arc_px"] == 0:
            no_arc += 1
            print(
                f"  ({k['center'][0]:5d},{k['center'][1]:5d}) r={k['radius']:2d}"
                f"  needle={needle} ({frac})  arc=NONE"
            )
        else:
            flag = "  <-- ARC/NEEDLE MISMATCH" if k.get("arc_needle_delta_deg", 0) > 4 else ""
            print(
                f"  ({k['center'][0]:5d},{k['center'][1]:5d}) r={k['radius']:2d}"
                f"  needle={needle} ({frac})"
                f"  arc=[{k['arc_start_deg']:7},{k['arc_end_deg']:7}] from={k['fills_from']}"
                f"  delta={k.get('arc_needle_delta_deg')}{flag}"
            )
    if no_arc:
        print(
            f"  {no_arc}/{len(knobs)} knobs draw no value arc. Expected only for knobs at a"
            f" true zero-length fill; investigate if a unipolar knob sits away from its minimum."
        )


def parse_radii(spec: str):
    if "-" in spec:
        rng, _, step = spec.partition(":")
        lo, _, hi = rng.partition("-")
        return list(range(int(lo), int(hi) + 1, int(step or 2)))
    return [int(part) for part in spec.split(",")]


# ---------------------------------------------------------------------------
# Text: glyph heights, colours, contrast
# ---------------------------------------------------------------------------


def nearest_theme(rgb, tolerance: int = 26):
    """Snap a sampled colour to a theme token so antialiasing noise collapses."""
    best, best_d = None, 1e9
    for name, value in THEME.items():
        d = sum((a - b) ** 2 for a, b in zip(rgb, value)) ** 0.5
        if d < best_d:
            best, best_d = name, d
    return best if best_d <= tolerance else None


def find_text_runs(arr: np.ndarray, box, min_pixels: int = 12):
    """Group ink pixels into text runs inside a box.

    Ink is anything that differs from the run's own background, so this finds
    dark-on-accent text (a label inside an orange button) as readily as
    light-on-dark text, and reports the contrast of the pair that actually
    matters.
    """
    x0, y0, x1, y1 = box
    sub = arr[y0:y1, x0:x1]
    if sub.size == 0:
        return []
    page_bg = np.median(sub.reshape(-1, 3), axis=0)
    ink = np.abs(sub - page_bg).sum(axis=2) > 40
    if ink.sum() < min_pixels:
        return []

    rows = ink.any(axis=1)
    bands = []
    start = None
    for i, hit in enumerate(rows):
        if hit and start is None:
            start = i
        elif not hit and start is not None:
            bands.append((start, i - 1))
            start = None
    if start is not None:
        bands.append((start, len(rows) - 1))

    runs = []
    for by0, by1 in bands:
        if not (4 <= by1 - by0 <= 70):
            # Shorter than a glyph, or a whole stacked block rather than one run.
            continue
        band = ink[by0 : by1 + 1]
        cols = band.any(axis=0)
        segments = []
        cstart = None
        for i, hit in enumerate(cols):
            if hit and cstart is None:
                cstart = i
            elif not hit and cstart is not None:
                segments.append((cstart, i - 1))
                cstart = None
        if cstart is not None:
            segments.append((cstart, len(cols) - 1))
        merged = []
        for seg in segments:
            if merged and seg[0] - merged[-1][1] <= 14:
                merged[-1] = (merged[-1][0], seg[1])
            else:
                merged.append(seg)

        for sx0, sx1 in merged:
            if sx1 - sx0 < 4:
                continue
            patch = sub[by0 : by1 + 1, sx0 : sx1 + 1].reshape(-1, 3)
            if patch.shape[0] < min_pixels:
                continue
            # Background is the modal colour of the patch; foreground is the mean
            # of the pixels furthest from it in RGB space.
            quant = (patch // 8 * 8).astype(np.int32)
            keys, counts = np.unique(quant, axis=0, return_counts=True)
            bg_key = keys[counts.argmax()]
            bg_pixels = patch[(np.abs(quant - bg_key).sum(axis=1) == 0)]
            bg = tuple(int(v) for v in bg_pixels.mean(axis=0).round())
            dist = np.abs(patch - np.array(bg)).sum(axis=1)
            top = max(1, int(len(patch) * 0.06))
            fg = tuple(int(v) for v in patch[np.argsort(dist)[-top:]].mean(axis=0).round())
            if sum(abs(a - b) for a, b in zip(fg, bg)) < 60:
                continue  # a fill, not text
            # Measure the glyphs themselves, not the ink band: the band includes
            # the control's own border, which would inflate every height.
            region = sub[by0 : by1 + 1, sx0 : sx1 + 1]
            near_fg = np.abs(region - np.array(fg)).sum(axis=2) < 90
            if near_fg.sum() < min_pixels:
                continue
            gy = np.where(near_fg.any(axis=1))[0]
            gx = np.where(near_fg.any(axis=0))[0]
            runs.append(
                {
                    "box": [
                        sx0 + x0 + int(gx.min()),
                        by0 + y0 + int(gy.min()),
                        sx0 + x0 + int(gx.max()),
                        by0 + y0 + int(gy.max()),
                    ],
                    "height": int(gy.max() - gy.min() + 1),
                    "width": int(gx.max() - gx.min() + 1),
                    "fg": fg,
                    "fg_token": nearest_theme(fg),
                    "bg": bg,
                    "bg_token": nearest_theme(bg),
                    "contrast": round(contrast_ratio(fg, bg), 2),
                }
            )
    return runs


def cmd_text(args):
    """Glyph metrics for one probe box. Use for comparing font sizes between
    control classes: put a box around one label, one button caption, one combo,
    and compare the reported heights."""
    arr = load(args.image)
    box = tuple(int(v) for v in args.box.split(","))
    runs = find_text_runs(arr, box)
    if args.json:
        print(json.dumps(runs, indent=2))
        return
    if not runs:
        print("no text runs found in that box")
        return
    for r in runs:
        fg_hex = "#%02X%02X%02X" % r["fg"]
        bg_hex = "#%02X%02X%02X" % r["bg"]
        token = f" ({r['fg_token']})" if r["fg_token"] else ""
        print(
            f"  box {r['box']}  h={r['height']:3d} w={r['width']:4d}"
            f"  fg={fg_hex}{token}  bg={bg_hex}  contrast={r['contrast']}"
        )


def cmd_colors(args):
    """Colour inventory per panel.

    This is what catches a palette that has quietly grown three different muted
    greys, or a disabled state that fell below the contrast floor: every colour
    the renderer actually emitted, snapped to a theme token where one matches.
    """
    arr = load(args.image)
    panels = find_panels(arr)
    report = []
    for p in panels:
        x0, y0, x1, y1 = p["interior"]
        sub = arr[y0:y1, x0:x1].reshape(-1, 3)
        keys, counts = np.unique(sub, axis=0, return_counts=True)
        bg = tuple(int(v) for v in keys[counts.argmax()])
        entries = []
        for key, count in zip(keys, counts):
            if count < args.min_pixels:
                continue
            rgb = tuple(int(v) for v in key)
            if rgb == bg:
                continue
            entries.append(
                {
                    "rgb": rgb,
                    "hex": "#%02X%02X%02X" % rgb,
                    "count": int(count),
                    "token": nearest_theme(rgb),
                    "contrast_vs_bg": round(contrast_ratio(rgb, bg), 2),
                }
            )
        for e in entries:
            lighter = relative_luminance(e["rgb"]) > relative_luminance(bg)
            e["foreground"] = lighter and e["token"] not in SURFACE_TOKENS and e["contrast_vs_bg"] >= 1.3
        entries.sort(key=lambda e: -e["count"])
        report.append(
            {"panel": {"top": p["top"], "bottom": p["bottom"]}, "bg": bg, "colors": entries}
        )
    if args.json:
        print(json.dumps(report, indent=2))
        return
    for r in report:
        pt = r["panel"]
        print(f"panel y {pt['top']}-{pt['bottom']}  bg=#%02X%02X%02X" % r["bg"])
        foreground = [e for e in r["colors"] if e["foreground"]]
        surfaces = [e for e in r["colors"] if not e["foreground"]]
        if not foreground:
            print("    no foreground colours above the pixel floor")
        for e in foreground[: args.top]:
            token = f"  {e['token']}" if e["token"] else "  NOT A THEME TOKEN"
            flag = ""
            if e["contrast_vs_bg"] < 3.0:
                flag = "  <-- BELOW 3:1"
            elif e["contrast_vs_bg"] < 4.5:
                flag = "  <-- below 4.5:1"
            print(
                f"    {e['hex']}  n={e['count']:7d}  contrast={e['contrast_vs_bg']:6.2f}{token}{flag}"
            )
        if surfaces and args.show_surfaces:
            names = ", ".join(
                f"{e['hex']}{'/' + e['token'] if e['token'] else ''}" for e in surfaces[:8]
            )
            print(f"    surfaces: {names}")


# ---------------------------------------------------------------------------
# Accent presence: catches a selection indicator lost in a disabled state
# ---------------------------------------------------------------------------


def accent_counts(arr: np.ndarray, panels):
    out = []
    for p in panels:
        x0, y0, x1, y1 = p["interior"]
        sub = arr[y0:y1, x0:x1]
        r, g, b = sub[..., 0], sub[..., 1], sub[..., 2]
        mask = (r > 150) & ((r - b) > 90)
        out.append({"top": p["top"], "bottom": p["bottom"], "accent_px": int(mask.sum())})
    return out


def cmd_accent(args):
    per_image = {}
    box = tuple(int(v) for v in args.box.split(",")) if args.box else None
    for path in args.images:
        arr = load(path)
        if box:
            # One control group rather than whole panels: use when a panel's
            # count drops without reaching zero, which means one group inside
            # it lost its indicator.
            regions = [{"top": box[1], "bottom": box[3], "interior": list(box)}]
        else:
            regions = find_panels(arr)
        per_image[os.path.basename(path)] = accent_counts(arr, regions)
    if args.json:
        print(json.dumps(per_image, indent=2))
        return
    names = list(per_image)
    rows = max(len(v) for v in per_image.values())
    print("accent pixels per panel (a panel that has accent in one state and 0 in another")
    print("has probably lost its selection indicator rather than its selection)")
    header = "  panel".ljust(22) + "".join(n.ljust(18) for n in names)
    print(header)
    for i in range(rows):
        cells = []
        label = ""
        for n in names:
            entries = per_image[n]
            if i < len(entries):
                label = f"y {entries[i]['top']}-{entries[i]['bottom']}"
                cells.append(str(entries[i]["accent_px"]))
            else:
                cells.append("-")
        flag = ""
        numeric = [int(c) for c in cells if c.isdigit()]
        if numeric and min(numeric) == 0 and max(numeric) > 0:
            flag = "  <-- ACCENT LOST IN ONE STATE"
        print("  " + label.ljust(20) + "".join(c.ljust(18) for c in cells) + flag)


# ---------------------------------------------------------------------------
# Edges: widget alignment within a band
# ---------------------------------------------------------------------------


def cmd_edges(args):
    """Report the content extents of each row inside a box.

    Rows of controls that should share a right edge (a stacked button column,
    a form) betray a layout bug by ending a few pixels apart. Point this at the
    column and read the right column of numbers.
    """
    arr = load(args.image)
    box = tuple(int(v) for v in args.box.split(","))
    x0, y0, x1, y1 = box
    mask = content_mask(arr, box)
    row = mask.sum(axis=1)

    bands = []
    start = None
    for i, v in enumerate(row):
        if v > 0 and start is None:
            start = i
        elif v == 0 and start is not None:
            if i - start >= args.min_band:
                bands.append((start, i - 1))
            start = None
    if start is not None and len(row) - start >= args.min_band:
        bands.append((start, len(row) - 1))

    report = []
    for by0, by1 in bands:
        cols = np.where(mask[by0 : by1 + 1].any(axis=0))[0]
        if not len(cols):
            continue
        report.append(
            {
                "y0": by0 + y0,
                "y1": by1 + y0,
                "left": int(cols.min()) + x0,
                "right": int(cols.max()) + x0,
            }
        )
    if args.json:
        print(json.dumps(report, indent=2))
        return

    print(f"content extents per row band in x {x0}-{x1}, y {y0}-{y1}:")
    for r in report:
        print(f"    y {r['y0']:5d}-{r['y1']:5d}   left={r['left']:5d}   right={r['right']:5d}")
    if len(report) > 1:
        rights = [r["right"] for r in report]
        lefts = [r["left"] for r in report]
        target_r, target_l = max(rights), min(lefts)
        for r in report:
            notes = []
            if 2 < target_r - r["right"] <= args.tolerance:
                notes.append(f"right edge {target_r - r['right']}px short of {target_r}")
            if 2 < r["left"] - target_l <= args.tolerance:
                notes.append(f"left edge {r['left'] - target_l}px inside {target_l}")
            if notes:
                print(f"    MISALIGNED  y {r['y0']}-{r['y1']}: " + "; ".join(notes))


def cmd_strokes(args):
    """Measure the segments of a single-colour indicator inside a box.

    An outline, underline, LED bar or playhead frame should be one weight. This
    reports each horizontal and vertical segment of the chosen colour with its
    thickness, so a 10 px bar merging into a 2 px outline is visible as a
    number rather than as a vague impression of heaviness.
    """
    arr = load(args.image)
    x0, y0, x1, y1 = (int(v) for v in args.box.split(","))
    sub = arr[y0:y1, x0:x1]

    if args.color.startswith("#"):
        target = tuple(int(args.color[i : i + 2], 16) for i in (1, 3, 5))
    else:
        if args.color not in THEME:
            print(f"unknown token {args.color}; known: {', '.join(sorted(THEME))}", file=sys.stderr)
            return 1
        target = THEME[args.color]
    mask = np.abs(sub - np.array(target)).sum(axis=2) <= args.tolerance

    height, width = mask.shape
    row_cov = mask.sum(axis=1) / max(1, width)
    col_cov = mask.sum(axis=0) / max(1, height)

    def segments(coverage, offset):
        out = []
        start = None
        for i, v in enumerate(coverage):
            if v >= args.coverage and start is None:
                start = i
            elif v < args.coverage and start is not None:
                out.append({"start": start + offset, "thickness": i - start})
                start = None
        if start is not None:
            out.append({"start": start + offset, "thickness": len(coverage) - start})
        return out

    rows = segments(row_cov, y0)
    cols = segments(col_cov, x0)
    if args.json:
        print(json.dumps({"horizontal": rows, "vertical": cols}, indent=2))
        return

    print(f"{args.color} segments in x {x0}-{x1}, y {y0}-{y1} (coverage >= {args.coverage}):")
    for r in rows:
        print(f"    horizontal  y {r['start']:5d}  thickness={r['thickness']}px")
    for c in cols:
        print(f"    vertical    x {c['start']:5d}  thickness={c['thickness']}px")
    weights = sorted({s["thickness"] for s in rows + cols})
    if len(weights) > 1:
        print(
            f"    NON-UNIFORM WEIGHT: thicknesses {weights}. One indicator should be one weight;"
            f" separate marks that touch read as a single uneven edge."
        )
    elif weights:
        print(f"    uniform weight: {weights[0]}px")


# ---------------------------------------------------------------------------
# Diff: what actually changed between two states
# ---------------------------------------------------------------------------


def cmd_diff(args):
    a = load(args.image_a)
    b = load(args.image_b)
    if a.shape != b.shape:
        print(f"shape mismatch: {a.shape} vs {b.shape}", file=sys.stderr)
        return 1
    d = np.abs(a - b).sum(axis=2) > args.threshold
    total = int(d.sum())
    ys = np.where(d.any(axis=1))[0]
    clusters = []
    if len(ys):
        start = prev = ys[0]
        for y in ys[1:]:
            if y - prev > 6:
                clusters.append((start, prev))
                start = y
            prev = y
        clusters.append((start, prev))
    regions = []
    for y0, y1 in clusters:
        band = d[y0 : y1 + 1]
        xs = np.where(band.any(axis=0))[0]
        regions.append(
            {"y0": int(y0), "y1": int(y1), "x0": int(xs.min()), "x1": int(xs.max()), "px": int(band.sum())}
        )
    if args.json:
        print(json.dumps({"total_px": total, "regions": regions}, indent=2))
        return
    print(f"{os.path.basename(args.image_a)} vs {os.path.basename(args.image_b)}: {total} px differ")
    for r in regions:
        print(f"    y {r['y0']:5d}-{r['y1']:5d}  x {r['x0']:5d}-{r['x1']:5d}  n={r['px']}")
    if not regions:
        print("    IDENTICAL. The two captures do not show different states.")


# ---------------------------------------------------------------------------
# Crop: hand a magnified region to the vision pass
# ---------------------------------------------------------------------------


def cmd_crop(args):
    img = Image.open(args.image).convert("RGB")
    x0, y0, x1, y1 = (int(v) for v in args.box.split(","))
    crop = img.crop((x0, y0, x1, y1))
    if args.scale != 1:
        resample = Image.NEAREST if args.scale >= 3 else Image.LANCZOS
        crop = crop.resize((crop.width * args.scale, crop.height * args.scale), resample)
    crop.save(args.out)
    print(f"wrote {args.out}  {crop.width}x{crop.height}")


# ---------------------------------------------------------------------------
# Report: everything, over a capture directory
# ---------------------------------------------------------------------------


def cmd_report(args):
    directory = args.directory
    images = sorted(
        os.path.join(directory, f)
        for f in os.listdir(directory)
        if f.endswith(".png") and "_" not in f
    )
    if not images:
        print(f"no top-level capture PNGs in {directory}", file=sys.stderr)
        return 1

    print("=" * 78)
    print("CAPTURE SET")
    print("=" * 78)
    for path in images:
        with Image.open(path) as im:
            w, h = im.size
        flag = "" if w == EXPECTED_WIDTH else f"  <-- WIDTH {w}, EXPECTED {EXPECTED_WIDTH}"
        print(f"  {os.path.basename(path):20s} {w}x{h}{flag}")

    base = next((i for i in images if os.path.basename(i) == args.base), images[0])
    for path in images[1:]:
        print()
        print("=" * 78)
        print(f"DIFF  {os.path.basename(base)} vs {os.path.basename(path)}")
        print("=" * 78)
        cmd_diff(argparse.Namespace(image_a=base, image_b=path, threshold=18, json=False))

    for path in images:
        print()
        print("=" * 78)
        print(f"GEOMETRY  {os.path.basename(path)}")
        print("=" * 78)
        cmd_panels(argparse.Namespace(image=path, json=False))
        print()
        cmd_deadspace(argparse.Namespace(image=path, min=600, min_rows=120, json=False))
        print()
        cmd_colors(
            argparse.Namespace(
                image=path, min_pixels=150, top=12, show_surfaces=False, json=False
            )
        )

    print()
    print("=" * 78)
    print("KNOBS  (first capture only; re-run per state if knob values differ)")
    print("=" * 78)
    cmd_knobs(
        argparse.Namespace(image=base, radii=args.radii, threshold=0.55, json=False)
    )

    print()
    print("=" * 78)
    print("ACCENT / SELECTION PRESENCE")
    print("=" * 78)
    cmd_accent(argparse.Namespace(images=images, box=None, json=False))
    return 0


# ---------------------------------------------------------------------------


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)

    p = sub.add_parser("report", help="run every check over a capture directory")
    p.add_argument("directory")
    p.add_argument("--base", default="latest.png", help="capture every other state is diffed against")
    p.add_argument("--radii", default="20-44:2")
    p.set_defaults(func=cmd_report)

    p = sub.add_parser("panels", help="detect panel frames and inter-panel gaps")
    p.add_argument("image")
    p.set_defaults(func=cmd_panels)

    p = sub.add_parser("deadspace", help="find empty runs inside each panel")
    p.add_argument("image")
    p.add_argument("--min", type=int, default=600, help="minimum empty column run, px")
    p.add_argument("--min-rows", dest="min_rows", type=int, default=120)
    p.set_defaults(func=cmd_deadspace)

    p = sub.add_parser("knobs", help="knob centres, arc extent, needle angle")
    p.add_argument("image")
    p.add_argument("--radii", default="20-44:2", help="lo-hi:step or comma list")
    p.add_argument("--threshold", type=float, default=0.55, help="minimum ring stroke score")
    p.set_defaults(func=cmd_knobs)

    p = sub.add_parser("text", help="glyph height and contrast for one probe box")
    p.add_argument("image")
    p.add_argument("--box", required=True, help="x0,y0,x1,y1")
    p.set_defaults(func=cmd_text)

    p = sub.add_parser("colors", help="colour inventory per panel, snapped to theme tokens")
    p.add_argument("image")
    p.add_argument("--min-pixels", dest="min_pixels", type=int, default=150)
    p.add_argument("--top", type=int, default=14, help="foreground colours listed per panel")
    p.add_argument("--show-surfaces", dest="show_surfaces", action="store_true")
    p.set_defaults(func=cmd_colors)

    p = sub.add_parser("accent", help="accent pixels per panel (or one box) across states")
    p.add_argument("images", nargs="+")
    p.add_argument("--box", help="x0,y0,x1,y1 to measure one control group instead of every panel")
    p.set_defaults(func=cmd_accent)

    p = sub.add_parser("edges", help="widget edge coordinates inside a band")
    p.add_argument("image")
    p.add_argument("--box", required=True, help="x0,y0,x1,y1")
    p.add_argument("--min-band", dest="min_band", type=int, default=10)
    p.add_argument("--tolerance", type=int, default=60,
                   help="largest gap still treated as a misalignment rather than a short row")
    p.set_defaults(func=cmd_edges)

    p = sub.add_parser("strokes", help="segment thicknesses of a single-colour indicator")
    p.add_argument("image")
    p.add_argument("--box", required=True, help="x0,y0,x1,y1")
    p.add_argument("--color", default="TEXT_PRIMARY", help="theme token name or #RRGGBB")
    p.add_argument("--tolerance", type=int, default=45)
    p.add_argument("--coverage", type=float, default=0.85,
                   help="fraction of the box a row/column must cover to count as a segment")
    p.set_defaults(func=cmd_strokes)

    p = sub.add_parser("diff", help="changed-region clusters between two states")
    p.add_argument("image_a")
    p.add_argument("image_b")
    p.add_argument("--threshold", type=int, default=18)
    p.set_defaults(func=cmd_diff)

    p = sub.add_parser("crop", help="write a magnified crop for the vision pass")
    p.add_argument("image")
    p.add_argument("--box", required=True, help="x0,y0,x1,y1")
    p.add_argument("--scale", type=int, default=2)
    p.add_argument("-o", "--out", required=True)
    p.set_defaults(func=cmd_crop)

    for name in ("report", "panels", "deadspace", "knobs", "text", "colors", "accent", "edges", "strokes", "diff"):
        sub.choices[name].add_argument("--json", action="store_true", help="machine-readable output")

    parser.add_argument(
        "--theme", default=DEFAULT_THEME_RS, help="path to theme.rs for token names"
    )
    args = parser.parse_args(argv)
    load_theme_tokens(args.theme)
    return args.func(args) or 0


if __name__ == "__main__":
    sys.exit(main())
