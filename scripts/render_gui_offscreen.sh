#!/usr/bin/env bash
# Render every GUI QA state offscreen (no window, no Screen Recording permission)
# and cut the section crops. Output names match scripts/capture_standalone_gui.sh,
# so gui_qa_measure.py and crop_gui_sections.py read it unchanged.
#
# Usage: scripts/render_gui_offscreen.sh [OUTPUT_DIR]   (default target/gui-qa-offscreen)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OUTPUT_DIR="${1:-$REPO_ROOT/target/gui-qa-offscreen}"

cd "$REPO_ROOT"
cargo run --release -p gui_render -- --out "$OUTPUT_DIR"

python3 "$SCRIPT_DIR/crop_gui_sections.py" "$OUTPUT_DIR"

echo "Offscreen renders and crops are in $OUTPUT_DIR"
echo "Measure: python3 scripts/gui_qa_measure.py report $OUTPUT_DIR"
