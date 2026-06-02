#!/usr/bin/env bash
# Build ba283_train_bundle.zip for Colab T4 training.
# Contains the train/ package + manifests + dry/wet WAVs, paths preserved so the
# manifests' relative data/... paths resolve from the unzip root (/content/ba283).
set -euo pipefail
cd "$(dirname "$0")/.."

OUT="colab/ba283_train_bundle.zip"
rm -f "$OUT"

# zip binary may be absent; use Python's zipfile (always present via uv venv).
uv run python - "$OUT" <<'PY'
import sys, zipfile
from pathlib import Path

out = Path(sys.argv[1])
roots = [
    Path("train"),
    Path("data/dry"),
    Path("data/wet"),
]
files = [
    Path("data/train.csv"),
    Path("data/val.csv"),
    Path("data/test.csv"),
    Path("data/manifest.csv"),
]
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    for r in roots:
        for p in sorted(r.rglob("*")):
            if p.is_file() and "__pycache__" not in p.parts:
                z.write(p, p.as_posix())
    for f in files:
        z.write(f, f.as_posix())
print("wrote", out, f"({out.stat().st_size/1e6:.1f} MB)")
PY
