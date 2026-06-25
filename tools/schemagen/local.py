"""Local/offline schematic digitization backend (deps-gated).

Mirrors the SINA / AMSnet 2.0 classical pipeline so no cloud LLM is needed:

  1. Component detection — YOLO11 (ultralytics) bounding boxes per component class.
  2. Connectivity inference — Connected-Component Labeling on the wire layer, with
     junction-dot handling (AMSnet 2.0) to resolve crossings vs. true connections.
  3. Designator + value OCR — EasyOCR over the text regions near each component.
  4. Assembly — fuse detections + nets + designators into the circuit.toml schema.

This requires heavy dependencies (torch via ultralytics + easyocr + opencv) and a
component-detection model. It is intentionally a documented stub: the cloud VLM
backend (``vlm.py``) is the default and needs no torch. Implement the stages here
when a labelled component-detection model is available.

Install: ``uv pip install ultralytics easyocr opencv-python-headless``
"""

from __future__ import annotations

from pathlib import Path


def _require_deps() -> None:
    missing = []
    for mod in ("ultralytics", "easyocr", "cv2"):
        try:
            __import__(mod)
        except ImportError:
            missing.append(mod)
    if missing:
        raise ImportError(
            "local backend needs: " + ", ".join(missing) + " — "
            "install `ultralytics easyocr opencv-python-headless`, or use the "
            "default VLM backend (OPENROUTER_API_KEY)."
        )


def digitize(image_path: str | Path, *, hint: str | None = None) -> str:  # noqa: ARG001
    """schematic image -> draft circuit.toml text (local pipeline).

    Raises ImportError if the optional CV/ML deps are absent, and
    NotImplementedError once they are present (pipeline stages are TODO and need a
    trained component-detection model).
    """
    _require_deps()
    raise NotImplementedError(
        "local YOLO11 + CCL + EasyOCR pipeline is scaffolded but not implemented; "
        "see this module's docstring for the stage plan. Use the VLM backend."
    )
