#!/usr/bin/env python3
"""
crop_gui_sections.py
Extracts dynamic section crops, broad zones, right/bottom boundary margins, and multi-state composite strips.
Parses dimensions directly from plugin303/src/gui/mod.rs.
"""

import re
import sys
from pathlib import Path
from PIL import Image


def parse_gui_dimensions() -> dict:
    repo_root = Path(__file__).resolve().parent.parent
    gui_mod = repo_root / "plugin303" / "src" / "gui" / "mod.rs"
    dims = {"width": 1080, "height": 720}
    if gui_mod.exists():
        content = gui_mod.read_text(encoding="utf-8")
        w_match = re.search(r"pub\s+const\s+GUI_WIDTH:\s*u32\s*=\s*(\d+);", content)
        h_match = re.search(r"pub\s+const\s+GUI_HEIGHT:\s*u32\s*=\s*(\d+);", content)
        if w_match:
            dims["width"] = int(w_match.group(1))
        if h_match:
            dims["height"] = int(h_match.group(1))
    return dims


def get_crop_definitions(width: int, height: int):
    # Row bounds in logical px for the 1216x640 layout (gui/mod.rs row stack):
    # app bar, voice+envelopes, engine (souped/clipper/tweaks), generator, sequencer.
    scale_y = height / 640.0
    sections = {
        "crop_header": (0, int(31 * scale_y)),
        "crop_voice": (int(31 * scale_y), int(134 * scale_y)),
        "crop_engine": (int(134 * scale_y), int(300 * scale_y)),
        "crop_generator": (int(300 * scale_y), int(405 * scale_y)),
        "crop_sequencer": (int(405 * scale_y), height),
        "zone_top": (0, int(134 * scale_y)),
        "zone_mid": (int(134 * scale_y), int(405 * scale_y)),
        "zone_bottom": (int(405 * scale_y), height),
    }
    return sections


def process_image(image_path: Path, output_dir: Path, dims: dict):
    if not image_path.is_file():
        return

    stem = image_path.stem
    img = Image.open(image_path)
    w, h = img.size
    scale = w / float(dims["width"])
    titlebar_h = int(32 * scale) if h > int(dims["height"] * scale) else 0

    sections = get_crop_definitions(dims["width"], dims["height"])

    for name, (y_start_pt, y_end_pt) in sections.items():
        y1 = max(0, titlebar_h + int(y_start_pt * scale))
        y2 = min(h, titlebar_h + int(y_end_pt * scale))
        cropped = img.crop((0, y1, w, y2))

        prefixed_path = output_dir / f"{stem}_{name}.png"
        cropped.save(prefixed_path)

        if stem == "latest":
            alias_path = output_dir / f"{name}.png"
            cropped.save(alias_path)


def generate_boundary_and_diff_crops(output_dir: Path, dims: dict):
    latest_p = output_dir / "latest.png"
    fun_p = output_dir / "accurate.png"
    running_p = output_dir / "running.png"

    if latest_p.exists():
        img = Image.open(latest_p)
        w, h = img.size
        scale = w / float(dims["width"])
        titlebar_h = int(32 * scale) if h > int(dims["height"] * scale) else 0

        # Right margin: rightmost 15% of width across full height
        x_right_start = int(w * 0.85)
        right_crop = img.crop((x_right_start, titlebar_h, w, h))
        right_crop.save(output_dir / "crop_right_margin.png")

        # Bottom margin: bottom 12% of height
        y_bottom_start = max(0, h - int(85 * scale))
        bottom_crop = img.crop((0, y_bottom_start, w, h))
        bottom_crop.save(output_dir / "crop_bottom_margin.png")

    # Generate state diff composite
    if latest_p.exists() and fun_p.exists() and running_p.exists():
        img_lat = Image.open(latest_p)
        img_acc = Image.open(fun_p)
        img_run = Image.open(running_p)

        w, h = img_lat.size
        scale = w / float(dims["width"])
        titlebar_h = int(32 * scale) if h > int(dims["height"] * scale) else 0
        y_mid_start = titlebar_h + int(134 * scale)
        y_mid_end = titlebar_h + int(405 * scale)

        mid_lat = img_lat.crop((0, y_mid_start, w, y_mid_end))
        mid_acc = img_acc.crop((0, y_mid_start, w, y_mid_end))
        mid_run = img_run.crop((0, y_mid_start, w, y_mid_end))

        diff_h = mid_lat.height * 3
        composite = Image.new("RGBA", (w, diff_h), (20, 20, 20, 255))
        composite.paste(mid_lat, (0, 0))
        composite.paste(mid_acc, (0, mid_lat.height))
        composite.paste(mid_run, (0, mid_lat.height * 2))
        composite.save(output_dir / "crop_state_diff.png")


# States captured at a non-default egui zoom factor render the UI scaled
# inside the window, so the fixed 100%-zoom section coordinates do not apply
# to them. Full screenshots are kept; per-section crops would land on the
# wrong regions.
ZOOM_SCALED_STATES = {"zoom75"}


def process_all_states(qa_dir: Path):
    dims = parse_gui_dimensions()
    for stem in [
        "latest",
        "accurate",
        "advanced",
        "running",
        "running_note",
        "knobs",
        "quality_high",
        "generator",
        "zoom75",
    ]:
        if stem in ZOOM_SCALED_STATES:
            continue
        p = qa_dir / f"{stem}.png"
        if p.exists():
            process_image(p, qa_dir, dims)
    generate_boundary_and_diff_crops(qa_dir, dims)


def main():
    repo_root = Path(__file__).resolve().parent.parent
    qa_dir = repo_root / "target" / "gui-qa"
    if len(sys.argv) > 1:
        qa_dir = Path(sys.argv[1]).resolve()
    if not qa_dir.exists():
        qa_dir.mkdir(parents=True, exist_ok=True)
    process_all_states(qa_dir)


if __name__ == "__main__":
    main()
