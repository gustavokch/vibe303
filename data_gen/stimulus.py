"""96 kHz dry-stimulus library for BA283 SPICE data generation.

Generator functions produce continuous ``np.float32`` arrays. The individual
generators honour a ``level_db`` (or ``peak_db``) peak target; the *block
builders* (:func:`build_synthetic_block`, :func:`load_user_music`) renormalise
to a peak of 0.5 V, which is the SPICE input scaling from the design doc.
:func:`build_synthetic_segments` instead emits one file per stimulus segment,
each scaled to ``0.5 * 10**(level_db/20)`` so relative drive levels are
preserved (level-dependent nonlinearity coverage) while staying at/under 0.5 V.

Everything is 96 kHz, mono, float32. Pink noise is reproducible per-call via
the ``seed`` argument (defaulting to the module ``SEED`` constant), so a bare
call is byte-for-byte reproducible regardless of call order. The block builders
separate their parts with 50 ms of silence to avoid seam transients (clicks
from abutting tone segments at arbitrary phase). No chunking — arrays are built
whole.
"""

from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np
import soundfile as sf
from scipy.signal import chirp, resample_poly

SAMPLE_RATE = 96_000
SEED = 0xBA283


def _db_to_amp(db: float) -> float:
    """Convert a dBFS level to a linear amplitude."""
    return 10 ** (db / 20.0)


def _peak_normalize(x: np.ndarray, peak: float) -> np.ndarray:
    """Scale ``x`` so its peak absolute value equals ``peak`` (no-op if silent)."""
    m = np.max(np.abs(x))
    if m == 0:
        return x.astype(np.float32)
    return (x * (peak / m)).astype(np.float32)


def _scale_to_dbfs(x: np.ndarray, level_db: float) -> np.ndarray:
    """Scale x so its peak == 0.5 * 10**(level_db/20) (0 dBFS == 0.5 V SPICE input max)."""
    peak = np.max(np.abs(x))
    if peak == 0:
        return x.astype(np.float32)
    target = 0.5 * (10 ** (level_db / 20.0))
    return (x * (target / peak)).astype(np.float32)


def log_sine_sweep(
    duration_s: float, f_start: float, f_end: float, level_db: float
) -> np.ndarray:
    """Logarithmic sine sweep from ``f_start`` to ``f_end``, peak ``level_db``."""
    n = int(duration_s * SAMPLE_RATE)
    t = np.arange(n) / SAMPLE_RATE
    sweep = chirp(t, f0=f_start, f1=f_end, t1=duration_s, method="logarithmic")
    return _peak_normalize(sweep, _db_to_amp(level_db))


def _pink_noise(n: int, seed: int | None = None) -> np.ndarray:
    """Pink noise via FFT colouring of white noise with a 1/sqrt(f) spectrum.

    Reproducible per-call: ``seed`` defaults to the module ``SEED`` constant.
    """
    rng = np.random.default_rng(seed if seed is not None else SEED)
    white = rng.standard_normal(n)
    spectrum = np.fft.rfft(white)
    freqs = np.fft.rfftfreq(n, d=1.0 / SAMPLE_RATE)
    scale = np.ones_like(freqs)
    scale[1:] = 1.0 / np.sqrt(freqs[1:])
    spectrum *= scale
    pink = np.fft.irfft(spectrum, n=n)
    return pink


def pink_noise_bursts(
    duration_s: float,
    burst_ms: float,
    gap_ms: float,
    peak_db: float,
    seed: int | None = None,
) -> np.ndarray:
    """Pink-noise bursts (``burst_ms`` on, ``gap_ms`` off) at peak ``peak_db``.

    Gap regions are set to exactly 0.0. Reproducible per-call: ``seed`` defaults
    to the module ``SEED`` constant, so a bare call is byte-for-byte
    reproducible regardless of call order.
    """
    n = int(duration_s * SAMPLE_RATE)
    pink = _peak_normalize(_pink_noise(n, seed), _db_to_amp(peak_db))

    burst_n = round(burst_ms * 1e-3 * SAMPLE_RATE)
    gap_n = round(gap_ms * 1e-3 * SAMPLE_RATE)
    period = burst_n + gap_n

    envelope = np.zeros(n, dtype=np.float32)
    if period > 0:
        for start in range(0, n, period):
            envelope[start : min(start + burst_n, n)] = 1.0

    return (pink * envelope).astype(np.float32)


def two_tone_imd(
    duration_s: float, f1: float, f2: float, level_db: float
) -> np.ndarray:
    """Sum of two equal-amplitude sines at ``f1``/``f2``, combined peak ``level_db``."""
    n = int(duration_s * SAMPLE_RATE)
    t = np.arange(n) / SAMPLE_RATE
    sig = np.sin(2 * np.pi * f1 * t) + np.sin(2 * np.pi * f2 * t)
    return _peak_normalize(sig, _db_to_amp(level_db))


def multisine_schroeder(
    duration_s: float, num_tones: int, f_low: float, f_high: float
) -> np.ndarray:
    """Schroeder-phase multisine spanning ``f_low``..``f_high`` with a low crest factor.

    Tones are equally spaced — the Schroeder phase formula minimises crest factor
    for equally-spaced (harmonic-like) tone sets, so log spacing would push the
    crest above the design target. The result is a flat, dense multisine across
    the band, which is what data-gen needs.
    """
    n = int(duration_s * SAMPLE_RATE)
    t = np.arange(n) / SAMPLE_RATE
    freqs = np.linspace(f_low, f_high, num_tones)
    sig = np.zeros(n)
    for i, f in enumerate(freqs):
        k = i + 1
        phi = -k * (k - 1) * np.pi / num_tones
        sig += np.sin(2 * np.pi * f * t + phi)
    return _peak_normalize(sig, 1.0)


def filtered_impulses(duration_s: float) -> np.ndarray:
    """~10 discrete impulses plus a few step responses, spread deterministically."""
    n = int(duration_s * SAMPLE_RATE)
    sig = np.zeros(n, dtype=np.float32)

    num_impulses = 10
    for i in range(num_impulses):
        pos = int((i + 0.5) / num_impulses * n)
        if pos < n:
            sig[pos] = 1.0

    num_steps = 4
    step_len = int(0.02 * SAMPLE_RATE)  # 20 ms steps
    for i in range(num_steps):
        pos = int((i + 0.5) / num_steps * n)
        end = min(pos + step_len, n)
        sig[pos:end] = 0.5

    return sig.astype(np.float32)


def _to_mono(samples: np.ndarray) -> np.ndarray:
    """Downmix to mono by averaging channels if needed."""
    if samples.ndim > 1:
        return samples.mean(axis=1)
    return samples


def _resample_to_96k(samples: np.ndarray, sr: int) -> np.ndarray:
    """Resample ``samples`` from ``sr`` to 96 kHz via polyphase filtering."""
    if sr == SAMPLE_RATE:
        return samples
    g = np.gcd(int(sr), SAMPLE_RATE)
    up = SAMPLE_RATE // g
    down = int(sr) // g
    return resample_poly(samples, up, down)


def _read_wav(path: Path) -> tuple[np.ndarray, int]:
    """Read a WAV as float32, raising with the offending path on failure."""
    if not Path(path).is_file():
        raise FileNotFoundError(f"audio file not found: {path}")
    try:
        return sf.read(str(path), dtype="float32")
    except Exception as exc:  # re-raise with path context
        raise RuntimeError(f"failed to read audio file: {path}") from exc


def load_user_music(paths: list[Path]) -> np.ndarray:
    """Load WAVs, resample to 96 kHz, peak-normalize each to 0.5, concatenate.

    Clips are joined with 50 ms of silence. Stereo is downmixed to mono.
    """
    silence = np.zeros(int(0.05 * SAMPLE_RATE), dtype=np.float32)
    clips: list[np.ndarray] = []
    for i, path in enumerate(paths):
        samples, sr = _read_wav(path)
        samples = _to_mono(samples)
        samples = _resample_to_96k(samples, sr)
        samples = _peak_normalize(samples, 0.5)
        if i > 0:
            clips.append(silence)
        clips.append(samples)
    if not clips:
        return np.zeros(0, dtype=np.float32)
    return np.concatenate(clips).astype(np.float32)


def load_dry(path: Path) -> tuple[np.ndarray, int]:
    """Load a WAV as mono float32 at 96 kHz; resample if needed."""
    samples, sr = _read_wav(path)
    samples = _to_mono(samples)
    samples = _resample_to_96k(samples, sr)
    return samples.astype(np.float32), SAMPLE_RATE


def build_synthetic_block() -> np.ndarray:
    """Build the ~8 min synthetic stimulus block, peak-normalized to 0.5.

    Parts are separated by 50 ms of silence (the same convention as
    :func:`load_user_music`) so abutting tone segments at arbitrary phase do not
    inject broadband click transients into the SPICE training targets. Silence
    is benign for SPICE — the netlist settles during the gap.
    """
    parts: list[np.ndarray] = []

    # 4 log sweeps 20 Hz -> 20 kHz, 30 s each, at -18/-12/-6/-1 dBFS.
    for level in (-18.0, -12.0, -6.0, -1.0):
        parts.append(log_sine_sweep(30.0, 20.0, 20_000.0, level))

    # 1 min pink-noise bursts (200 ms / 800 ms), levels around -24..-3 dBFS.
    # Distinct explicit seeds keep each segment different yet reproducible.
    burst_levels = (-24.0, -18.0, -12.0, -6.0, -3.0)
    seg = 60.0 / len(burst_levels)
    for i, level in enumerate(burst_levels):
        parts.append(pink_noise_bursts(seg, 200, 800, level, seed=SEED + i))

    # 1 min two-tone IMD at musical-interval pairs.
    pairs = (
        (200.0, 300.0),   # perfect fifth (3:2)
        (220.0, 277.18),  # major third
        (440.0, 660.0),   # perfect fifth
        (500.0, 750.0),   # perfect fifth
    )
    seg = 60.0 / len(pairs)
    for f1, f2 in pairs:
        parts.append(two_tone_imd(seg, f1, f2, -6.0))

    # 1 min filtered impulses.
    parts.append(filtered_impulses(60.0))

    # 3 min Schroeder multisine.
    parts.append(multisine_schroeder(180.0, 40, 20.0, 20_000.0))

    silence = np.zeros(int(0.05 * SAMPLE_RATE), dtype=np.float32)
    joined: list[np.ndarray] = []
    for i, part in enumerate(parts):
        if i > 0:
            joined.append(silence)
        joined.append(part)

    block = np.concatenate(joined)
    return _peak_normalize(block, 0.5)


def build_synthetic_segments(duration_scale: float = 1.0) -> dict[str, np.ndarray]:
    """Build per-segment dry stimuli as ``{name: float32_array}``.

    Each segment is its own file (so the leakage-free-by-file manifest split has
    enough distinct files for a representative train/val/test split) and is
    scaled via :func:`_scale_to_dbfs` to a peak of ``0.5 * 10**(level_db/20)``.
    This preserves *relative* drive levels across segments (so the dataset
    covers the circuit's level-dependent nonlinearity) while keeping every
    segment under the 0.5 V SPICE input ceiling. All 96 kHz mono float32.

    Because each segment is a separate file, the inter-segment seam-click
    problem addressed by :func:`build_synthetic_block` does not apply here.

    ``duration_scale`` multiplies every segment's base duration. Use < 1.0 to
    shrink the dataset for a tractable SPICE batch (the SPICE transient runs at
    ~tens of x realtime and PySpice holds the whole transient in RAM, so full
    base durations make the batch many hours / many GB). The sweeps still cover
    the full 20 Hz-20 kHz range regardless of scale (the chirp spans whatever
    duration it is given), so shortening trades sweep *resolution*, not range.
    """
    sweep_s = 30.0 * duration_scale
    short_s = 20.0 * duration_scale
    multisine_s = 60.0 * duration_scale
    return {
        "sweep_m18": _scale_to_dbfs(
            log_sine_sweep(sweep_s, 20.0, 20_000.0, 0.0), -18.0
        ),
        "sweep_m12": _scale_to_dbfs(
            log_sine_sweep(sweep_s, 20.0, 20_000.0, 0.0), -12.0
        ),
        "sweep_m6": _scale_to_dbfs(
            log_sine_sweep(sweep_s, 20.0, 20_000.0, 0.0), -6.0
        ),
        "sweep_m1": _scale_to_dbfs(
            log_sine_sweep(sweep_s, 20.0, 20_000.0, 0.0), -1.0
        ),
        "pink_m18": _scale_to_dbfs(
            pink_noise_bursts(short_s, 200, 800, -18.0, seed=SEED), -18.0
        ),
        "pink_m6": _scale_to_dbfs(
            pink_noise_bursts(short_s, 200, 800, -6.0, seed=SEED + 1), -6.0
        ),
        "twotone_m12": _scale_to_dbfs(
            two_tone_imd(short_s, 220.0, 277.18, -12.0), -12.0
        ),
        "twotone_m3": _scale_to_dbfs(
            two_tone_imd(short_s, 330.0, 495.0, -3.0), -3.0
        ),
        "impulse": _scale_to_dbfs(filtered_impulses(short_s), -6.0),
        "multisine_m12": _scale_to_dbfs(
            multisine_schroeder(multisine_s, 40, 20.0, 20_000.0), -12.0
        ),
        "multisine_m3": _scale_to_dbfs(
            multisine_schroeder(multisine_s, 40, 20.0, 20_000.0), -3.0
        ),
    }


# STEPPED (WS4) -------------------------------------------------------------
# Input-amplitude sweep used at EACH of the 5 hardware gain positions, so the
# stepped retrain sees the full input range that the plugin's input_gain x drive
# can produce. The grid spans -24..+48 dB relative to the 0 dBFS == 0.5 V SPICE
# convention (so +48 dB peaks at 0.5 * 10**(48/20) ~ 126 V at the netlist input,
# far into clipping — intentional: the model must cover hot drive, level-norm
# pulls it back to unity). Clips are kept SHORT (default 8 s) because run_spice
# holds the whole transient in RAM and ngspice has a ~2.1 GB output-buffer wall
# (hot/clipping points densify the gear timestep); os=1 (OVERSAMPLE=2) is the
# run_spice default. Coarser dB steps -> fewer points -> a tractable batch.
STEPPED_AMP_DB = (-24.0, -12.0, -6.0, 0.0, 6.0, 12.0, 24.0, 36.0, 48.0)


def _amplitude_base(duration_s: float) -> np.ndarray:
    """Unit-peak (1.0) broadband base for the amplitude sweep.

    A short log sweep (covers the whole band) followed by a two-tone IMD burst
    (intermod / asymmetry content) so each amplitude point exercises both the
    frequency response and the saturation character. Peak-normalized to 1.0 so
    the per-amplitude scaling below sets the exact 0.5 * 10**(db/20) input peak.
    """
    sweep = log_sine_sweep(duration_s * 0.6, 20.0, 20_000.0, 0.0)
    imd = two_tone_imd(duration_s * 0.4, 220.0, 277.18, 0.0)
    silence = np.zeros(int(0.05 * SAMPLE_RATE), dtype=np.float32)
    base = np.concatenate([sweep, silence, imd])
    return _peak_normalize(base, 1.0)


def build_amplitude_sweep(
    amps_db: tuple[float, ...] = STEPPED_AMP_DB, duration_s: float = 8.0
) -> dict[str, np.ndarray]:
    """Per-amplitude dry stimuli as ``{name: float32_array}`` (WS4 stepped set).

    Each entry is the same broadband base scaled to a peak of
    ``0.5 * 10**(db/20)`` (the 0 dBFS == 0.5 V convention). Names are
    ``ampsweep_p06`` / ``ampsweep_m24`` etc. (``p``/``m`` = plus/minus the dB).
    These dry files are then run through SPICE at EACH of the 5 R_KJ positions by
    ``data_gen.run_spice.run_stepped_batch``; the wet target is normalized by the
    measured per-position G_i (see that runner).

    NOTE: the peak is set by ``_scale_to_dbfs`` against ``level_db = db``, so the
    scaling uses the SAME 0.5 V reference as the rest of the library. There is no
    clipping in the dry signal itself — clipping happens inside the netlist at the
    hot (positive-dB) points, which is exactly the saturation we want to learn.
    """
    base = _amplitude_base(duration_s)
    out: dict[str, np.ndarray] = {}
    for db in amps_db:
        sign = "p" if db >= 0 else "m"
        tag = f"ampsweep_{sign}{abs(int(round(db))):02d}"
        out[tag] = _scale_to_dbfs(base, db)
    return out


def main() -> None:
    """CLI: write per-segment dry stimulus files (and optional music) to ``--out``."""
    parser = argparse.ArgumentParser(description="Generate BA283 dry stimulus files.")
    parser.add_argument("--out", default="data/dry/", help="output directory")
    parser.add_argument(
        "--music-paths",
        nargs="*",
        default=None,
        help="optional WAV clips, each written as its own dry file",
    )
    parser.add_argument(
        "--overwrite", action="store_true", help="overwrite existing output files"
    )
    parser.add_argument(
        "--duration-scale",
        type=float,
        default=1.0,
        help="multiply every segment's base duration (use <1 for a faster/smaller SPICE batch)",
    )
    # STEPPED (WS4): emit the input-amplitude sweep set instead of the default
    # per-segment library. These dry files feed run_spice.run_stepped_batch.
    parser.add_argument(
        "--amplitude-sweep",
        action="store_true",
        help="WS4: write the input-amplitude sweep dry set (covers -24..+48 dB) "
        "for the stepped-gain retrain instead of the default segment library",
    )
    parser.add_argument(
        "--amp-duration-s",
        type=float,
        default=8.0,
        help="WS4 amplitude-sweep clip length (keep short: ngspice ~2.1 GB buffer)",
    )
    args = parser.parse_args()

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)

    def _write(path: Path, sig: np.ndarray) -> bool:
        """Write ``sig`` to ``path`` honouring the clobber guard; True if written."""
        if path.exists() and not args.overwrite:
            print(f"skipping {path}: already exists (use --overwrite)")
            return False
        sf.write(str(path), sig, SAMPLE_RATE, subtype="FLOAT")
        return True

    written = 0
    # STEPPED (WS4): the amplitude-sweep set is an alternative dry library.
    if args.amplitude_sweep:
        for name, sig in build_amplitude_sweep(duration_s=args.amp_duration_s).items():
            if _write(out / f"{name}.wav", sig):
                written += 1
        print(f"wrote {written} amplitude-sweep dry file(s) to {out}")
        return
    for name, sig in build_synthetic_segments(args.duration_scale).items():
        if _write(out / f"{name}.wav", sig):
            written += 1

    if args.music_paths:
        for i, p in enumerate(args.music_paths):
            mp = Path(p)
            samples, _ = load_dry(mp)
            samples = _scale_to_dbfs(samples, -6.0)
            if _write(out / f"music_{i}_{mp.stem}.wav", samples):
                written += 1

    print(f"wrote {written} dry stimulus file(s) to {out}")


if __name__ == "__main__":
    main()
