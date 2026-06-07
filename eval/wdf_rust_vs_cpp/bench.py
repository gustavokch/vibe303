"""
Performance: Rust WDF port vs C++ original — wall-clock realtime-factor per tier.

Times {rust, cpp} × {realtime, fast} on one fixed dry WAV and reports the
realtime-factor (wall_seconds / audio_duration; < 1.0 = realtime-capable), the
fast/realtime speedup per language, the Rust/C++ ratio per tier, and the Newton
avgIters (per-iter solver-cost proxy) parsed from each binary.

Fairness:
  - Both binaries run os=2 (C++ ``--quality realtime|fast``; the Rust ``OS`` const)
    → 2 circuit solves per host sample on both sides.
  - Both are built at native opt: C++ via its Makefile (``-O3 -march=native``),
    Rust via ``RUSTFLAGS=-C target-cpu=native cargo build --release``.
  - Single binary invocation per timed run (min-of-N, with warmup runs discarded).
    Wall-clock includes process start + the model warmup + WAV I/O; on a multi-
    second file the per-sample solve dominates. Use a longer ``--stim`` to amortize
    further. No hyperfine dependency — uses ``time.perf_counter`` min-of-N.

Usage (under this repo's uv env):
    uv run python -m eval.wdf_rust_vs_cpp.bench \\
        --stim sweep_m6 --dry-dir data/dry --runs 5 \\
        --bias 0.77 --gain 0.5 --out eval/wdf_rust_vs_cpp/out
"""
from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

import soundfile as sf

from data_gen.netlist import gain_norm_to_rkj
from eval.wdf_vs_spice.run_wdf import WDF_DIR, BINARY as CPP_BIN, parse_newton as parse_cpp
from eval.wdf_rust_vs_cpp.run_rust_wdf import (
    REPO, BINARY as RUST_BIN, build_if_needed as build_rust, parse_newton as parse_rust,
)

TIERS = ("realtime", "fast")


def build_cpp_native() -> None:
    """Build ba283_sim via its Makefile (g++ -O3 -march=native) for a fair perf race."""
    subprocess.run(["make"], cwd=WDF_DIR, check=True, capture_output=True, text=True)


def time_min(argv: list[str], cwd: Path | None, runs: int, warmup: int) -> tuple[float, str, str]:
    """Run *argv* warmup+runs times; return (min_wall_s, last_stdout, last_stderr)."""
    best = float("inf")
    out = err = ""
    for k in range(warmup + runs):
        t0 = time.perf_counter()
        r = subprocess.run(argv, cwd=cwd, capture_output=True, text=True, timeout=600)
        dt = time.perf_counter() - t0
        if r.returncode != 0:
            raise RuntimeError(f"run failed (rc={r.returncode}):\n{r.stderr}")
        out, err = r.stdout, r.stderr
        if k >= warmup:
            best = min(best, dt)
    return best, out, err


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--stim", default="sweep_m6")
    ap.add_argument("--dry-dir", default="data/dry")
    ap.add_argument("--gain", type=float, default=0.5, help="gain_norm in [0,1]")
    ap.add_argument("--bias", type=float, default=0.77)
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--warmup", type=int, default=1)
    ap.add_argument("--out", default="eval/wdf_rust_vs_cpp/out")
    ap.add_argument("--no-build", action="store_true")
    args = ap.parse_args()

    dry_dir = (REPO / args.dry_dir) if not Path(args.dry_dir).is_absolute() else Path(args.dry_dir)
    out_dir = (REPO / args.out) if not Path(args.out).is_absolute() else Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    dry_path = None
    for cand in (args.stim, f"{args.stim}.wav", f"{args.stim}_dry.wav"):
        if (dry_dir / cand).exists():
            dry_path = dry_dir / cand
            break
    if dry_path is None:
        raise FileNotFoundError(f"no dry WAV for {args.stim!r} in {dry_dir}")

    info = sf.info(str(dry_path))
    audio_dur = info.frames / info.samplerate
    rgain = gain_norm_to_rkj(args.gain)

    if not args.no_build:
        print("[bench] building C++ (make, -O3 -march=native) ...")
        build_cpp_native()
        print("[bench] building Rust (cargo --release, target-cpu=native) ...")
        build_rust()

    results: dict[str, dict] = {}

    # --- C++ runs (hardcoded input.wav/output.wav in cwd → isolate in a temp dir) ---
    with tempfile.TemporaryDirectory(prefix="bench_cpp_") as _tmp:
        tmp = Path(_tmp)
        shutil.copy2(CPP_BIN, tmp / "ba283_sim")
        shutil.copy2(dry_path, tmp / "input.wav")
        for tier in TIERS:
            argv = [str(tmp / "ba283_sim"), str(rgain), str(args.bias), "--quality", tier]
            wall, out, _ = time_min(argv, tmp, args.runs, args.warmup)
            st = parse_cpp(out)
            results[f"cpp/{tier}"] = {"wall_s": wall, "rt_factor": wall / audio_dur, **st}

    # --- Rust runs (explicit --input/--output) ---
    with tempfile.TemporaryDirectory(prefix="bench_rust_") as _tmp:
        out_wav = Path(_tmp) / "out.wav"
        for tier in TIERS:
            argv = [
                str(RUST_BIN), "--backend", "wdf", "--tier", tier,
                "--circuit-gain", str(rgain), "--bias", str(args.bias),
                "--mix", "100", "--drive-db", "0", "--output-db", "0",
                "--input", str(dry_path), "--output", str(out_wav),
            ]
            wall, _, err = time_min(argv, None, args.runs, args.warmup)
            st = parse_rust(err)
            results[f"rust/{tier}"] = {"wall_s": wall, "rt_factor": wall / audio_dur, **st}

    summary = {
        "config": {"stim": args.stim, "audio_dur_s": audio_dur, "gain_norm": args.gain,
                   "rgain_ohm": rgain, "bias": args.bias, "runs": args.runs, "warmup": args.warmup,
                   "note": "both os=2, native opt, single-thread; wall incl. warmup+I/O"},
        "results": results,
        "fast_speedup": {lang: results[f"{lang}/realtime"]["rt_factor"] / results[f"{lang}/fast"]["rt_factor"]
                         for lang in ("rust", "cpp")},
        "rust_over_cpp": {tier: results[f"rust/{tier}"]["rt_factor"] / results[f"cpp/{tier}"]["rt_factor"]
                          for tier in TIERS},
    }
    (out_dir / "bench.json").write_text(json.dumps(summary, indent=2))

    print(f"\nstim={args.stim} ({audio_dur:.1f}s @ {info.samplerate} Hz), "
          f"gain_norm={args.gain} (R_KJ={rgain:.0f} Ω), min of {args.runs} runs\n")
    print(f"{'config':14s} {'wall(s)':>8s} {'rt-factor':>10s} {'avgIters':>9s} {'maxIters':>9s}")
    for key in ("cpp/realtime", "rust/realtime", "cpp/fast", "rust/fast"):
        r = results[key]
        print(f"{key:14s} {r['wall_s']:8.3f} {r['rt_factor']:10.4f} "
              f"{r.get('avg_iters', float('nan')):9.3f} {r.get('max_iters', -1):9d}")
    print("\nfast/realtime speedup:  " +
          "  ".join(f"{lang}={summary['fast_speedup'][lang]:.2f}x" for lang in ("rust", "cpp")))
    print("rust/cpp rt-factor:     " +
          "  ".join(f"{tier}={summary['rust_over_cpp'][tier]:.2f}x" for tier in TIERS))
    print(f"\nwrote {out_dir/'bench.json'}")


if __name__ == "__main__":
    main()
