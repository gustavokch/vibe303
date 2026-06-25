"""schemagen — schematic image -> draft circuit IR (SINA-style front-end).

Pipeline (arXiv 2601.22114v1 / SINA, Masala-CHAI V2 closed-loop idea):

    schematic.{jpg,png,pdf}
        -> [detect components + infer connectivity + read designators]
        -> circuit/<name>.draft.toml   (a DRAFT IR for human review)
        -> emit SPICE via circuitgen -> ngspice .op -> convergence gate

Two backends:
  * ``vlm`` (default): OpenRouter GPT-4o vision — stdlib-only HTTP, no torch.
    Needs OPENROUTER_API_KEY. The VLM does detection + connectivity + netlist in
    one structured pass and emits our canonical circuit.toml schema directly.
  * ``local`` (fallback, deps-gated): YOLO11 detection + Connected-Component
    connectivity (AMSnet junction-dot handling) + EasyOCR designators. Requires
    ``ultralytics`` + ``easyocr`` + ``opencv`` (heavy: torch). See README.

The draft IR is NEVER authoritative: it is reviewed/corrected by a human and only
then promoted to circuit/<name>.toml. Validation reuses tools.circuitgen so a
draft that does not converge in ngspice is rejected before review.
"""
