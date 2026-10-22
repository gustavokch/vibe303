import argparse
import contextlib
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path

import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts"))
import gui_qa_measure as qa

S = 2  # Retina capture scale
CHASSIS = qa.THEME_FALLBACK["CHASSIS_BG"]
BODY = qa.THEME_FALLBACK["PANEL_BG"]
HEADER = qa.THEME_FALLBACK["PANEL_HEADER_BG"]
TITLEBAR = (0x38, 0x38, 0x38)


def flat_editor(titlebar_pt=32):
    """2x window capture of the 1216x640 editor: flat cards on the chassis plane.

    Geometry follows gui/mod.rs: MARGIN 4, PANEL_GAP 3, split_columns(1208) =
    (803, 402), rows 24 / 100 / 163 (80 + 3 + 80) / 102 / 231.
    """
    top = titlebar_pt * S
    img = np.zeros((top + 640 * S, 1216 * S, 3), np.int16)
    img[:top] = TITLEBAR
    img[top:] = CHASSIS
    cards = []

    def card(x, y, w, h, header_h=22):
        y0, y1, x0, x1 = top + y * S, top + (y + h) * S, x * S, (x + w) * S
        img[y0:y1, x0:x1] = BODY
        img[y0 : y0 + header_h * S, x0:x1] = HEADER
        cards.append((y0, y1, x0, x1))

    left, right = 803, 402
    xr = 4 + left + 3
    card(4, 4, 1208, 24, header_h=24)  # app bar
    y = 31
    card(4, y, left, 100)
    card(xr, y, right, 100)
    y += 103
    card(4, y, left, 80)
    card(4, y + 83, left, 80)
    card(xr, y, right, 163)
    y += 166
    card(4, y, left, 102)
    card(xr, y, right, 102)
    y += 105
    card(4, y, left, 231)
    card(xr, y, right, 231)
    return img, cards


class TestFindPanels(unittest.TestCase):
    def test_flat_cards_are_found_and_titlebar_is_not(self):
        img, cards = flat_editor()
        found = sorted(
            (p["top"], p["bottom"], p["left"], p["right"]) for p in qa.find_panels(img)
        )
        self.assertEqual(found, sorted(cards))

    def test_offscreen_render_without_titlebar(self):
        img, cards = flat_editor(titlebar_pt=0)
        self.assertEqual(len(qa.find_panels(img)), len(cards))

    def test_interior_starts_below_the_header_strip(self):
        img, _ = flat_editor()
        voice = next(p for p in qa.find_panels(img) if p["height"] == 100 * S)
        self.assertGreaterEqual(voice["interior"][1], voice["top"] + 22 * S)

    def test_blank_frame_has_no_panels(self):
        img = np.zeros((1280, 2432, 3), np.int16)
        img[:] = CHASSIS
        self.assertEqual(qa.find_panels(img), [])


def flat_knob(pointer, arc, radius_pt=21.0, centre=(120.3, 110.6), button=False):
    """2x render of one Knob303 (widgets/knob.rs) on the device body.

    `pointer` and `arc` are positions in the sweep (0 = min, 1 = max); `arc` is
    the (from, to) span of the value fill, None for a zero-length fill.
    """
    def deg(fraction):
        return qa.SWEEP_MIN_DEG + fraction * (qa.SWEEP_MAX_DEG - qa.SWEEP_MIN_DEG)

    img = np.zeros((240, 320, 3), np.int16)
    img[:] = BODY
    yy, xx = np.mgrid[0:240, 0:320]
    dx, dy = xx - centre[0], yy - centre[1]
    dist = np.hypot(dx, dy)
    angle = np.degrees(np.arctan2(dx, -dy))
    radius = radius_pt * S
    arc_w = qa.knob_arc_width(radius_pt) * S
    band = np.abs(dist - (radius - arc_w / 2)) <= arc_w / 2
    img[band & (np.abs(angle) <= 135)] = qa.THEME_FALLBACK["KNOB_TRACK"]
    if arc:
        lit = (angle >= deg(arc[0])) & (angle <= deg(arc[1]))
        img[band & lit] = qa.THEME_FALLBACK["ACCENT_ORANGE"]
    body_r = qa.BODY_FRACTION * radius
    img[dist <= body_r] = qa.THEME_FALLBACK["SURFACE_ELEVATED"]
    a = np.radians(deg(pointer))
    along = dx * np.sin(a) - dy * np.cos(a)
    across = dx * np.cos(a) + dy * np.sin(a)
    on_pointer = (np.abs(across) <= S) & (along >= qa.POINTER_INNER * body_r)
    on_pointer &= along <= body_r - S
    img[on_pointer] = qa.THEME_FALLBACK["TEXT_PRIMARY"]
    if button:
        # A raised button shares the knob body colour.
        img[180:228, 200:310] = qa.THEME_FALLBACK["SURFACE_ELEVATED"]
    return img


def measure_only_knob(img, radius_pt=21.0):
    found = qa.detect_knob_centers(img, [radius_pt], S)
    assert len(found) == 1, found
    _, x, y, r = found[0]
    return qa.measure_knob(img, *qa.refine_knob_center(img, x, y, r, S), r, S)


class TestKnobs(unittest.TestCase):
    def test_buttons_in_the_body_colour_are_not_knobs(self):
        img = flat_knob(0.75, (0.0, 0.75), button=True)
        found = qa.detect_knob_centers(img, [21.0, 12.0], S)
        self.assertEqual(len(found), 1)
        centre = qa.refine_knob_center(img, found[0][1], found[0][2], 21.0, S)
        self.assertAlmostEqual(centre[0], 120.3, delta=0.5)
        self.assertAlmostEqual(centre[1], 110.6, delta=0.5)

    def test_unipolar_arc_fills_from_min_to_the_pointer(self):
        for radius_pt in (21.0, 12.0):
            with self.subTest(radius_pt=radius_pt):
                k = measure_only_knob(flat_knob(0.75, (0.0, 0.75), radius_pt), radius_pt)
                self.assertAlmostEqual(k["needle_fraction"], 0.75, delta=0.01)
                self.assertEqual(k["fills_from"], "min")
                self.assertLessEqual(k["arc_needle_delta_px"], qa.MISMATCH_PX)

    def test_bipolar_arc_below_centre_fills_from_centre(self):
        # A full-negative bipolar arc spans -135..0 like a unipolar arc at 50 %;
        # the pointer decides which end is the origin.
        for pointer in (0.3, 0.0):
            with self.subTest(pointer=pointer):
                k = measure_only_knob(flat_knob(pointer, (pointer, 0.5)))
                self.assertEqual(k["fills_from"], "centre")
                self.assertLessEqual(k["arc_needle_delta_px"], qa.MISMATCH_PX)

    def test_arc_that_stops_short_of_the_pointer_is_a_mismatch(self):
        k = measure_only_knob(flat_knob(0.75, (0.0, 0.5)))
        self.assertGreater(k["arc_needle_delta_px"], qa.MISMATCH_PX)

    def test_zero_length_fill_draws_no_arc(self):
        k = measure_only_knob(flat_knob(0.5, None))
        self.assertEqual(k["arc_px"], 0)
        self.assertAlmostEqual(k["needle_fraction"], 0.5, delta=0.01)


def colors_report(img, name):
    """Run `colors --json` on an in-memory capture saved as `<name>.png`."""
    out = io.StringIO()
    with tempfile.TemporaryDirectory() as tmp:
        path = str(Path(tmp) / f"{name}.png")
        Image.fromarray(img.astype(np.uint8)).save(path)
        args = argparse.Namespace(
            image=path,
            min_pixels=qa.colors_min_pixels(path),
            top=12,
            show_surfaces=False,
            json=True,
        )
        with contextlib.redirect_stdout(out):
            qa.cmd_colors(args)
    return json.loads(out.getvalue())


def foreground_of(report, top):
    card = next(r for r in report if r["panel"]["top"] == top)
    return {c["hex"]: c for c in card["colors"] if c["foreground"]}


class TestColors(unittest.TestCase):
    def test_zoom_scaled_card_keeps_its_small_text(self):
        # zoom75 draws at 1.5 px/pt: a card of 8 pt text emits about 120 glyph
        # pixels per colour, below the 2 px/pt floor of 150.
        img, cards = flat_editor(titlebar_pt=0)
        y0, _, x0, _ = cards[3]  # SOUPED-UP ENGINE
        img[y0 + 60 : y0 + 70, x0 + 40 : x0 + 52] = qa.THEME_FALLBACK["TEXT_PRIMARY"]
        self.assertIn("#ECECEC", foreground_of(colors_report(img, "zoom75"), y0))


if __name__ == "__main__":
    unittest.main()
