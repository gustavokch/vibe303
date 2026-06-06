"""Ablation cell C7: estimate the achievable ESR *floor* of the SPICE target.

If independent white-box models (ACME.jl, WDF) disagree with the SPICE renders by
roughly as much as the neural model's ``val_esr`` (~0.097), then fidelity is
**target-bound** -- no architecture or data change on the neural side can beat a
target the reference models themselves cannot reproduce. In that case the redesign
starts at data-gen (SPICE topology / oversampling), not the model.

``summarize_floor`` is the pure, tested core. The ``__main__`` CLI is thin
orchestration: it shells out to the existing ``eval/acme_vs_spice/compare.py`` and
``eval/wdf_vs_spice/compare.py`` harnesses on the subset and feeds their per-row
ESR values here. See ``docs/plans/2026-06-06-ablation-matrix-design.md`` (C7 gate).
"""

import numpy as np


def summarize_floor(
    esrs: dict[str, list[float]], *, threshold: float
) -> dict[str, object]:
    """Summarize reference-model-vs-SPICE ESRs into a floor verdict.

    ``esrs`` maps a reference model name (e.g. ``"acme"``, ``"wdf"``) to its list
    of per-row ESR values against the SPICE target. Returns ``{<model>_median:
    float, ..., target_bound: bool}``. ``target_bound`` is True when **any**
    model's median ESR is >= ``threshold`` (the worst reference gates the floor:
    a single model failing to reproduce the target already caps achievable
    fidelity).

    Raises ``ValueError`` for an empty ESR list (median undefined -> a silent NaN
    would corrupt the verdict).
    """
    out: dict[str, object] = {}
    medians = []
    for model, values in esrs.items():
        if len(values) == 0:
            raise ValueError(f"summarize_floor: empty ESR list for model {model!r}")
        med = float(np.median(values))
        out[f"{model}_median"] = med
        medians.append(med)
    out["target_bound"] = any(m >= threshold for m in medians)
    return out


def _main() -> None:  # pragma: no cover - thin CLI orchestration
    import argparse
    import json
    import subprocess
    from pathlib import Path

    parser = argparse.ArgumentParser(description="C7 target-floor check")
    parser.add_argument("--subset", type=Path, required=True, help="subset manifest CSV")
    parser.add_argument("--threshold", type=float, default=0.05)
    parser.add_argument(
        "--acme-compare",
        type=Path,
        default=Path("eval/acme_vs_spice/compare.py"),
    )
    parser.add_argument(
        "--wdf-compare",
        type=Path,
        default=Path("eval/wdf_vs_spice/compare.py"),
    )
    args = parser.parse_args()

    esrs: dict[str, list[float]] = {}
    for name, script in (("acme", args.acme_compare), ("wdf", args.wdf_compare)):
        if not script.exists():
            print(f"[C7] skip {name}: {script} not present")
            continue
        # Each harness is expected to emit a JSON line {"esr": [...]} on stdout for
        # the given subset; adapt the flags here to the harness's actual CLI.
        proc = subprocess.run(
            ["python", str(script), "--manifest", str(args.subset), "--emit-esr"],
            capture_output=True,
            text=True,
            check=True,
        )
        esrs[name] = json.loads(proc.stdout.strip().splitlines()[-1])["esr"]

    if not esrs:
        raise SystemExit("[C7] no reference harness available; cannot estimate floor")

    summary = summarize_floor(esrs, threshold=args.threshold)
    print(json.dumps(summary, indent=2))
    if summary["target_bound"]:
        print("[C7] TARGET-BOUND: redesign starts at data-gen, not the model.")


if __name__ == "__main__":  # pragma: no cover
    _main()
