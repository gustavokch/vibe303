# schemagen — schematic image → circuit IR

SINA-style (arXiv 2601.22114v1) front-end that turns a schematic image into a
**draft** `circuit/*.toml` IR for the circuitgen pipeline. The draft is always
human-reviewed before promotion; a closed-loop ngspice `.op` gate (Masala-CHAI V2
idea) rejects non-convergent drafts up front.

## Workflow

```
schematic.jpg ──schemagen──▶ circuit/foo.draft.toml ──human review──▶ circuit/foo.toml
                   │                                                        │
                   └─ validate: structural + ngspice .op            python -m tools.circuitgen
                                                                     (emits SPICE / Rust / ACME)
```

## Usage

```bash
# Cloud VLM backend (default) — needs an OpenRouter key, no torch:
export OPENROUTER_API_KEY=sk-or-...
uv run --no-project python -m tools.schemagen path/to/schematic.jpg \
    --out circuit/foo.draft.toml --hint "Neve-style mic preamp, +24V rail"

# Validate an existing draft (or any IR) on its own:
uv run --no-project python -m tools.schemagen --validate circuit/foo.draft.toml

# Local/offline backend (deps-gated, see local.py):
uv run --no-project python -m tools.schemagen schematic.jpg --backend local
```

After review, rename the draft to `circuit/<name>.toml` and run
`python -m tools.circuitgen` to generate the SPICE/Rust/ACME backends.

## Backends

| Backend | Deps | Status |
|---|---|---|
| `vlm` (default) | stdlib only (`urllib`) + `OPENROUTER_API_KEY` | implemented; GPT-4o vision emits the circuit.toml schema directly |
| `local` | `ultralytics` + `easyocr` + `opencv` (torch) | scaffolded stub — YOLO11 detect + CCL connectivity (junction-dot) + EasyOCR; needs a trained detector |

## Validating the BA283 reconstruction

The intended first use is a **regression check** of the existing hand-authored
`circuit/ba283.toml` against the original board image (`1073PRE2.jpg`) /
transformer drawing (`docs/xformers/NeveXfmr.jpeg`): digitize the image, then diff
the draft IR against the hand-authored one to surface any topology disagreement
before trusting the tool on a new circuit.

## Notes / verification status

- The **closed-loop validation** (`validate.py`: structural gate + ngspice `.op`)
  is implemented and verified against `circuit/ba283.toml` (RESULT: OK).
- The **VLM backend** (`vlm.py`) is stdlib-only and ready, but un-run here (needs a
  key). The deck builder ties `role="input"` nodes to a 0 V source so the `.op`
  has a defined DC point (the real backends drive the input at runtime).
- The **local backend** is a documented stub (torch/model not available in this
  environment).
