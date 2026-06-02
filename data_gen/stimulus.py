"""96 kHz dry-stimulus library for BA283 SPICE data generation.

Generator functions produce continuous ``np.float32`` arrays. The individual
generators honour a ``level_db`` (or ``peak_db``) peak target; the *block
builders* (:func:`build_synthetic_block`, :func:`load_user_music`) renormalise
to a peak of 0.5 V, which is the SPICE input scaling from the design doc.

Everything is 96 kHz, mono, float32. Pink noise is reproducible via the module
``SEED``. No chunking — arrays are built whole.
"""

from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np
import soundfile as sf
from scipy.signal import chirp, resample_poly

SAMPLE_RATE = 96_000
SEED = 0xBA283

_RNG = np.random.default_rng(SEED)


def _db_to_amp(db: float) -> float:
    """Convert a dBFS level to a linear amplitude."""
    return 10 ** (db / 20.0)


def _peak_normalize(x: np.ndarray, peak: float) -> np.ndarray:
    """Scale ``x`` so its peak absolute value equals ``peak`` (no-op if silent)."""
    m = np.max(np.abs(x))
    if m == 0:
        return x.astype(np.float32)
    return (x * (peak / m)).astype(np.float32)


def log_sine_sweep(
    duration_s: float, f_start: float, f_end: float, level_db: float
) -> np.ndarray:
    """Logarithmic sine sweep from ``f_start`` to ``f_end``, peak ``level_db``."""
    n = int(duration_s * SAMPLE_RATE)
    t = np.arange(n) / SAMPLE_RATE
    sweep = chirp(t, f0=f_start, f1=f_end, t1=duration_s, method="logarithmic")
    return _peak_normalize(sweep, _db_to_amp(level_db))


def _pink_noise(n: int) -> np.ndarray:
    """Pink noise via FFT colouring of white noise with a 1/sqrt(f) spectrum."""
    white = _RNG.standard_normal(n)
    spectrum = np.fft.rfft(white)
    freqs = np.fft.rfftfreq(n, d=1.0 / SAMPLE_RATE)
    scale = np.ones_like(freqs)
    scale[1:] = 1.0 / np.sqrt(freqs[1:])
    spectrum *= scale
    pink = np.fft.irfft(spectrum, n=n)
    return pink


def pink_noise_bursts(
    duration_s: float, burst_ms: float, gap_ms: float, peak_db: float
) -> np.ndarray:
    """Pink-noise bursts (``burst_ms`` on, ``gap_ms`` off) at peak ``peak_db``.

    Gap regions are set to exactly 0.0.
    """
    n = int(duration_s * SAMPLE_RATE)
    pink = _peak_normalize(_pink_noise(n), _db_to_amp(peak_db))

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


def load_user_music(paths: list[Path]) -> np.ndarray:
    """Load WAVs, resample to 96 kHz, peak-normalize each to 0.5, concatenate.

    Clips are joined with 50 ms of silence. Stereo is downmixed to mono.
    """
    silence = np.zeros(int(0.05 * SAMPLE_RATE), dtype=np.float32)
    clips: list[np.ndarray] = []
    for i, path in enumerate(paths):
        samples, sr = sf.read(str(path), dtype="float32")
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
    samples, sr = sf.read(str(path), dtype="float32")
    samples = _to_mono(samples)
    samples = _resample_to_96k(samples, sr)
    return samples.astype(np.float32), SAMPLE_RATE


def build_synthetic_block() -> np.ndarray:
    """Build the ~8 min synthetic stimulus block, peak-normalized to 0.5."""
    parts: list[np.ndarray] = []

    # 4 log sweeps 20 Hz -> 20 kHz, 30 s each, at -18/-12/-6/-1 dBFS.
    for level in (-18.0, -12.0, -6.0, -1.0):
        parts.append(log_sine_sweep(30.0, 20.0, 20_000.0, level))

    # 1 min pink-noise bursts (200 ms / 800 ms), levels around -24..-3 dBFS.
    burst_levels = (-24.0, -18.0, -12.0, -6.0, -3.0)
    seg = 60.0 / len(burst_levels)
    for level in burst_levels:
        parts.append(pink_noise_bursts(seg, 200, 800, level))

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

    block = np.concatenate(parts)
    return _peak_normalize(block, 0.5)


def main() -> None:
    """CLI: write the synthetic block (and optional music block) to ``--out``."""
    parser = argparse.ArgumentParser(description="Generate BA283 dry stimulus blocks.")
    parser.add_argument("--out", default="data/dry/", help="output directory")
    parser.add_argument(
        "--music-paths", nargs="*", default=None, help="optional WAV clips for a music block"
    )
    args = parser.parse_args()

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)

    synth = build_synthetic_block()
    sf.write(str(out / "synth_block.wav"), synth, SAMPLE_RATE, subtype="FLOAT")

    if args.music_paths:
        music = load_user_music([Path(p) for p in args.music_paths])
        sf.write(str(out / "music_block.wav"), music, SAMPLE_RATE, subtype="FLOAT")


if __name__ == "__main__":
    main()
