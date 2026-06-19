"""SPICE execution runner for BA283 wet-target generation.

Pushes a dry-stimulus WAV through the parameterized BA283 netlist via
ngspice and saves the raw ``(t, v)`` wet target as a compressed ``.npz``.

The dry audio is injected via an XSPICE ``filesource`` code-model a-device on
node ``INPUT`` (which ``data_gen.netlist`` leaves open for exactly this
purpose). The output tap is read from node ``SEC_IN`` (the 600 Ohm-terminated
transformer secondary).

The XSPICE nonlinear-magnetics transformer has no DC operating point, so the
simulation runs a ``uic`` transient only (no ``.op``).

Every sim — regardless of duration — writes a sidecar ``time value`` ASCII
file (one line per 96 kHz sample) and injects it via the XSPICE ``filesource``
code model. The ngspice ``Vin INPUT 0 PWL file="..."`` sidecar syntax does
NOT work in ngspice 46 (``Cannot compute substitute``), so it is not used.
``filesource`` is the single, verified-working injection path.
"""

from __future__ import annotations

import argparse
import csv
import logging
import multiprocessing as mp
import os
from pathlib import Path

import numpy as np
import soundfile as sf

from data_gen.netlist import (
    BIAS_NORM_FIXED,
    GAIN_RKJ_OHMS,
    STEPPED_MAKEUP,
    STEPPED_RKJ_OHMS,
    build_netlist,
)

# ngspice-46 is our validated target; PySpice 1.5's version table predates it
# and emits a spurious "Unsupported Ngspice version 46" warning on every sim.
# Silence it.
logging.getLogger("PySpice.Spice.NgSpice.Shared").setLevel(logging.ERROR)

SAMPLE_RATE = 96_000
# Internal transient max-step factor (max_step = 1 / (SR * OVERSAMPLE)).
# This only CAPS ngspice's adaptive (gear) timestep; solution accuracy is
# governed by reltol/abstol, which still refine fast transients. A high cap
# (8) emitted ~8 output points/sample held entirely in RAM by PySpice's
# shared-lib mode, blowing past memory on multi-minute clips (a 60 s clip at
# 8x needed ~7.4 GB). Lowered to 2 -> ~4x less memory + time with negligible
# accuracy loss (PCHIP resample handles the variable output grid). 2x also
# matches the plugin's own internal 2x oversampling.
OVERSAMPLE = 2
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


def _write_filesource(samples: np.ndarray, path: Path) -> None:
    """Write a ``time value`` ASCII file for the XSPICE ``filesource`` model.

    One line per 96 kHz sample with ``time = i / 96000``. ``%.12g`` for time
    avoids the timestamp-collision cliff that ``%.9g`` hits on long stimuli.
    Written with ``np.savetxt`` so multi-minute (millions-of-line) files stay
    fast and memory-friendly.
    """
    t = np.arange(samples.shape[0], dtype=np.float64) / SAMPLE_RATE
    data = np.column_stack((t, samples.astype(np.float64)))
    np.savetxt(path, data, fmt=("%.12g", "%.8g"))


def simulate_one(
    dry_wav: Path, bias: float, out_npz: Path, gain_rkj: float = GAIN_RKJ_OHMS
) -> Path:
    """Simulate one (dry stimulus, bias, gain) point and save the wet target.

    Parameters
    ----------
    dry_wav:
        Path to a 96 kHz dry-stimulus WAV (mono or multi-channel).
    bias:
        RV1 wiper position in ``[0.0, 1.0]`` passed to ``build_netlist``.
    out_npz:
        Destination ``.npz`` path; receives arrays ``t`` and ``v``.
    gain_rkj:
        K<->J gain-shunt resistor R_KJ in ohms (default = 26 dB tap). For the
        gain-conditioned dataset this is swept while ``bias`` is held fixed.

    Returns
    -------
    ``Path`` to the written ``.npz`` (equal to ``out_npz``).
    """
    dry_wav = Path(dry_wav)
    out_npz = Path(out_npz)

    samples = _load_dry(dry_wav)
    duration = samples.shape[0] / SAMPLE_RATE

    circuit = build_netlist(bias, gain_rkj)

    # Inject the dry audio via the XSPICE filesource code model on node INPUT.
    # A single path for every duration: the 'Vin INPUT 0 PWL file=' sidecar
    # syntax does not work in ngspice 46, and inline PWL does not scale to the
    # multi-minute production stimuli. filesource interpolates from a
    # 'time value' ASCII file (absolute path; ngspice's file search needs it).
    sidecar = out_npz.with_suffix(".src")
    _write_filesource(samples, sidecar)
    src_path = str(Path(sidecar).resolve())
    circuit.raw_spice += (
        "a_in %vd([INPUT 0]) inputsrc\n"
        f'.model inputsrc filesource (file="{src_path}" amploffset=[0] '
        "amplscale=[1] timeoffset=0 timescale=1 timerelative=false "
        "amplstep=false)\n"
    )

    # Restrict the transient output buffer to the single tapped node. ngspice
    # otherwise stores every node voltage + branch current (~46 vectors) in its
    # in-RAM output buffer, which at 60 s / 2x oversample overflows ngspice's
    # ~2 GB output-memory ceiling on high-gain points (hard clipping drives the
    # gear integrator to dense adaptive timesteps -> the accumulated buffer
    # crosses the 2 GB wall and "Setting the output memory is not possible").
    # simulate_one only ever reads SEC_IN, so saving just that node cuts the
    # buffer ~40x with zero effect on the solve or the saved target.
    circuit.raw_spice += ".save v(SEC_IN)\n"

    sim = circuit.simulator()
    # method='gear' is accepted as an options kwarg by PySpice 1.5.
    # Tolerances relaxed from the old XSPICE-core era (reltol 1e-4 / abstol 1e-9):
    # the behavioral transformer passes HF fully, so a sustained ~20 kHz sweep
    # drives TR3's nonlinear junction caps hard and the tight tolerances forced
    # the timestep toward 1e-17 (abort). reltol=1e-3 / abstol=1e-7 let the gear
    # integrator take sane steps through the HF tail while staying well within
    # audio accuracy (the null-vs-SPICE gate is -22 dB, ~5 effective bits).
    sim.options(
        reltol=1e-3,
        abstol=1e-7,
        vntol=1e-5,
        gmin=1e-12,
        method="gear",
        # .op convergence aids (gmin- and source-stepping, higher DC iter limit)
        # so the d.c.-coupled feedback amp finds its operating point at every bias.
        itl1=500,
        gminsteps=10,
        srcsteps=10,
    )

    # .op-initialized transient (use_initial_condition=False): the DC-feedback
    # bias loop (RV1 -> 56k -> TR1 base) now gives the circuit a convergent DC
    # operating point, so the transient starts at steady state. This avoids the
    # multi-second turn-on settling of the big electrolytics (80 µF gain/feedback
    # caps) that, under the old `uic`, dominated the output and starved TR2/TR1
    # toward cutoff (signal decayed to ~unity gain).
    analysis = sim.transient(
        step_time=1 / (SAMPLE_RATE * OVERSAMPLE),
        end_time=duration + TAIL_S,
        use_initial_condition=False,
    )

    t = np.asarray(analysis.time)
    v = np.asarray(analysis["SEC_IN"])
    if t.shape != v.shape:
        raise RuntimeError(
            f"length mismatch: t{t.shape} vs v{v.shape} for {dry_wav}"
        )

    out_npz.parent.mkdir(parents=True, exist_ok=True)
    np.savez_compressed(out_npz, t=t, v=v)
    # Drop the filesource sidecar once the result is saved — these are large
    # (~MB/s of audio) and would otherwise pile up across a full batch on a
    # disk-constrained box. (Left in place only if the sim raised before here.)
    sidecar.unlink(missing_ok=True)
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
        # maxtasksperchild=1: each sim runs in a fresh process so libngspice
        # (cached as a per-process singleton by PySpice) cannot accumulate or
        # leak state across sims over a long batch.
        with mp.Pool(processes=workers, maxtasksperchild=1) as pool:
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


def _gain_tag(gain_rkj: float) -> str:
    return f"{gain_rkj:g}"


def _run_gain_pair(
    args: tuple[Path, float, float],
) -> tuple[Path, float, str | None]:
    """Worker: run one (stim, gain) point at a fixed bias.

    Returns ``(stim, gain, error_or_None)``.
    """
    stim, gain, bias = args
    out = WET_RAW_DIR / f"{stim.stem}_gain{_gain_tag(gain)}.npz"
    log = SPICE_LOG_DIR / f"{stim.stem}_gain{_gain_tag(gain)}.log"
    try:
        simulate_one(stim, bias, out, gain_rkj=gain)
        return (stim, gain, None)
    except Exception as exc:
        SPICE_LOG_DIR.mkdir(parents=True, exist_ok=True)
        log.write_text(f"{type(exc).__name__}: {exc}\n")
        return (stim, gain, f"{type(exc).__name__}: {exc}")


def run_gain_batch(
    manifest_or_stim_paths, gains, bias: float, workers: int | None = None
):
    """Run every (stimulus x gain) point at a FIXED bias, writing wet targets.

    The gain-conditioned dataset: R_KJ is the swept conditioning knob and the
    bias is pinned to the real card's operating point (default 0.77 ~ 65 mA
    TR3 idle). Per-point failures are appended to ``data/failed.csv`` and the
    batch continues.
    """
    stimuli = _resolve_stimuli(manifest_or_stim_paths)
    points = [(stim, float(g), float(bias)) for stim in stimuli for g in gains]

    WET_RAW_DIR.mkdir(parents=True, exist_ok=True)
    SPICE_LOG_DIR.mkdir(parents=True, exist_ok=True)

    if workers is None:
        workers = max(1, (os.cpu_count() or 2) - 2)

    if workers == 1:
        results = [_run_gain_pair(p) for p in points]
    else:
        with mp.Pool(processes=workers, maxtasksperchild=1) as pool:
            results = pool.map(_run_gain_pair, points)

    failures = [(s, g, e) for (s, g, e) in results if e is not None]
    if failures:
        FAILED_CSV.parent.mkdir(parents=True, exist_ok=True)
        write_header = not FAILED_CSV.exists()
        with FAILED_CSV.open("a", newline="") as fh:
            writer = csv.writer(fh)
            if write_header:
                writer.writerow(["stim", "gain", "error"])
            for stim, gain, err in failures:
                writer.writerow([str(stim), _gain_tag(gain), err])

    return results


# STEPPED (WS4) -------------------------------------------------------------
# Small-signal probe used to MEASURE the per-position output scale G_i (the
# secondary-volts-per-input-volt small-signal gain). The wet targets are then
# normalized by G_i so every position lands at unity small-signal gain — exactly
# what the plugin's level-norm does — and the WDF + NN share the same reference.
# Probe is a low-level (well below clipping) 1 kHz tone so the measured gain is
# the linear operating-point gain, not a saturated one.
_GI_PROBE_HZ = 1_000.0
_GI_PROBE_PEAK_V = 0.005  # 0.005 V << 0.5 V ceiling -> stays in the linear region
_GI_PROBE_S = 0.05        # short: 50 ms is many cycles at 1 kHz, tiny RAM


def measure_gi(rkj: float, bias: float = BIAS_NORM_FIXED, sr: int = SAMPLE_RATE) -> float:
    """Measure the small-signal output scale G_i for one R_KJ position (volts/volt).

    Runs a low-level 1 kHz probe through the netlist at ``rkj`` and returns
    ``rms(SEC_IN) / rms(input)`` over the settled portion. This is the linear
    secondary gain the wet targets should be normalized by (``wet /= G_i``) so
    the stepped dataset is level-normalized per position. The makeup table
    ``1 / G_i`` is what the plugin/WDF multiplies back in.
    """
    n = int(_GI_PROBE_S * sr)
    t = np.arange(n, dtype=np.float64) / sr
    probe = (_GI_PROBE_PEAK_V * np.sin(2 * np.pi * _GI_PROBE_HZ * t)).astype(np.float32)

    circuit = build_netlist(bias, rkj)
    sidecar = WET_RAW_DIR / f"_gi_probe_rkj{_gain_tag(rkj)}.src"
    WET_RAW_DIR.mkdir(parents=True, exist_ok=True)
    _write_filesource(probe, sidecar)
    src_path = str(Path(sidecar).resolve())
    circuit.raw_spice += (
        "a_in %vd([INPUT 0]) inputsrc\n"
        f'.model inputsrc filesource (file="{src_path}" amploffset=[0] '
        "amplscale=[1] timeoffset=0 timescale=1 timerelative=false "
        "amplstep=false)\n"
    )
    circuit.raw_spice += ".save v(SEC_IN)\n"
    sim = circuit.simulator()
    sim.options(reltol=1e-3, abstol=1e-7, vntol=1e-5, gmin=1e-12, method="gear",
                itl1=500, gminsteps=10, srcsteps=10)
    analysis = sim.transient(
        step_time=1 / (sr * OVERSAMPLE), end_time=_GI_PROBE_S + TAIL_S,
        use_initial_condition=False,
    )
    sidecar.unlink(missing_ok=True)
    tt = np.asarray(analysis.time)
    vv = np.asarray(analysis["SEC_IN"])
    # Settle: drop the first half so DC/transient does not bias the RMS ratio.
    settle = tt >= (tt[0] + 0.5 * (tt[-1] - tt[0]))
    out_rms = float(np.sqrt(np.mean(vv[settle] ** 2)))
    in_rms = _GI_PROBE_PEAK_V / np.sqrt(2.0)  # rms of the known input tone
    return out_rms / max(in_rms, 1e-12)


def run_stepped_batch(
    manifest_or_stim_paths,
    bias: float = BIAS_NORM_FIXED,
    positions: tuple[float, ...] = STEPPED_RKJ_OHMS,
    workers: int | None = None,
    gi_out: Path = WET_RAW_DIR / "g_i.json",
):
    """WS4: run every (stimulus x stepped-position) point and RECORD per-pos G_i.

    The stepped-gain dataset: instead of the continuous gain_norm grid, the wet
    targets are rendered at EXACTLY the 5 hardware positions
    ``STEPPED_RKJ_OHMS`` (idx 0..4). For each position the small-signal G_i is
    measured (see :func:`measure_gi`) and written to ``gi_out`` as
    ``{idx, rkj, g_i, makeup=1/g_i, makeup_ref}`` so the WDF + NN share one
    reference. ``makeup_ref`` is the embedded WDF table (STEPPED_MAKEUP) for a
    drift check. This delegates the actual SPICE runs to :func:`run_gain_batch`
    (the wet WAVs are written raw; downstream resample/normalize divides by G_i).
    """
    import json

    g_rows = []
    for idx, rkj in enumerate(positions):
        g_i = measure_gi(rkj, bias=bias)
        g_rows.append(
            {
                "idx": idx,
                "rkj_ohms": float(rkj),
                "g_i": float(g_i),
                "makeup": float(1.0 / g_i) if g_i else None,
                # Drift check vs the embedded WDF makeup table 1/G_i (idx 0..4).
                "makeup_ref": float(STEPPED_MAKEUP[idx])
                if idx < len(STEPPED_MAKEUP)
                else None,
            }
        )
    gi_out.parent.mkdir(parents=True, exist_ok=True)
    gi_out.write_text(json.dumps(g_rows, indent=2))

    # Render the wet targets at the same positions (raw volts; the resample step
    # applies wet /= G_i using gi_out so the NN target is level-normalized).
    results = run_gain_batch(
        manifest_or_stim_paths, list(positions), bias, workers=workers
    )
    return results, g_rows


def _parse_biases(spec: str) -> list[float]:
    return [float(x) for x in spec.split(",") if x.strip()]


def _parse_gains(spec: str) -> list[float]:
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
        help="Comma-separated bias values in [0,1] (bias-sweep mode).",
    )
    parser.add_argument(
        "--gains",
        help="Comma-separated R_KJ ohms for a GAIN sweep at fixed bias "
        "(--fixed-bias). When set, sweeps gain instead of bias.",
    )
    parser.add_argument(
        "--fixed-bias",
        type=float,
        default=0.77,
        help="Bias held fixed during a --gains sweep (default 0.77 ~ 65 mA).",
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
    parser.add_argument(
        "--gain-rkj",
        type=float,
        default=GAIN_RKJ_OHMS,
        help="K<->J gain resistor R_KJ (ohms) for --single (default = 26 dB tap).",
    )
    parser.add_argument("--out", help="Output .npz path for --single.")
    # STEPPED (WS4): render the dataset at EXACTLY the 5 hardware positions
    # STEPPED_RKJ_OHMS and record per-position G_i to data/wet_raw/g_i.json.
    parser.add_argument(
        "--stepped",
        action="store_true",
        help="WS4: sweep the 5 hardware gain positions (R_KJ in "
        f"{list(STEPPED_RKJ_OHMS)}) at --fixed-bias and record per-pos G_i.",
    )
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
        out = simulate_one(
            Path(args.single), args.bias, Path(args.out), gain_rkj=args.gain_rkj
        )
        n = int(np.load(out)["t"].shape[0])
        print(f"wrote {out} ({n} samples)")
        return 0

    if not args.manifest:
        parser.error("provide --manifest, --single, or --smoke")

    if args.stepped:
        # WS4: 5-position render + G_i record. Wet WAVs are raw volts; the
        # resample step divides by the recorded G_i to level-normalize.
        results, g_rows = run_stepped_batch(
            args.manifest, bias=args.fixed_bias, workers=args.workers
        )
        failed = sum(1 for (_, _, e) in results if e is not None)
        print(
            f"stepped-batch done: {len(results)} points at "
            f"{len(STEPPED_RKJ_OHMS)} positions (bias={args.fixed_bias:g}), "
            f"{failed} failed. G_i:"
        )
        for r in g_rows:
            print(
                f"  idx {r['idx']} R_KJ={r['rkj_ohms']:g}Ω  G_i={r['g_i']:.5f}  "
                f"makeup={r['makeup']:.6f}  (WDF ref {r['makeup_ref']})"
            )
        return 0

    if args.gains:
        gains = _parse_gains(args.gains)
        results = run_gain_batch(
            args.manifest, gains, args.fixed_bias, workers=args.workers
        )
        failed = sum(1 for (_, _, e) in results if e is not None)
        print(
            f"gain-batch done: {len(results)} points "
            f"(bias={args.fixed_bias:g}), {failed} failed"
        )
        return 0

    biases = _parse_biases(args.biases)
    results = run_batch(args.manifest, biases, workers=args.workers)
    failed = sum(1 for (_, _, e) in results if e is not None)
    print(f"batch done: {len(results)} pairs, {failed} failed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
