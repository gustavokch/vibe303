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
        # Mock capture: 2x Retina of the 1216x640 editor
        img = Image.new("RGBA", (2432, 1280), color=(30, 30, 30, 255))
        img.save(self.test_dir / "latest.png")
        img.save(self.test_dir / "accurate.png")
        img.save(self.test_dir / "running.png")

    def tearDown(self):
        import shutil
        if self.test_dir.exists():
            shutil.rmtree(self.test_dir)

    def test_parse_gui_dimensions(self):
        # Parsed from gui/mod.rs, not the built-in fallback: a regex that stops
        # matching the Rust source must fail here, not crop the wrong rows.
        dims = crop_gui_sections.parse_gui_dimensions()
        self.assertNotEqual(dims, {"width": 1080, "height": 720})

    def test_generate_all_crops(self):
        # Create all extended QA state mock images
        for stem in ["advanced", "knobs", "quality_high", "generator", "zoom75"]:
            img = Image.new("RGBA", (2432, 1280), color=(30, 30, 30, 255))
            img.save(self.test_dir / f"{stem}.png")

        crop_gui_sections.process_all_states(self.test_dir)
        expected_files = [
            "latest_crop_header.png",
            "latest_crop_voice.png",
            "latest_crop_engine.png",
            "latest_crop_generator.png",
            "latest_crop_sequencer.png",
            "crop_right_margin.png",
            "crop_bottom_margin.png",
            "crop_state_diff.png",
            "generator_crop_header.png",
            "quality_high_crop_voice.png",
            "knobs_crop_voice.png",
            "advanced_crop_engine.png",
        ]
        for f in expected_files:
            p = self.test_dir / f
            self.assertTrue(p.exists(), f"Expected crop file {f} was not generated")

        # Verify zoom-scaled states do not produce per-section crops
        for section in ["crop_header", "crop_voice", "crop_engine", "crop_sequencer"]:
            zoom_crop = self.test_dir / f"zoom75_{section}.png"
            self.assertFalse(
                zoom_crop.exists(),
                f"Zoom-scaled state crop {zoom_crop.name} should not be generated",
            )


if __name__ == "__main__":
    unittest.main()
