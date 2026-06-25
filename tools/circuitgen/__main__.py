"""Regenerate every backend from the canonical IR.

    uv run --no-project python -m tools.circuitgen [--check]

Without ``--check``: writes each generated file. With ``--check``: regenerates in
memory and exits non-zero if any committed file is stale (the CI drift guard).
"""

from __future__ import annotations

import sys

from .targets import REPO_ROOT, render_circuits


def main(argv: list[str]) -> int:
    check = "--check" in argv
    rendered = render_circuits()

    stale: list[str] = []
    for path, text in rendered.items():
        rel = path.relative_to(REPO_ROOT)
        if check:
            current = path.read_text() if path.exists() else None
            if current != text:
                stale.append(str(rel))
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
            print(f"wrote {rel}")

    if check:
        if stale:
            print("STALE generated files (run `python -m tools.circuitgen`):", file=sys.stderr)
            for s in stale:
                print(f"  - {s}", file=sys.stderr)
            return 1
        print("OK: all generated backends are up to date with the IR")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
