"""Offline spectral cross-check for the Ultra tier (F3-VCO V5).

Mirrors the Rust gate `plugin303/tests/ultra_spectral.rs`: compares the Welch-
averaged spectrum of an Ultra render against the whole-voice SPICE gold and
reports the gold-power-weighted log-spectral distance (LSD) + the dB-spectrum
correlation. The Ultra acceptance is a spectral *character* match, not sample-
exact ESR (which is structurally out of reach vs ngspice — see the design doc).

Usage:
  cargo run --bin render303 -p plugin303 -- --tier 2 --note 57 --sr 48000 \
      --secs 0.2 --cutoff 1002 --reso 85 --out /tmp/ultra.f32
  uv run --no-project --with numpy python eval/303/coherence.py \
      eval/303/gold/voice_A2_mid.f32 /tmp/ultra.f32
"""

from __future__ import annotations

import sys

import numpy as np

SEG = 1024
SR = 48_000.0
BAND = (40.0, 16_000.0)


def load(path: str) -> np.ndarray:
    return np.fromfile(path, dtype="<f4")


def welch_pow(x: np.ndarray) -> np.ndarray:
    """Hann, 50 % overlap, averaged power spectrum normalised to unit sum."""
    win = np.hanning(SEG)
    hop = SEG // 2
    nseg = max((len(x) - SEG) // hop + 1, 1)
    acc = np.zeros(SEG // 2)
    for s in range(0, nseg * hop, hop):
        seg = x[s:s + SEG]
        if len(seg) < SEG:
            break
        sp = np.fft.rfft(seg * win)[: SEG // 2]
        acc += np.abs(sp) ** 2
    acc /= nseg
    return acc / max(acc.sum(), 1e-300)


def spectral_match(gold: np.ndarray, model: np.ndarray) -> tuple[float, float]:
    gp, mp = welch_pow(gold), welch_pow(model)
    df = SR / SEG
    lo, hi = int(BAND[0] / df), int(BAND[1] / df)
    gp, mp = gp[lo:hi], mp[lo:hi]
    gdb = 10 * np.log10(np.maximum(gp, 1e-300))
    mdb = 10 * np.log10(np.maximum(mp, 1e-300))
    gdb -= gdb.mean()
    mdb -= mdb.mean()
    lsd = float(np.sqrt(np.sum(gp * (gdb - mdb) ** 2) / max(gp.sum(), 1e-300)))
    corr = float(np.corrcoef(gdb, mdb)[0, 1])
    return lsd, corr


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    gold = load(sys.argv[1])
    model = load(sys.argv[2])
    n = min(len(gold), len(model))
    lsd, corr = spectral_match(gold[:n], model[:n])
    print(f"LSD = {lsd:.2f} dB (weighted)   corr = {corr:.3f}")
    ok = lsd <= 5.5 and corr >= 0.95
    print("RESULT:", "MATCH" if ok else "OFF")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
