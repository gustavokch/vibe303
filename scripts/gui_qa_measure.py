#!/usr/bin/env python3
"""Numeric measurement toolkit for the TECHNEO 303 GUI visual audit.

Reads capture PNGs and reports what vision alone cannot measure reliably:
panel geometry, dead space, knob arc-vs-needle agreement, text contrast and
glyph sizes, accent/selection presence, widget edge alignment, and per-state
pixel diffs.

Dependencies: numpy and Pillow only.

Usage (D=target/gui-qa-offscreen):
    scripts/gui_qa_measure.py report    $D
    scripts/gui_qa_measure.py knobs     $D/knobs.png
    scripts/gui_qa_measure.py colors    $D/accurate.png
    scripts/gui_qa_measure.py text      $D/latest.png --box 235,203,335,227
    scripts/gui_qa_measure.py accent    $D/latest.png $D/accurate.png
    scripts/gui_qa_measure.py deadspace $D/latest.png --min 600
    scripts/gui_qa_measure.py diff      $D/latest.png $D/running.png
    scripts/gui_qa_measure.py edges     $D/latest.png --box 1628,320,1960,540
    scripts/gui_qa_measure.py strokes   $D/running.png --box 616,856,808,880 --color ACCENT_ORANGE
    scripts/gui_qa_measure.py crop      $D/latest.png --box 1620,62,2424,262 --scale 2 -o /tmp/env.png

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
    "CHASSIS_BG": (0x15, 0x15, 0x15),
    "WELL_BG": (0x1A, 0x1A, 0x1A),
    "PANEL_BG": (0x26, 0x26, 0x26),
    "PANEL_HEADER_BG": (0x2F, 0x2F, 0x2F),
    "SURFACE_ELEVATED": (0x3A, 0x3A, 0x3A),
    "SURFACE_HOVER": (0x49, 0x49, 0x49),
    "KNOB_TRACK": (0x40, 0x40, 0x40),
    "TEXT_PRIMARY": (0xEC, 0xEC, 0xEC),
    "TEXT_MUTED": (0x8F, 0x8F, 0x8F),
    "ACCENT_ORANGE": (0xF6, 0x66, 0x1E),
}

THEME = dict(THEME_FALLBACK)

# Knob size tiers (theme.rs KNOB_RADIUS_*): outer radius in points.
KNOB_RADII = {"LARGE": 21.0, "MEDIUM": 12.0}


def load_theme_tokens(path: str = DEFAULT_THEME_RS):
    """Parse the Color32 tokens and the KNOB_RADIUS_* tiers out of theme.rs."""
    global THEME, KNOB_RADII
    if not os.path.exists(path):
        return THEME
    with open(path) as handle:
        source = handle.read()
    pattern = re.compile(
        r"pub const (\w+): Color32 = Color32::from_rgb\(\s*"
        r"(0x[0-9a-fA-F]+|\d+),\s*(0x[0-9a-fA-F]+|\d+),\s*(0x[0-9a-fA-F]+|\d+)\s*\)"
    )
    parsed = {
        match.group(1): tuple(int(match.group(i), 0) for i in (2, 3, 4))
        for match in pattern.finditer(source)
    }
    if parsed:
        THEME = parsed
    radii = {
        match.group(1): float(match.group(2))
        for match in re.finditer(r"pub const KNOB_RADIUS_(\w+): f32 = ([0-9.]+);", source)
    }
    if radii:
        KNOB_RADII = radii
    return THEME


# Window width in points (gui/mod.rs GUI_WIDTH). Window captures and the
# tools/gui_render frames are both 2x, so a capture is 2 * GUI_WIDTH px wide.
GUI_WIDTH = 1216
CAPTURE_SCALE = 2


def load_gui_width(theme_path: str = DEFAULT_THEME_RS) -> int:
    """Read GUI_WIDTH from the gui/mod.rs beside theme.rs."""
    global GUI_WIDTH
    mod_rs = os.path.join(os.path.dirname(theme_path), "mod.rs")
    if os.path.exists(mod_rs):
        with open(mod_rs) as handle:
            match = re.search(r"pub const GUI_WIDTH: u32 = (\d+);", handle.read())
        if match:
            GUI_WIDTH = int(match.group(1))
    return GUI_WIDTH


def expected_width() -> int:
    """Capture width in px. A wrong window id silently yields a different one."""
    return GUI_WIDTH * CAPTURE_SCALE


# One capture run writes every state within seconds (offscreen) or a few
# minutes (window capture). A PNG much older than the newest one is left over
# from an earlier run, often of a state the renderer no longer draws, and its
# diffs, colours and accent counts describe old code.
STALE_AFTER_S = 600.0


def stale_captures(mtimes: dict, after_s: float = STALE_AFTER_S) -> dict:
    """Captures written more than `after_s` before the newest one: name -> age in s."""
    if not mtimes:
        return {}
    newest = max(mtimes.values())
    return {name: newest - t for name, t in mtimes.items() if newest - t > after_s}


def format_age(seconds: float) -> str:
    if seconds >= 86400:
        return f"{seconds / 86400:.1f} d"
    if seconds >= 3600:
        return f"{seconds / 3600:.1f} h"
    return f"{seconds / 60:.0f} min"


# The knob sweep runs from 135 deg to 405 deg in egui screen space, which is
# -135 deg to +135 deg measured clockwise from 12 o'clock.
SWEEP_MIN_DEG = -135.0
SWEEP_MAX_DEG = 135.0

# `colors` pixel floor for a 2 px/pt capture.
COLORS_MIN_PIXELS = 150


# Tokens that name a surface, a fill or a frame. Contrast floors apply to
# foreground colours only, so these are listed but never flagged.
SURFACE_TOKENS = {
    "CHASSIS_BG",
    "WELL_BG",
    "PANEL_BG",
    "PANEL_HEADER_BG",
    "SURFACE_ELEVATED",
    "SURFACE_HOVER",
    "SURFACE_PRESSED",
    "KNOB_TRACK",
    # Lit fills that carry dark TEXT_ON_ACCENT text.
    "ACCENT_ORANGE_HOVER",
    "ACCENT_BLUE",
    "GATE_NOTE_FILL",
    "GATE_TIE_FILL",
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


def _runs(mask: np.ndarray):
    """Half-open [start, end) runs of True in a 1-D bool array."""
    padded = np.concatenate(([False], mask, [False]))
    edges = np.flatnonzero(padded[1:] != padded[:-1])
    return list(zip(edges[::2].tolist(), edges[1::2].tolist(), strict=True))


def find_panels(arr: np.ndarray, min_size: int = 32):
    """Detect the flat device cards.

    The editor draws each device as a filled card on the CHASSIS_BG plane, with
    a chassis gutter between cards and no outline. A recursive XY cut on rows
    and columns that are pure chassis isolates every card. Window chrome above
    the first all-chassis row (the macOS title bar) is ignored. Bounds are
    half-open: arr[top:bottom, left:right] is the card. `interior` excludes the
    PANEL_HEADER_BG title strip.
    """
    chassis = np.array(THEME["CHASSIS_BG"], dtype=np.int16)
    header = np.array(THEME["PANEL_HEADER_BG"], dtype=np.int16)
    bg = np.abs(arr.astype(np.int16) - chassis).max(axis=2) <= 2
    full_rows = np.flatnonzero(bg.all(axis=1))
    if full_rows.size == 0:
        return []
    height, width = bg.shape
    panels = []

    def leaf(top, bottom, left, right):
        # Below min_size a region is a gutter or a control, not a card.
        if bottom - top < min_size or right - left < min_size:
            return
        hdr = top
        while hdr < bottom and np.abs(np.median(arr[hdr, left:right], axis=0) - header).max() <= 2:
            hdr += 1
        body_top = hdr + 8 if bottom - hdr > 16 else top + 8
        panels.append(
            {
                "top": top,
                "bottom": bottom,
                "left": left,
                "right": right,
                "height": bottom - top,
                "header_bottom": hdr,
                "interior": [left + 8, body_top, right - 8, bottom - 8],
                "inner_dividers": [],
                "horizontal_dividers": [],
            }
        )

    def cut(y0, y1, x0, x1):
        sub = ~bg[y0:y1, x0:x1]
        rows = _runs(sub.any(axis=1))
        cols = _runs(sub.any(axis=0))
        if len(rows) == 1 and len(cols) == 1:
            (ry0, ry1), (cx0, cx1) = rows[0], cols[0]
            if (ry0, ry1, cx0, cx1) == (0, y1 - y0, 0, x1 - x0):
                leaf(y0, y1, x0, x1)
            else:
                cut(y0 + ry0, y0 + ry1, x0 + cx0, x0 + cx1)
        elif len(rows) > 1:
            for ry0, ry1 in rows:
                cut(y0 + ry0, y0 + ry1, x0, x1)
        else:
            for cx0, cx1 in cols:
                cut(y0, y1, x0 + cx0, x0 + cx1)

    cut(int(full_rows[0]), height, 0, width)
    return panels


def cmd_panels(args):
    arr = load(args.image)
    panels = sorted(find_panels(arr), key=lambda p: (p["top"], p["left"]))
    if args.json:
        print(json.dumps(panels, indent=2))
        return
    print(f"{len(panels)} panels in {os.path.basename(args.image)}")
    # Cards in different columns share a top edge, so a gap only means
    # something against the previous card in the same column.
    prev_bottom = {}
    gaps = []
    for p in panels:
        above = prev_bottom.get(p["left"])
        gap = ""
        if above is not None:
            gaps.append(p["top"] - above)
            gap = f"  gap_above={p['top'] - above}"
        print(
            f"  y {p['top']:5d}-{p['bottom']:5d}  x {p['left']:5d}-{p['right']:5d}"
            f"  h={p['height']:4d}{gap}"
        )
        prev_bottom[p["left"]] = p["bottom"]
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
            # The app bar is one header strip with no body: brand left,
            # controls right, an empty middle by design.
            if p["bottom"] - p["header_bottom"] <= 16:
                for band in r["bands"]:
                    band["title_row"] = True
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


# Knob303 (widgets/knob.rs), for a knob of outer radius r points:
#   value ring: a stroke arc_w = clamp(0.14 r, 2, 3) wide centred on
#               r - arc_w / 2; KNOB_TRACK over the whole sweep, the value arc
#               on top (ACCENT_ORANGE, TEXT_MUTED when disabled);
#   body disc:  KNOB_BODY (= SURFACE_ELEVATED, SURFACE_HOVER when hovered),
#               radius 0.68 r, ringed by a gap of panel background;
#   pointer:    TEXT_PRIMARY (TEXT_MUTED when disabled), from 0.35 of the body
#               radius out to its edge.
BODY_FRACTION = 0.68
POINTER_INNER = 0.35
# A fill origin within this distance of -135 deg or 0 deg, measured along the
# ring, counts as min or centre. Antialiasing spreads an arc end by up to 2 px.
ORIGIN_TOL_PX = 2.5
# Arc tip and pointer further apart than this along the ring disagree.
MISMATCH_PX = 2.0


def knob_arc_width(radius_pt: float) -> float:
    return min(max(0.14 * radius_pt, 2.0), 3.0)


def _circle_mean(mask: np.ndarray, radius: float, samples: int = 72) -> np.ndarray:
    """Mean of `mask` on a circle of `radius` around every pixel."""
    height, width = mask.shape
    acc = np.zeros(mask.shape, np.float32)
    for k in range(samples):
        a = 2.0 * math.pi * k / samples
        dx = round(radius * math.sin(a))
        dy = round(-radius * math.cos(a))
        ys0, ys1 = max(0, dy), min(height, height + dy)
        xs0, xs1 = max(0, dx), min(width, width + dx)
        acc[ys0 - dy : ys1 - dy, xs0 - dx : xs1 - dx] += mask[ys0:ys1, xs0:xs1]
    return acc / samples


def detect_knob_centers(arr: np.ndarray, radii_pt, scale: float, threshold: float = 0.7):
    """Find knob body discs. Returns (score, x, y, radius_pt) per knob.

    A circle just inside the disc edge lies on body colour; one just outside it,
    in the gap before the value ring, lies on panel background. Buttons share
    the body colour but score zero: both circles land on the same fill. Real
    knobs score 0.94-1.0.
    """
    body = np.zeros(arr.shape[:2], bool)
    for token in ("SURFACE_ELEVATED", "SURFACE_HOVER"):
        body |= np.abs(arr - np.array(THEME[token])).max(axis=2) <= 2
    body = body.astype(np.float32)
    found = []
    for radius_pt in radii_pt:
        body_r = BODY_FRACTION * radius_pt * scale
        score = _circle_mean(body, body_r - 3) - _circle_mean(body, body_r + 2.5)
        for y, x in np.argwhere(score >= threshold):
            found.append((float(score[y, x]), int(x), int(y), radius_pt))
    found.sort(key=lambda f: -f[0])
    kept = []
    for score, x, y, radius_pt in found:
        reach = radius_pt * scale
        if any(abs(x - kx) < reach and abs(y - ky) < reach for _, kx, ky, _ in kept):
            continue
        kept.append((score, x, y, radius_pt))
    kept.sort(key=lambda k: (k[2] // 40, k[1]))
    return kept


def _window(arr: np.ndarray, cx: float, cy: float, half: int):
    """Square window around (cx, cy) plus its pixel coordinates, or None at the edge."""
    x0, y0 = round(cx) - half, round(cy) - half
    side = 2 * half + 1
    if x0 < 0 or y0 < 0 or y0 + side > arr.shape[0] or x0 + side > arr.shape[1]:
        return None
    yy, xx = np.mgrid[y0 : y0 + side, x0 : x0 + side]
    return arr[y0 : y0 + side, x0 : x0 + side], xx, yy


def refine_knob_center(arr: np.ndarray, x: int, y: int, radius_pt: float, scale: float):
    """Coverage-weighted centroid of the body disc alone.

    The window stops inside the gap between disc and ring, so neither the value
    ring nor the label below can pull the centre; a window that reaches the
    label drags it 10 deg or more off. Pointer pixels count as body, so the
    pointer does not push the centre away from itself.
    """
    lo, hi = max(THEME["PANEL_BG"]), max(THEME["SURFACE_ELEVATED"])
    body_r = BODY_FRACTION * radius_pt * scale
    cx, cy = float(x), float(y)
    for _ in range(3):
        win = _window(arr, cx, cy, int(body_r + 4))
        if win is None:
            return None
        sub, xx, yy = win
        weight = np.clip((sub.max(axis=2) - lo) / (hi - lo), 0.0, 1.0)
        weight[np.hypot(xx - cx, yy - cy) > body_r + 2.5] = 0.0
        total = weight.sum()
        if total == 0:
            return None
        cx, cy = float((weight * xx).sum() / total), float((weight * yy).sum() / total)
    return cx, cy


def measure_knob(arr: np.ndarray, cx: float, cy: float, radius_pt: float, scale: float):
    radius = radius_pt * scale
    arc_w = knob_arc_width(radius_pt) * scale
    ring_r = radius - arc_w / 2
    body_r = BODY_FRACTION * radius
    win = _window(arr, cx, cy, int(radius + 3))
    if win is None:
        return None
    sub, xx, yy = win
    dx, dy = xx - cx, yy - cy
    dist = np.hypot(dx, dy)
    red, blue = sub[..., 0], sub[..., 2]
    lum = sub.max(axis=2)
    sat = lum - sub.min(axis=2)

    band = np.abs(dist - ring_r) <= arc_w / 2 + 1
    accent = band & (red - blue > 40) & (red > 80)
    muted = band & (sat < 20) & (lum >= 95)
    pointer = (
        (dist >= POINTER_INNER * body_r - 2) & (dist <= body_r + 0.5) & (sat < 30) & (lum >= 100)
    )

    out = {"center": [round(cx, 1), round(cy, 1)], "radius_pt": radius_pt}
    if pointer.sum() >= 3:
        # The direction of the pointer's long axis, not the angle of its pixels
        # seen from the centre: egui snaps lines to the pixel grid, and a 1 px
        # sideways offset tilts that angle by up to 6 deg on a MEDIUM knob.
        px, py = dx[pointer], dy[pointer]
        mx, my = px.mean(), py.mean()
        values, vectors = np.linalg.eigh(np.cov(np.vstack([px - mx, py - my])))
        vx, vy = vectors[:, np.argmax(values)]
        if vx * mx + vy * my < 0:
            vx, vy = -vx, -vy
        needle = math.degrees(math.atan2(vx, -vy))
        out["needle_deg"] = round(needle, 1)
        out["needle_fraction"] = round(
            (needle - SWEEP_MIN_DEG) / (SWEEP_MAX_DEG - SWEEP_MIN_DEG), 3
        )

    arc = accent if accent.sum() >= muted.sum() else muted
    if arc.sum() < 3:
        out["arc_px"] = 0
        return out
    # Degrees clockwise from 12 o'clock; the sweep's gap sits at 180, so the
    # -135..135 sweep never wraps.
    angles = np.sort(np.degrees(np.arctan2(dx[arc], -dy[arc])))
    start, end = float(angles[0]), float(angles[-1])
    out["arc_start_deg"] = round(start, 1)
    out["arc_end_deg"] = round(end, 1)
    out["arc_px"] = int(arc.sum())
    out["arc_color"] = "accent" if arc is accent else "muted"
    if "needle_deg" in out:
        needle = out["needle_deg"]
        # The arc end nearest the pointer is its tip; the other end is where
        # the fill starts. A bipolar knob below centre fills from 0 down to the
        # pointer, so "the start" is not always the origin.
        tip, origin = (start, end) if abs(start - needle) <= abs(end - needle) else (end, start)
        tolerance = math.degrees(ORIGIN_TOL_PX / ring_r)
        if abs(origin - SWEEP_MIN_DEG) <= tolerance:
            out["fills_from"] = "min"
        elif abs(origin) <= tolerance:
            out["fills_from"] = "centre"
        else:
            out["fills_from"] = "other"
        delta = abs(tip - needle)
        out["arc_needle_delta_deg"] = round(delta, 1)
        out["arc_needle_delta_px"] = round(math.radians(delta) * ring_r, 1)
    return out


def knob_scale(arr: np.ndarray, override):
    """Pixels per point: capture width over GUI_WIDTH, unless --scale overrides it."""
    return override if override else arr.shape[1] / GUI_WIDTH


def cmd_knobs(args):
    arr = load(args.image)
    scale = knob_scale(arr, args.scale)
    radii = parse_radii(args.radii)
    tier = {r: name for name, r in KNOB_RADII.items()}
    knobs = []
    for score, x, y, radius_pt in detect_knob_centers(arr, radii, scale, args.threshold):
        centre = refine_knob_center(arr, x, y, radius_pt, scale)
        if centre is None:
            continue
        m = measure_knob(arr, *centre, radius_pt, scale)
        if m is None:
            continue
        m["tier"] = tier.get(radius_pt, f"r={radius_pt:g}pt")
        m["disc_score"] = round(score, 3)
        knobs.append(m)
    if args.json:
        print(json.dumps(knobs, indent=2))
        return
    tiers = sorted({k["tier"] for k in knobs})
    counts = ", ".join(f"{sum(k['tier'] == t for k in knobs)} {t}" for t in tiers)
    print(
        f"{len(knobs)} knobs in {os.path.basename(args.image)} ({counts}; {scale:g} px/pt;"
        f" sweep {SWEEP_MIN_DEG:g}..{SWEEP_MAX_DEG:g} deg)"
    )
    no_arc = 0
    for k in knobs:
        head = f"  ({k['center'][0]:7.1f},{k['center'][1]:7.1f}) {k['tier']:6s}"
        needle = (
            f"needle={k['needle_deg']:7.1f} ({k['needle_fraction']:.3f})"
            if "needle_deg" in k
            else "needle=   none        "
        )
        if k["arc_px"] == 0:
            no_arc += 1
            print(f"{head}  {needle}  arc=NONE")
            continue
        arc = f"arc=[{k['arc_start_deg']:7.1f},{k['arc_end_deg']:7.1f}] {k['arc_color']}"
        if "fills_from" not in k:
            print(f"{head}  {needle}  {arc}")
            continue
        flag = "  <-- ARC/NEEDLE MISMATCH" if k["arc_needle_delta_px"] > MISMATCH_PX else ""
        print(
            f"{head}  {needle}  {arc} from={k['fills_from']:6s}"
            f" delta={k['arc_needle_delta_deg']}deg/{k['arc_needle_delta_px']}px{flag}"
        )
    if no_arc:
        print(
            f"  {no_arc}/{len(knobs)} knobs draw no value arc. Correct only at a zero-length"
            f" fill: a unipolar knob at its minimum, a bipolar knob on its centre detent."
        )


def parse_radii(spec):
    """Comma list of knob outer radii in points; None means the theme's tiers."""
    if not spec:
        return sorted(KNOB_RADII.values(), reverse=True)
    return [float(part) for part in spec.split(",")]


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
        out.append(
            {
                "top": p["top"],
                "bottom": p["bottom"],
                "left": p["left"],
                "accent_px": int(mask.sum()),
            }
        )
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
            regions = [{"top": box[1], "bottom": box[3], "left": box[0], "interior": list(box)}]
        else:
            regions = sorted(find_panels(arr), key=lambda p: (p["top"], p["left"]))
        per_image[os.path.basename(path)] = accent_counts(arr, regions)
    if args.json:
        print(json.dumps(per_image, indent=2))
        return
    names = list(per_image)
    if len({len(v) for v in per_image.values()}) > 1:
        print("  WARNING: panel counts differ between images, so rows do not line up.")
        print("  Compare one group with --box instead.")
    rows = max(len(v) for v in per_image.values())
    print("accent pixels per panel. A disabled control drops the accent by design; flag")
    print("accent on a disabled control, or no accent where an enabled selection exists.")
    header = "  panel".ljust(30) + "".join(n.ljust(18) for n in names)
    print(header)
    first = per_image[names[0]]
    for i in range(rows):
        label = "-"
        if i < len(first):
            label = f"y {first[i]['top']}-{first[i]['bottom']} x {first[i]['left']}"
        cells = [str(per_image[n][i]["accent_px"]) if i < len(per_image[n]) else "-" for n in names]
        flag = ""
        numeric = [int(c) for c in cells if c.isdigit()]
        if numeric and min(numeric) == 0 and max(numeric) > 0:
            flag = "  <-- ACCENT IN SOME STATES ONLY"
        print("  " + label.ljust(28) + "".join(c.ljust(18) for c in cells) + flag)


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
        if f.endswith(".png")
        and not f.startswith("crop_")
        and not f.startswith("zone_")
        and "_crop_" not in f
        and "_zone_" not in f
    )
    if not images:
        print(f"no top-level capture PNGs in {directory}", file=sys.stderr)
        return 1

    def stem(path):
        return os.path.splitext(os.path.basename(path))[0]

    print("=" * 78)
    print("CAPTURE SET")
    print("=" * 78)
    width = expected_width()
    stale = stale_captures({path: os.path.getmtime(path) for path in images})
    for path in images:
        with Image.open(path) as im:
            w, h = im.size
        flag = "" if w == width else f"  <-- WIDTH {w}, EXPECTED {width}"
        if path in stale:
            flag += (
                f"  <-- STALE: written {format_age(stale[path])} before the newest"
                " capture; re-capture or remove it"
            )
        print(f"  {os.path.basename(path):20s} {w}x{h}{flag}")

    comparable = images
    base = next((i for i in comparable if os.path.basename(i) == args.base), comparable[0])
    for path in comparable:
        if path == base:
            continue
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
                image=path,
                min_pixels=COLORS_MIN_PIXELS,
                top=12,
                show_surfaces=False,
                json=False,
            )
        )

    # The base shows every knob at its default; `knobs` moves one bipolar and
    # one unipolar knob off it, which is the only way to test arc polarity.
    knob_states = [base] + [i for i in comparable if stem(i) == "knobs" and i != base]
    for path in knob_states:
        print()
        print("=" * 78)
        print(f"KNOBS  {os.path.basename(path)}")
        print("=" * 78)
        cmd_knobs(
            argparse.Namespace(image=path, radii=args.radii, scale=None, threshold=0.7, json=False)
        )

    print()
    print("=" * 78)
    print("ACCENT / SELECTION PRESENCE")
    print("=" * 78)
    cmd_accent(argparse.Namespace(images=comparable, box=None, json=False))
    return 0


# ---------------------------------------------------------------------------


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)

    p = sub.add_parser("report", help="run every check over a capture directory")
    p.add_argument("directory")
    p.add_argument("--base", default="latest.png", help="capture every other state is diffed against")
    p.add_argument(
        "--radii", default=None, help="knob outer radii in points, comma list (default: theme)"
    )
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
    p.add_argument(
        "--radii", default=None, help="outer radii in points, comma list (default: theme tiers)"
    )
    p.add_argument(
        "--scale", type=float, default=None, help="px per point (default: width / GUI_WIDTH)"
    )
    p.add_argument("--threshold", type=float, default=0.7, help="minimum body-disc score")
    p.set_defaults(func=cmd_knobs)

    p = sub.add_parser("text", help="glyph height and contrast for one probe box")
    p.add_argument("image")
    p.add_argument("--box", required=True, help="x0,y0,x1,y1")
    p.set_defaults(func=cmd_text)

    p = sub.add_parser("colors", help="colour inventory per panel, snapped to theme tokens")
    p.add_argument("image")
    p.add_argument("--min-pixels", dest="min_pixels", type=int, default=COLORS_MIN_PIXELS)
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
    load_gui_width(args.theme)
    return args.func(args) or 0


if __name__ == "__main__":
    sys.exit(main())
