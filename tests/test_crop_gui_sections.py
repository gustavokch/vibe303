import os
import sys
import unittest
from pathlib import Path
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts"))
import crop_gui_sections


class TestCropGuiSections(unittest.TestCase):
    def setUp(self):
        self.test_dir = Path("/tmp/test_gui_crop")
        self.test_dir.mkdir(parents=True, exist_ok=True)
        # Create mock 2160x1440 image (2x Retina of 1080x720)
        img = Image.new("RGBA", (2160, 1440), color=(30, 30, 30, 255))
        img.save(self.test_dir / "latest.png")
        img.save(self.test_dir / "accurate.png")
        img.save(self.test_dir / "running.png")

    def tearDown(self):
        import shutil
        if self.test_dir.exists():
            shutil.rmtree(self.test_dir)

    def test_parse_gui_dimensions(self):
        dims = crop_gui_sections.parse_gui_dimensions()
        self.assertEqual(dims["width"], 1280)
        self.assertEqual(dims["height"], 720)

    def test_generate_all_crops(self):
        crop_gui_sections.process_all_states(self.test_dir)
        expected_files = [
            "latest_crop_header.png",
            "latest_crop_classic.png",
            "latest_crop_souped.png",
            "latest_crop_clipper_tweaks.png",
            "latest_crop_sequencer.png",
            "crop_right_margin.png",
            "crop_bottom_margin.png",
            "crop_state_diff.png",
        ]
        for f in expected_files:
            p = self.test_dir / f
            self.assertTrue(p.exists(), f"Expected crop file {f} was not generated")


if __name__ == "__main__":
    unittest.main()
