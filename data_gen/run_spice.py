"""SPICE execution runner for BA283 wet-target generation.

Pushes a dry-stimulus WAV through the parameterized BA283 netlist via
ngspice and saves the raw ``(t, v)`` wet target as a compressed ``.npz``.

The dry audio is injected as a PWL voltage source on node ``INPUT`` (which
``data_gen.netlist`` leaves open for exactly this purpose). The output tap is
read from node ``SEC_IN`` (the 600 Ohm-terminated transformer secondary).

The XSPICE nonlinear-magnetics transformer has no DC operating point, so the
simulation runs a ``uic`` transient only (no ``.op``).

For long stimuli (> 30 s) the PWL points are written to a sidecar ``.pwl``
file and referenced via ngspice ``PWL file="..."`` syntax; for short stimuli
the points are inlined directly into the netlist. Only the inline path is
exercised by the Phase 1 tests and gate smoke.
"""

from __future__ import annotations

import argparse
import csv
import multiprocessing as mp
import os
from pathlib import Path

import numpy as np
import soundfile as sf

from data_gen.netlist import build_netlist

SAMPLE_RATE = 96_000
# Internal transient oversample factor (step = 1 / (SR * OVERSAMPLE)).
OVERSAMPLE = 8
# Stimuli longer than this (seconds) use a sidecar PWL file instead of inline.
SIDECAR_THRESHOLD_S = 30.0
# Tail appended after the stimulus so the transient settles past the input.
TAIL_S = 0.05

DATA_DIR = Path("data")
WET_RAW_DIR = DATA_DIR / "wet_raw"
SPICE_LOG_DIR = DATA_DIR / "spice_logs"
FAILED_CSV = DATA_DIR / "failed.csv"


def _load_dry(dry_wav: Path) -> np.ndarray:
    """Load a 96 kHz WAV, downmix to mono float32, return the samples."""
    samples, sr = sf.read(str(dry_wav), dtype="float32", always_2d=False)
    if sr != SAMPLE_RATE:
        raise ValueError(f"{dry_wav}: expected SR {SAMPLE_RATE}, got {sr}")
    if samples.ndim > 1:
        samples = samples.mean(axis=1).astype(np.float32)
    return np.ascontiguousarray(samples, dtype=np.float32)


def _inline_pwl(samples: np.ndarray) -> str:
    """Render an inline ``PWL(t0 v0 t1 v1 ...)`` body at the 96 kHz grid."""
    t = np.arange(samples.shape[0]) / SAMPLE_RATE
    parts = [f"{ti:.9g} {vi:.9g}" for ti, vi in zip(t, samples, strict=True)]
    return "PWL(" + " ".join(parts) + ")"


def _write_pwl_file(samples: np.ndarray, pwl_path: Path) -> None:
    """Write a sidecar PWL file (``t v`` per line) for long stimuli."""
    t = np.arange(samples.shape[0]) / SAMPLE_RATE
    with pwl_path.open("w") as fh:
        for ti, vi in zip(t, samples, strict=True):
            fh.write(f"{ti:.9g} {vi:.9g}\n")


def simulate_one(dry_wav: Path, bias: float, out_npz: Path) -> Path:
    """Simulate one (dry stimulus, bias) pair and save the wet target.

    Parameters
    ----------
    dry_wav:
        Path to a 96 kHz dry-stimulus WAV (mono or multi-channel).
    bias:
        RV1 wiper position in ``[0.0, 1.0]`` passed to ``build_netlist``.
    out_npz:
        Destination ``.npz`` path; receives arrays ``t`` and ``v``.

    Returns
    -------
    ``Path`` to the written ``.npz`` (equal to ``out_npz``).
    """
    dry_wav = Path(dry_wav)
    out_npz = Path(out_npz)

    samples = _load_dry(dry_wav)
    duration = samples.shape[0] / SAMPLE_RATE

    circuit = build_netlist(bias)

    # Inject the dry audio as a PWL voltage source on node INPUT.
    if duration > SIDECAR_THRESHOLD_S:
        # Sidecar path for real >30 s stimuli; not exercised in Phase 1.
        # TODO: validate sidecar PWL against ngspice 46 in Task 1.5 gate run.
        pwl_path = out_npz.with_suffix(".pwl")
        _write_pwl_file(samples, pwl_path)
        circuit.raw_spice += f'Vin INPUT 0 PWL file="{pwl_path}"\n'
    else:
        circuit.raw_spice += f"Vin INPUT 0 {_inline_pwl(samples)}\n"

    sim = circuit.simulator()
    # method='gear' is accepted as an options kwarg by PySpice 1.5.
    sim.options(
        reltol=1e-4,
        abstol=1e-9,
        vntol=1e-6,
        gmin=1e-13,
        method="gear",
    )

    # uic transient only — the XSPICE transformer has no DC operating point.
    analysis = sim.transient(
        step_time=1 / (SAMPLE_RATE * OVERSAMPLE),
        end_time=duration + TAIL_S,
        use_initial_condition=True,
    )

    t = np.asarray(analysis.time)
    v = np.asarray(analysis["SEC_IN"])
    if t.shape != v.shape:
        raise RuntimeError(
            f"length mismatch: t{t.shape} vs v{v.shape} for {dry_wav}"
        )

    out_npz.parent.mkdir(parents=True, exist_ok=True)
    np.savez_compressed(out_npz, t=t, v=v)
    return out_npz


def _bias_tag(bias: float) -> str:
    return f"{bias:.2f}"


def _resolve_stimuli(manifest_or_stim_paths) -> list[Path]:
    """Resolve a manifest CSV, a directory, or a path iterable to WAV paths."""
    if isinstance(manifest_or_stim_paths, (str, Path)):
        p = Path(manifest_or_stim_paths)
        if p.is_dir():
            return sorted(p.glob("*.wav"))
        if p.suffix.lower() == ".csv":
            stims: list[Path] = []
            with p.open(newline="") as fh:
                for row in csv.DictReader(fh):
                    # Accept a 'stim' or 'path' column; fall back to first col.
                    val = row.get("stim") or row.get("path")
                    if val is None and row:
                        val = next(iter(row.values()))
                    if val:
                        stims.append(Path(val))
            return stims
        return [p]
    return [Path(x) for x in manifest_or_stim_paths]


def _run_pair(args: tuple[Path, float]) -> tuple[Path, float, str | None]:
    """Worker: run one pair, returning (stim, bias, error_or_None)."""
    stim, bias = args
    out = WET_RAW_DIR / f"{stim.stem}_bias{_bias_tag(bias)}.npz"
    log = SPICE_LOG_DIR / f"{stim.stem}_bias{_bias_tag(bias)}.log"
    try:
        simulate_one(stim, bias, out)
        return (stim, bias, None)
    except Exception as exc:
        SPICE_LOG_DIR.mkdir(parents=True, exist_ok=True)
        # Best-effort stderr capture: PySpice does not expose ngspice stderr
        # cleanly, so log the exception text.
        log.write_text(f"{type(exc).__name__}: {exc}\n")
        return (stim, bias, f"{type(exc).__name__}: {exc}")


def run_batch(manifest_or_stim_paths, biases, workers: int | None = None):
    """Run every (stimulus x bias) pair, writing wet targets to ``data/``.

    Failures for individual pairs are caught and appended to
    ``data/failed.csv`` (columns ``stim,bias,error``); the batch continues.
    """
    stimuli = _resolve_stimuli(manifest_or_stim_paths)
    pairs = [(stim, float(b)) for stim in stimuli for b in biases]

    WET_RAW_DIR.mkdir(parents=True, exist_ok=True)
    SPICE_LOG_DIR.mkdir(parents=True, exist_ok=True)

    if workers is None:
        workers = max(1, (os.cpu_count() or 2) - 2)

    if workers == 1:
        results = [_run_pair(p) for p in pairs]
    else:
        with mp.Pool(processes=workers) as pool:
            results = pool.map(_run_pair, pairs)

    failures = [(s, b, e) for (s, b, e) in results if e is not None]
    if failures:
        FAILED_CSV.parent.mkdir(parents=True, exist_ok=True)
        write_header = not FAILED_CSV.exists()
        with FAILED_CSV.open("a", newline="") as fh:
            writer = csv.writer(fh)
            if write_header:
                writer.writerow(["stim", "bias", "error"])
            for stim, bias, err in failures:
                writer.writerow([str(stim), _bias_tag(bias), err])

    return results


def _parse_biases(spec: str) -> list[float]:
    return [float(x) for x in spec.split(",") if x.strip()]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="BA283 SPICE batch runner.")
    parser.add_argument(
        "--manifest",
        help="CSV of stimuli, or a directory of stimulus WAVs.",
    )
    parser.add_argument(
        "--biases",
        default="0.0,0.25,0.5,0.75,1.0",
        help="Comma-separated bias values in [0,1].",
    )
    parser.add_argument(
        "--workers",
        type=int,
        default=max(1, (os.cpu_count() or 2) - 2),
        help="Parallel worker processes.",
    )
    parser.add_argument(
        "--smoke",
        action="store_true",
        help="Run a single fixture pair as a smoke test.",
    )
    parser.add_argument("--single", help="Single stimulus WAV to simulate.")
    parser.add_argument(
        "--bias",
        type=float,
        help="Bias for --single (in [0,1]).",
    )
    parser.add_argument("--out", help="Output .npz path for --single.")
    args = parser.parse_args(argv)

    if args.smoke:
        WET_RAW_DIR.mkdir(parents=True, exist_ok=True)
        out = simulate_one(
            Path("tests/fixtures/short_sine_96k.wav"),
            0.5,
            WET_RAW_DIR / "short_sine_96k_bias0.50.npz",
        )
        n = int(np.load(out)["t"].shape[0])
        print(f"smoke ok: {out} ({n} samples)")
        return 0

    if args.single:
        if args.bias is None or args.out is None:
            parser.error("--single requires --bias and --out")
        out = simulate_one(Path(args.single), args.bias, Path(args.out))
        n = int(np.load(out)["t"].shape[0])
        print(f"wrote {out} ({n} samples)")
        return 0

    if not args.manifest:
        parser.error("provide --manifest, --single, or --smoke")

    biases = _parse_biases(args.biases)
    results = run_batch(args.manifest, biases, workers=args.workers)
    failed = sum(1 for (_, _, e) in results if e is not None)
    print(f"batch done: {len(results)} pairs, {failed} failed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
