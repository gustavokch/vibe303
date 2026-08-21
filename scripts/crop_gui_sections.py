#!/usr/bin/env python3
"""
crop_gui_sections.py
Extracts 3 broad zones and 5 discrete section crops from GUI screenshot captures.
Supports dynamic Retina (2x) and standard (1x) scale factors.
"""

import sys
from pathlib import Path
from PIL import Image

# Logical coordinate definitions for GUI (Logical Width: 1080, Logical Content Height: 720, Titlebar: 32)
# format: (y_start_pt, y_end_pt)
DISCRETE_SECTIONS = {
    "crop_header": (0, 52),
    "crop_classic": (48, 245),
    "crop_souped": (240, 390),
    "crop_clipper_tweaks": (385, 520),
    "crop_sequencer": (515, 720),
}

BROAD_ZONES = {
    "zone_top": (0, 245),
    "zone_mid": (240, 520),
    "zone_bottom": (515, 720),
}


def process_image(image_path: Path, output_dir: Path):
    if not image_path.is_file():
        print(f"Skipping missing file: {image_path}", file=sys.stderr)
        return

    stem = image_path.stem  # e.g., "latest", "accurate", "running"
    img = Image.open(image_path)
    w, h = img.size

    scale = w / 1080.0
    titlebar_h = int(32 * scale) if h > int(720 * scale) else 0

    all_crops = {**BROAD_ZONES, **DISCRETE_SECTIONS}

    for name, (y_start_pt, y_end_pt) in all_crops.items():
        y1 = max(0, titlebar_h + int(y_start_pt * scale))
        y2 = min(h, titlebar_h + int(y_end_pt * scale))
        box = (0, y1, w, y2)

        cropped = img.crop(box)

        # Save state-prefixed file: e.g. latest_crop_header.png, accurate_crop_souped.png
        prefixed_filename = f"{stem}_{name}.png"
        prefixed_path = output_dir / prefixed_filename
        cropped.save(prefixed_path)
        print(f"Generated: {prefixed_path.name} ({cropped.size[0]}x{cropped.size[1]})")

        # If processing 'latest', also save canonical un-prefixed alias
        if stem == "latest":
            alias_path = output_dir / f"{name}.png"
            cropped.save(alias_path)


def main():
    repo_root = Path(__file__).resolve().parent.parent
    qa_dir = repo_root / "target" / "gui-qa"

    if len(sys.argv) > 1:
        qa_dir = Path(sys.argv[1]).resolve()

    if not qa_dir.exists():
        print(f"QA directory does not exist: {qa_dir}", file=sys.stderr)
        sys.exit(1)

    target_stems = ["latest", "accurate", "running"]
    for stem in target_stems:
        img_path = qa_dir / f"{stem}.png"
        if img_path.exists():
            print(f"\nProcessing {img_path.name} ({img_path.stat().st_size} bytes)...")
            process_image(img_path, qa_dir)


if __name__ == "__main__":
    main()
