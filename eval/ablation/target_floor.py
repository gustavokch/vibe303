"""Ablation cell C7: estimate the achievable ESR *floor* of the SPICE target.

If independent white-box models (ACME.jl, WDF) disagree with the SPICE renders by
roughly as much as the neural model's ``val_esr`` (~0.097), then fidelity is
**target-bound** -- no architecture or data change on the neural side can beat a
target the reference models themselves cannot reproduce. In that case the redesign
starts at data-gen (SPICE topology / oversampling), not the model.

``summarize_floor`` and ``esrs_from_summary`` are the pure, tested core. The
``__main__`` CLI is orchestration: it runs the existing
``eval/{wdf,acme}_vs_spice/compare.py`` harnesses on the subset stimuli/gains
(their real CLI: ``--stims/--gains/--bias/--out``), reads each harness's
``summary.json``, converts the per-row ``esr_db`` to linear ESR, and feeds them to
``summarize_floor``. See ``docs/plans/2026-06-06-ablation-matrix-design.md`` (C7).
"""

from collections.abc import Mapping

import numpy as np
import pandas as pd


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
        # A NaN median (from a diverged/exploded reference render) would make
        # ``any(nan >= threshold)`` False and silently CLEAR the gate -- falsely
        # steering the redesign toward the model instead of data-gen. Reject it,
        # mirroring ``metrics.esr``'s non-finite policy.
        if not np.isfinite(med):
            raise ValueError(
                f"summarize_floor: non-finite median ESR for model {model!r}"
            )
        out[f"{model}_median"] = med
        medians.append(med)
    out["target_bound"] = any(m >= threshold for m in medians)
    return out


def esrs_from_summary(summary: Mapping[str, object]) -> list[float]:
    """Extract per-row **linear** ESR from a wdf/acme ``compare.py`` summary.json.

    The two harnesses store their per-point rows under different keys -- WDF uses
    ``gain_ladder`` (``{stem: [rows]}``), ACME uses ``points`` (``{key: [rows]}``)
    -- but each row carries an ``esr_db`` (or ``None`` for a failed SPICE/model
    point). dB is converted back to linear (``10**(esr_db/10)``) because
    ``summarize_floor`` and the neural ``val_esr`` live in the linear domain.

    Rows with a missing/``None`` ``esr_db`` (failed points) are skipped. Raises
    ``ValueError`` if neither container key is present or no valid row remains
    (an empty floor would otherwise read as a silent pass).
    """
    container = summary.get("gain_ladder")
    if container is None:
        container = summary.get("points")
    if container is None:
        raise ValueError(
            "esrs_from_summary: summary has neither 'gain_ladder' nor 'points'"
        )
    esrs: list[float] = []
    for rows in container.values():  # type: ignore[union-attr]
        for row in rows:
            db = row.get("esr_db")
            if db is None:
                continue
            esrs.append(10.0 ** (float(db) / 10.0))
    if not esrs:
        raise ValueError("esrs_from_summary: no valid esr_db rows in summary")
    return esrs


def subset_stims_gains(df: pd.DataFrame) -> tuple[list[str], list[float]]:
    """Derive harness ``--stims`` stems and ``--gains`` grid from a subset manifest.

    The harnesses take stimulus *stems* (the ``dry_path`` basename without the
    ``_dry.wav`` suffix) and a ``gain_norm`` grid in ``[0, 1]``. Returns
    ``(stims, gains)`` with stems de-duplicated in first-seen order and gains
    sorted ascending.
    """
    stims: list[str] = []
    for path in df["dry_path"]:
        name = str(path).rsplit("/", 1)[-1]
        for suffix in ("_dry.wav", ".wav"):
            if name.endswith(suffix):
                name = name[: -len(suffix)]
                break
        if name not in stims:
            stims.append(name)
    gains = sorted({float(g) for g in df["gain_value"]})
    return stims, gains


def _main() -> None:  # pragma: no cover - heavy harness orchestration
    import argparse
    import json
    import subprocess
    import tempfile
    from pathlib import Path

    parser = argparse.ArgumentParser(description="C7 target-floor check")
    parser.add_argument("--subset", type=Path, help="subset manifest CSV (derives --stims/--gains)")
    parser.add_argument("--stims", help="comma-separated stems (overrides --subset)")
    parser.add_argument("--gains", help="comma-separated gain_norm grid (overrides --subset)")
    parser.add_argument("--bias", type=float, default=0.77)
    parser.add_argument("--threshold", type=float, default=0.05)
    parser.add_argument(
        "--out-dir", type=Path, default=None, help="harness output root (default: temp)"
    )
    # Point at an existing summary.json to reuse a prior run instead of re-running.
    parser.add_argument("--wdf-summary", type=Path, default=None)
    parser.add_argument("--acme-summary", type=Path, default=None)
    parser.add_argument("--wdf-compare", type=Path, default=Path("eval/wdf_vs_spice/compare.py"))
    parser.add_argument("--acme-compare", type=Path, default=Path("eval/acme_vs_spice/compare.py"))
    args = parser.parse_args()

    if args.stims and args.gains:
        stims = [s.strip() for s in args.stims.split(",") if s.strip()]
        gains = [float(x) for x in args.gains.split(",")]
    elif args.subset is not None:
        stims, gains = subset_stims_gains(pd.read_csv(args.subset))
    else:
        raise SystemExit("[C7] need --subset OR both --stims and --gains")

    out_root = args.out_dir or Path(tempfile.mkdtemp(prefix="c7_floor_"))
    gains_arg = ",".join(str(g) for g in gains)
    stims_arg = ",".join(stims)

    refs = (
        ("wdf", args.wdf_compare, args.wdf_summary),
        ("acme", args.acme_compare, args.acme_summary),
    )
    esrs: dict[str, list[float]] = {}
    missing: list[str] = []
    for name, script, preset in refs:
        if preset is not None:
            summary = json.loads(Path(preset).read_text())
        elif script.exists():
            out_dir = out_root / name
            out_dir.mkdir(parents=True, exist_ok=True)
            subprocess.run(
                ["python", str(script), "--stims", stims_arg, "--gains", gains_arg,
                 "--bias", str(args.bias), "--out", str(out_dir)],
                check=True,
            )
            summary = json.loads((out_dir / "summary.json").read_text())
        else:
            missing.append(name)
            print(f"[C7] WARNING skip {name}: {script} not present and no --{name}-summary")
            continue
        esrs[name] = esrs_from_summary(summary)

    if not esrs:
        raise SystemExit("[C7] no reference harness available; cannot estimate floor")

    summary_out = summarize_floor(esrs, threshold=args.threshold)
    summary_out["reference_models"] = sorted(esrs)
    if missing:
        # A single-source floor (e.g. WDF only) is weaker than ACME+WDF agreement
        # and must not read as a full verdict.
        summary_out["missing_references"] = sorted(missing)
        print(f"[C7] WARNING floor is single-source; missing: {sorted(missing)}")
    print(json.dumps(summary_out, indent=2))
    if summary_out["target_bound"]:
        print("[C7] TARGET-BOUND: redesign starts at data-gen, not the model.")


if __name__ == "__main__":  # pragma: no cover
    _main()
