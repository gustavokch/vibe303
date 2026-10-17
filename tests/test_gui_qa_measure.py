import sys
import unittest
from pathlib import Path

import numpy as np

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


if __name__ == "__main__":
    unittest.main()
