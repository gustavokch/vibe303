"""schemagen CLI: schematic image -> validated draft circuit IR.

    uv run --no-project python -m tools.schemagen IMAGE [--out circuit/foo.draft.toml]
                                                  [--backend vlm|local]
                                                  [--hint "..."] [--no-sim]
    uv run --no-project python -m tools.schemagen --validate circuit/foo.draft.toml

The output is a DRAFT for human review — never promoted to circuit/<name>.toml
automatically. After review, rename it and run `python -m tools.circuitgen` to
generate the backends.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(prog="schemagen")
    ap.add_argument("image", nargs="?", help="schematic image (jpg/png/pdf)")
    ap.add_argument("--out", help="draft IR output path (default: <image>.draft.toml)")
    ap.add_argument("--backend", choices=["vlm", "local"], default="vlm")
    ap.add_argument("--hint", help="extra context passed to the VLM")
    ap.add_argument("--no-sim", action="store_true", help="skip the ngspice .op gate")
    ap.add_argument("--validate", metavar="DRAFT", help="only validate an existing draft IR")
    args = ap.parse_args(argv)

    from .validate import validate

    # validate-only mode
    if args.validate:
        res = validate(args.validate, run_sim=not args.no_sim)
        for m in res.messages:
            print(m)
        print("RESULT:", "OK" if res.ok else "REJECTED")
        return 0 if res.ok else 1

    if not args.image:
        ap.error("IMAGE is required unless --validate is given")

    image = Path(args.image)
    out = Path(args.out) if args.out else image.with_suffix(".draft.toml")

    if args.backend == "vlm":
        from .vlm import digitize

        draft = digitize(image, extra_hint=args.hint)
    else:
        try:
            from .local import digitize  # deps-gated (torch/ultralytics/easyocr)
        except ImportError as e:
            print(
                f"local backend unavailable: {e}\n"
                "Install with: uv pip install ultralytics easyocr opencv-python-headless",
                file=sys.stderr,
            )
            return 2
        draft = digitize(image, hint=args.hint)

    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(draft)
    print(f"wrote draft IR -> {out}")

    # closed-loop validation before handing to a human
    res = validate(out, run_sim=not args.no_sim)
    for m in res.messages:
        print(m)
    if not res.ok:
        print("DRAFT REJECTED by validation — review the messages above; the draft is "
              "kept for inspection but should not be promoted.", file=sys.stderr)
        return 1
    print(f"DRAFT OK — review {out} by hand, then rename to circuit/<name>.toml and run "
          "`python -m tools.circuitgen`.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
