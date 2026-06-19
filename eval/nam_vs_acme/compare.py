"""
Evaluate the trained NAM model vs the ACME.jl BA283 reference (and the SPICE
training target) on the model's own T3K sweep.

Context
-------
``model.nam`` (a NAM ``SlimmableContainer`` / WaveNet, no gain conditioning) was
trained by the ``neural-amp-modeler`` package on the T3K sweep
(``scratch/nam_sweep/ds48/T3K-60s_dry.wav`` -> SPICE-reamp target).  The model
ships no metadata sidecar, but the training set on disk pins the contract:

* input  = ``T3K-60s_dry.wav`` (48 kHz, the exact training input)
* target = ``T3K-60s_gain{G}.wav`` (SPICE reamp at gain_norm G, bias 0.77)
* the model matches **gain_norm 0.5** best (no conditioning -> one operating
  point); a latency of -24 samples and a DC/sub-bass bias offset must be removed
  before scoring (handled here).

So this is a **3-way** comparison at one operating point:
  NAM (candidate)  vs  ACME.jl (validated near-ground-truth)  vs  SPICE (truth).
ACME runs at its proven 96 kHz (the 48 kHz dry is soxr-upsampled, then the wet
is downsampled back); SPICE is the on-disk target.  All scoring is at 48 kHz.

The NAM output is not in volts; every ESR is **level-matched** (optimal LS
scalar) and lag-aligned (|lag| <= SPAN) after a 20 Hz high-pass.

Gain sweep (Task 6)
-------------------
With a gain-CONDITIONED ``.nam`` (``model.cond.nam``: 2-channel ``[audio,gain]``
WaveNet) the harness sweeps the 9-point gain grid.  At each gain it renders NAM
at that gain (the conditioning channel), runs ACME at the matching R_KJ, pulls
the per-gain SPICE target from the Task-2 ``ds48/`` reamp, and scores the same
3-way contract (lag-align / 20 Hz HP / level-match).  ``summary.json`` carries a
``per_gain`` ESR table and a worst-gain gate.

Usage:
    uv run --extra eval python -m eval.nam_vs_acme.compare \\
        [--gains 0.0,0.125,...,1.0] [--bias 0.77] [--max-seconds 30] \\
        [--out eval/nam_vs_acme/out]
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
import soundfile as sf
import soxr
from scipy.signal import butter, csd, sosfiltfilt, welch  # type: ignore[import-untyped]

from data_gen.netlist import gain_norm_to_rkj
from eval.wdf_vs_spice.compare import ONSET_SKIP, SPAN, best_lag
from eval.acme_vs_spice.run_acme import run_acme
from eval.nam_vs_acme.run_nam import NamModel

SR_ACME = 96_000
NAM_SWEEP = Path.home() / "Git/ba283-nam-lstm/scratch/nam_sweep/ds48"
DRY48 = NAM_SWEEP / "T3K-60s_dry.wav"
# Default to the gain-CONDITIONED model (Task 5); the legacy SlimmableContainer
# model.nam has no gain input and is scored single-point by an older run.
COND_NAM = Path.home() / "Git/ba283-nam-lstm/exported_models/model.cond.nam"
# The 9-point gain grid the conditioned model was trained on (ds48 reamp set).
DEFAULT_GAINS = "0.0,0.125,0.25,0.375,0.5,0.625,0.75,0.875,1.0"

# STEPPED (WS4) -------------------------------------------------------------
# The 5 hardware gain-switch detents, as R_KJ (ohms), idx 0..4 = 15/20/25/30/35
# dB. Mirrors data_gen.netlist.STEPPED_RKJ_OHMS / the Rust plugin table. The
# stepped mode scores at THESE positions instead of the continuous 9-gain sweep.
STEPPED_RKJ_OHMS = (1.0e9, 1200.0, 470.0, 220.0, 110.0)
# Hot-input-amplitude check: extra input gains (linear, applied to the dry) used
# to probe each position into clipping. 1.0 = the dry as-rendered; >1 drives
# harder. The stepped retrain must hold accuracy as input_gain x drive pushes the
# circuit hot, so these confirm the model tracks SPICE at the loud end too.
STEPPED_HOT_AMPS = (1.0, 4.0, 16.0)


# ---------------------------------------------------------------------------
# Resampling / alignment / metrics (at the 48 kHz comparison rate)
# ---------------------------------------------------------------------------


def _resample(x: np.ndarray, sr_from: int, sr_to: int) -> np.ndarray:
    if sr_from == sr_to:
        return np.asarray(x, dtype=np.float64)
    return soxr.resample(np.asarray(x, dtype=np.float64), sr_from, sr_to,
                         quality="VHQ").astype(np.float64)


def _hp(x: np.ndarray, sr: int, fc: float = 20.0) -> np.ndarray:
    """20 Hz high-pass: ACME/SPICE carry a DC/sub-bass bias-settling component
    the NAM (trained on AC) lacks; without removing it the ESR and level-fit are
    dominated by an offset none of the models is being judged on."""
    sos = butter(2, fc / (sr / 2), btype="high", output="sos")
    return sosfiltfilt(sos, x)


def _align(ref: np.ndarray, mod: np.ndarray, sr: int):
    """Lag-align (|lag| <= SPAN), trim the startup transient, DC/sub-bass HP."""
    lag = best_lag(ref, mod)
    if lag >= 0:
        r, m = ref[lag:], mod[: len(ref) - lag]
    else:
        r, m = ref[: len(ref) + lag], mod[-lag:]
    n = min(len(r), len(m))
    r, m = r[:n], m[:n]
    k = min(int(ONSET_SKIP * sr), n // 4)
    return _hp(r[k:], sr), _hp(m[k:], sr), lag


def metrics(ref: np.ndarray, mod: np.ndarray, sr: int) -> dict:
    """ESR / levels for *mod* against *ref*, raw and level-matched.

    The level-matched ESR fits ``g = <r,m>/<m,m>`` so the comparison is robust
    to *mod*'s scale; ``fit_gain`` is reported.
    """
    r, m, lag = _align(ref, mod, sr)
    err = r - m
    esr_raw = float(np.sum(err**2) / max(np.sum(r**2), 1e-20))
    g = float(np.dot(r, m) / max(np.dot(m, m), 1e-20))
    errm = r - g * m
    esr_m = float(np.sum(errm**2) / max(np.sum(r**2), 1e-20))
    return {
        "lag": lag,
        "esr_db": float(10.0 * np.log10(esr_raw + 1e-30)),
        "esr_matched_db": float(10.0 * np.log10(esr_m + 1e-30)),
        "fit_gain": g,
        "ref_rms": float(np.sqrt(np.mean(r**2))),
        "mod_rms": float(np.sqrt(np.mean(m**2))),
    }


# ---------------------------------------------------------------------------
# Welch FR + coherence
# ---------------------------------------------------------------------------


def _welch_tf(out: np.ndarray, inp: np.ndarray, sr: int, nbands: int = 28):
    n = min(len(out), len(inp))
    nper = min(16384, n)
    f, Pxx = welch(inp[:n], sr, nperseg=nper)
    _, Pyy = welch(out[:n], sr, nperseg=nper)
    _, Pxy = csd(inp[:n], out[:n], sr, nperseg=nper)
    H = np.abs(Pxy) / (Pxx + 1e-30)
    coh = (np.abs(Pxy) ** 2) / (Pxx * Pyy + 1e-30)
    edges = np.geomspace(20.0, sr / 2 * 0.98, nbands + 1)
    fcs, dbs, cohs = [], [], []
    for k in range(nbands):
        sel = (f >= edges[k]) & (f < edges[k + 1])
        if not sel.any():
            continue
        fcs.append(float(np.sqrt(edges[k] * edges[k + 1])))
        dbs.append(float(20.0 * np.log10(np.sqrt(np.mean(H[sel] ** 2)) + 1e-12)))
        cohs.append(float(np.mean(coh[sel])))
    return np.array(fcs), np.array(dbs), np.array(cohs)


# ---------------------------------------------------------------------------
# Plots (3-way: NAM vs ACME vs SPICE)
# ---------------------------------------------------------------------------


def _fr_plot(fcs, sdb, adb, ndb, nco, gn, rkj, bias, sr, out_dir, tag) -> dict:
    nyq = sr / 2
    band = (fcs >= 20) & (fcs <= 20_000)
    midband = (fcs >= 200) & (fcs <= 5_000)
    err = ndb - sdb  # NAM vs SPICE truth

    fig, axes = plt.subplots(2, 1, figsize=(10, 7), sharex=True)
    axes[0].semilogx(fcs, sdb, "o-", color="black", label="SPICE (truth)")
    axes[0].semilogx(fcs, adb, "^-", color="steelblue", label="ACME")
    axes[0].semilogx(fcs, ndb, "s--", color="darkorange", label="NAM (level-matched)")
    axes[0].set_ylabel("|H(f)| (dB)")
    axes[0].set_title(f"Frequency response (Welch) — T3K, "
                      f"gain_norm={gn} (R_KJ={rkj:.0f}Ω), bias={bias}")
    axes[0].legend()
    axes[0].grid(True, which="both", alpha=0.4)
    axes[1].semilogx(fcs, err, color="crimson", label="NAM − SPICE")
    axes[1].semilogx(fcs, adb - sdb, color="steelblue", lw=0.9, label="ACME − SPICE")
    axes[1].axhline(0, color="k", lw=0.8, ls="--")
    axes[1].fill_between([20, 20_000], -1, 1, color="green", alpha=0.08, label="±1 dB")
    axes[1].set_ylabel("Error (dB)")
    axes[1].set_xlabel("Frequency (Hz)")
    axes[1].set_ylim(-6, 6)
    axes[1].set_xlim(20, nyq)
    axes[1].legend(fontsize=8)
    axes[1].grid(True, which="both", alpha=0.4)
    plt.tight_layout()
    fig.savefig(out_dir / f"fr_welch_{tag}.png", dpi=150)
    plt.close(fig)

    fig, ax = plt.subplots(figsize=(10, 3.5))
    ax.semilogx(fcs, nco, color="purple")
    ax.set_ylim(0, 1.02)
    ax.set_xlim(20, nyq)
    ax.set_xlabel("Frequency (Hz)")
    ax.set_ylabel("Coherence")
    ax.set_title("NAM coherence vs dry — T3K")
    ax.grid(True, which="both", alpha=0.4)
    plt.tight_layout()
    fig.savefig(out_dir / f"fr_coherence_{tag}.png", dpi=150)
    plt.close(fig)

    return {
        "nam_vs_spice_max_err_db_20_20k": round(float(np.max(np.abs(err[band]))), 2) if band.any() else None,
        "nam_vs_spice_max_err_db_midband": round(float(np.max(np.abs(err[midband]))), 2) if midband.any() else None,
        "mean_coherence_midband": round(float(np.mean(nco[midband])), 3) if midband.any() else None,
    }


def _shift(x: np.ndarray, lag: int) -> np.ndarray:
    """Shift *x* by *lag* (so x[lag:] aligns to index 0), zero-padded."""
    if lag > 0:
        return np.concatenate([x[lag:], np.zeros(lag)])
    if lag < 0:
        return np.concatenate([np.zeros(-lag), x[:lag]])
    return x


def _overlay(spice, acme, nam, fit_gain, sr, tag, out_dir) -> None:
    """50 ms 3-way waveform overlay (NAM scaled into the SPICE volt domain),
    with ACME and NAM lag-aligned to the SPICE truth."""
    n = min(len(spice), len(acme), len(nam))
    s_hp = _hp(spice[:n], sr)
    a_hp = _hp(_shift(acme[:n], best_lag(spice[:n], acme[:n])), sr)
    nm = _hp(_shift(fit_gain * nam[:n], best_lag(spice[:n], nam[:n])), sr)
    w = min(int(0.05 * sr), n // 2)
    # centre the window on the highest-energy 50 ms of the sweep (skip warmup)
    g0 = int(0.5 * sr)
    env = np.convolve(s_hp[g0:] ** 2, np.ones(w), "valid")
    st = g0 + (int(np.argmax(env)) if env.size else 0)
    st = min(st, n - w)
    t = np.arange(w) / sr * 1e3
    fig, axes = plt.subplots(2, 1, figsize=(10, 6), sharex=True)
    axes[0].plot(t, a_hp[st:st + w], color="steelblue", lw=2.4, label="ACME")
    axes[0].plot(t, s_hp[st:st + w], color="black", lw=1.0, label="SPICE (truth)")
    axes[0].plot(t, nm[st:st + w], color="darkorange", ls="--", label="NAM (level-matched)")
    axes[0].set_ylabel("volts")
    axes[0].set_title(f"Waveform overlay — {tag}")
    axes[0].legend(fontsize=8)
    axes[0].grid(True, alpha=0.3)
    axes[1].plot(t, s_hp[st:st + w] - nm[st:st + w], color="crimson", label="SPICE − NAM")
    axes[1].plot(t, s_hp[st:st + w] - a_hp[st:st + w], color="steelblue", lw=0.9, label="SPICE − ACME")
    axes[1].set_ylabel("residual")
    axes[1].set_xlabel("time (ms)")
    axes[1].legend(fontsize=8)
    axes[1].grid(True, alpha=0.3)
    plt.tight_layout()
    fig.savefig(out_dir / f"waveform_overlay_{tag}.png", dpi=150)
    plt.close(fig)


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def _score_one_gain(model, dry, sr, gain, bias, out_dir):
    """Render NAM (at *gain*) + ACME (at the matching R_KJ) + load the SPICE
    target for *gain*, score the 3-way, write per-gain FR/overlay plots.

    Returns ``(summary_dict, per_gain_row)`` or raises on a missing target.
    """
    rkj = gain_norm_to_rkj(gain)
    target = NAM_SWEEP / f"T3K-60s_gain{gain:.3f}.wav"
    if not target.exists():
        raise FileNotFoundError(f"SPICE target for gain {gain:.3f} not found: {target}")

    spice, sr_t = sf.read(target, always_2d=False)
    spice = np.asarray(spice, dtype=np.float64)
    if sr_t != sr:
        spice = _resample(spice, sr_t, sr)
    spice = spice[: len(dry)]

    print(f"\n--- gain_norm={gain:.3f} (R_KJ={rkj:.0f}Ω) -> {target.name} ---")

    # NAM at this gain.  Conditioned model uses the gain channel; the legacy
    # SlimmableContainer ignores `gain` (back-compat) and renders one point.
    nam_gain = gain if model._conditioned else None
    nam_wet, thr = model.run(dry, gain=nam_gain)
    print(f"[nam] gain={thr}  out_pk={np.max(np.abs(nam_wet)):.4f}  "
          f"finite={np.all(np.isfinite(nam_wet))}")

    # ACME at the operating point (run at 96 kHz, then back to 48 kHz).
    dry96 = _resample(dry, sr, SR_ACME)
    acme96 = run_acme(dry96, SR_ACME, rgain=rkj, bias=bias)
    acme = _resample(acme96, SR_ACME, sr)
    print(f"[acme] out_pk={np.max(np.abs(acme)):.3f}  finite={np.all(np.isfinite(acme))}")

    n = min(len(nam_wet), len(acme), len(spice))
    nam_wet, acme, spice_n, dry_n = nam_wet[:n], acme[:n], spice[:n], dry[:n]

    m_na = metrics(acme, nam_wet, sr)     # NAM vs ACME
    m_ns = metrics(spice_n, nam_wet, sr)  # NAM vs SPICE truth
    m_as = metrics(spice_n, acme, sr)     # ACME vs SPICE (reference sanity)

    print(f"  NAM  vs ACME : ESR {m_na['esr_matched_db']:+6.2f} dB  lag={m_na['lag']:+d}")
    print(f"  NAM  vs SPICE: ESR {m_ns['esr_matched_db']:+6.2f} dB  "
          f"fit_gain={m_ns['fit_gain']:.2f}  lag={m_ns['lag']:+d}")
    print(f"  ACME vs SPICE: ESR {m_as['esr_matched_db']:+6.2f} dB  lag={m_as['lag']:+d}")

    tag = f"T3K_g{gain:.3f}"
    nam_lm = m_ns["fit_gain"] * nam_wet
    fcs, sdb, _ = _welch_tf(spice_n, dry_n, sr)
    _, adb, _ = _welch_tf(acme, dry_n, sr)
    _, ndb, nco = _welch_tf(nam_lm, dry_n, sr)
    fr = _fr_plot(fcs, sdb, adb, ndb, nco, gain, rkj, bias, sr, out_dir, tag)
    _overlay(spice_n, acme, nam_wet, m_ns["fit_gain"], sr, tag, out_dir)

    def _pack(m):
        return {k: (round(v, 3) if isinstance(v, float) else v) for k, v in m.items()}

    per_gain_summary = {
        "gain_norm": gain, "rkj_ohm": round(rkj, 1), "target": target.name,
        "nam_vs_acme": _pack(m_na), "nam_vs_spice": _pack(m_ns),
        "acme_vs_spice": _pack(m_as), "frequency_response": fr,
        "verdict": _verdict_one(m_na, m_ns, m_as, gain),
    }
    row = {
        "gain_norm": gain, "rkj_ohm": round(rkj, 1),
        "nam_vs_spice_esr_db": round(m_ns["esr_matched_db"], 2),
        "nam_vs_acme_esr_db": round(m_na["esr_matched_db"], 2),
        "acme_vs_spice_esr_db": round(m_as["esr_matched_db"], 2),
    }
    return per_gain_summary, row


def _score_one_stepped(model, dry, sr, idx, rkj, amp, bias, out_dir):
    """STEPPED (WS4): score one (position idx, input-amplitude) point.

    Renders NAM at the discrete switch position ``idx`` (the conditioning index),
    ACME at the matching ``rkj``, and the per-position SPICE target, then scores
    the 3-way (level-matched ESR, 20 Hz HP, lag-aligned) — the SAME contract as
    the continuous sweep. ``amp`` is a LINEAR input-gain applied to the dry to
    drive the circuit hot (hot-input-amplitude check); ``amp == 1.0`` is the
    nominal point. The SPICE target is looked up by position index and amplitude
    (``T3K-60s_pos{idx}_amp{amp}.wav``); a missing target raises (recorded as a
    skip by the caller). Returns ``(summary_dict, per_position_row)``.
    """
    dry_a = dry * float(amp)
    target = NAM_SWEEP / f"T3K-60s_pos{idx}_amp{amp:g}.wav"
    if not target.exists():
        raise FileNotFoundError(
            f"stepped SPICE target for pos {idx} amp {amp:g} not found: {target}"
        )

    spice, sr_t = sf.read(target, always_2d=False)
    spice = np.asarray(spice, dtype=np.float64)
    if sr_t != sr:
        spice = _resample(spice, sr_t, sr)
    spice = spice[: len(dry_a)]

    print(f"\n--- pos {idx} (R_KJ={rkj:g}Ω) amp x{amp:g} -> {target.name} ---")

    # The stepped NAM is conditioned on the DISCRETE position index, not a
    # continuous gain. NamModel's `gain` channel carries that index for stepped
    # models; legacy/continuous models ignore it (single operating point).
    nam_gain = float(idx) if model._conditioned else None
    nam_wet, thr = model.run(dry_a, gain=nam_gain)

    dry96 = _resample(dry_a, sr, SR_ACME)
    acme96 = run_acme(dry96, SR_ACME, rgain=rkj, bias=bias)
    acme = _resample(acme96, SR_ACME, sr)

    n = min(len(nam_wet), len(acme), len(spice))
    nam_wet, acme, spice_n = nam_wet[:n], acme[:n], spice[:n]

    m_na = metrics(acme, nam_wet, sr)
    m_ns = metrics(spice_n, nam_wet, sr)
    m_as = metrics(spice_n, acme, sr)
    print(f"  NAM vs SPICE: ESR {m_ns['esr_matched_db']:+6.2f} dB  lag={m_ns['lag']:+d}")

    def _pack(m):
        return {k: (round(v, 3) if isinstance(v, float) else v) for k, v in m.items()}

    summary = {
        "position_idx": idx, "rkj_ohm": round(rkj, 1), "input_amp": amp,
        "target": target.name, "nam_vs_acme": _pack(m_na),
        "nam_vs_spice": _pack(m_ns), "acme_vs_spice": _pack(m_as),
    }
    row = {
        "position_idx": idx, "rkj_ohm": round(rkj, 1), "input_amp": amp,
        "nam_vs_spice_esr_db": round(m_ns["esr_matched_db"], 2),
        "nam_vs_acme_esr_db": round(m_na["esr_matched_db"], 2),
        "acme_vs_spice_esr_db": round(m_as["esr_matched_db"], 2),
    }
    return summary, row


def _run_stepped(model, dry, sr, bias, amps, out_dir) -> None:
    """STEPPED (WS4): sweep the 5 positions x ``amps`` and write a summary.

    Gate = the WORST NAM-vs-SPICE ESR over every (position, amplitude) point, so
    a single weak detent or a hot-drive blow-up is not hidden by the average.
    """
    rows: list[dict] = []
    per_pos: list[dict] = []
    skipped: list[dict] = []
    for idx, rkj in enumerate(STEPPED_RKJ_OHMS):
        for amp in amps:
            try:
                s, row = _score_one_stepped(
                    model, dry, sr, idx, rkj, amp, bias, out_dir
                )
            except Exception as e:  # noqa: BLE001 — record per-point failure
                print(f"[skip] pos {idx} amp {amp:g}: {e}")
                skipped.append({"position_idx": idx, "input_amp": amp, "error": str(e)})
                continue
            per_pos.append(s)
            rows.append(row)

    if not rows:
        raise SystemExit("all stepped points failed; no summary written")

    worst = max(rows, key=lambda r: r["nam_vs_spice_esr_db"])
    best = min(rows, key=lambda r: r["nam_vs_spice_esr_db"])
    quality = _quality_label(worst["nam_vs_spice_esr_db"])
    verdict = (
        f"{quality} — worst NAM vs SPICE {worst['nam_vs_spice_esr_db']:+.1f} dB "
        f"@ pos {worst['position_idx']} amp x{worst['input_amp']:g} "
        f"(best {best['nam_vs_spice_esr_db']:+.1f} dB @ pos {best['position_idx']} "
        f"amp x{best['input_amp']:g}); gate = worst (position, amplitude) point."
        + (f" [{len(skipped)} point(s) skipped]" if skipped else "")
    )
    summary = {
        "mode": "stepped",
        "config": {
            "positions_rkj": list(STEPPED_RKJ_OHMS), "hot_amps": list(amps),
            "bias": bias, "sr": sr,
            "metric": "level-matched ESR (optimal LS scalar), 20Hz HP, lag-aligned",
        },
        "per_position_table": rows,
        "skipped": skipped,
        "worst": worst, "best": best, "verdict": verdict,
        "per_position": per_pos,
    }
    (out_dir / "summary_stepped.json").write_text(json.dumps(summary, indent=2))
    print("\n" + "=" * 64)
    print("STEPPED (WS4) PER-POSITION ESR (NAM vs SPICE, matched dB):")
    for r in rows:
        print(f"  pos {r['position_idx']} R_KJ={r['rkj_ohm']:g}Ω amp x{r['input_amp']:g}  "
              f"NAM-SPICE {r['nam_vs_spice_esr_db']:+6.2f}")
    print(f"VERDICT: {verdict}")
    print("=" * 64)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("--gains", type=str, default=DEFAULT_GAINS,
                    help="comma-separated gain_norm grid to sweep")
    ap.add_argument("--gain", type=float, default=None,
                    help="single gain_norm shortcut (overrides --gains if given)")
    ap.add_argument("--bias", type=float, default=0.77)
    ap.add_argument("--dry", type=Path, default=DRY48, help="T3K training dry (48 kHz)")
    ap.add_argument("--nam", type=Path, default=COND_NAM,
                    help="conditioned .nam (default: model.cond.nam)")
    ap.add_argument("--max-seconds", type=float, default=30.0,
                    help="trim dry/target to keep the ACME (Julia) sim tractable")
    ap.add_argument("--out", type=Path, default=Path("eval/nam_vs_acme/out"))
    # STEPPED (WS4): score the 5 discrete hardware positions (+ hot-input-amp
    # check) instead of the continuous 9-gain sweep.
    ap.add_argument("--stepped", action="store_true",
                    help="WS4: score the 5 discrete gain positions + hot-input "
                         "amplitudes instead of the continuous gain sweep")
    ap.add_argument("--hot-amps", type=str,
                    default=",".join(f"{a:g}" for a in STEPPED_HOT_AMPS),
                    help="WS4 stepped mode: comma-separated linear input gains "
                         "for the hot-input-amplitude check")
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)

    # STEPPED (WS4): the stepped path shares the model/dry loading below but
    # dispatches to _run_stepped instead of the continuous gain sweep.
    if args.stepped:
        amps = [float(a) for a in args.hot_amps.split(",") if a.strip()]
        model = NamModel(args.nam)
        sr = model.sample_rate
        dry, sr_dry = sf.read(args.dry, always_2d=False)
        dry = np.asarray(dry, dtype=np.float64)
        if sr_dry != sr:
            dry = _resample(dry, sr_dry, sr)
        dry = dry[: int(args.max_seconds * sr)]
        print("=" * 64)
        print("NAM vs ACME vs SPICE — BA283 STEPPED (WS4) 5-position check")
        print(f"  model     : {args.nam} (conditioned={model._conditioned})")
        print(f"  positions : {list(STEPPED_RKJ_OHMS)} Ω")
        print(f"  hot amps  : {amps}")
        print("=" * 64)
        _run_stepped(model, dry, sr, args.bias, amps, args.out)
        return

    if args.gain is not None:
        gains = [float(args.gain)]
    else:
        gains = [float(g) for g in args.gains.split(",") if g.strip()]

    # Reject out-of-range gains BEFORE the sweep: ACME clamps R_KJ to [0,1] but
    # the conditioned NAM feeds the raw gain to its conditioning channel, so an
    # out-of-[0,1] gain is both out-of-distribution (model trained on [0,1]) and
    # inconsistent between the two paths.
    bad = [g for g in gains if not (0.0 <= g <= 1.0)]
    if bad:
        raise SystemExit(
            f"gain_norm out of range [0,1]: {bad}; the conditioned NAM was "
            f"trained on [0,1] and ACME clamps R_KJ — refusing to sweep."
        )

    model = NamModel(args.nam)
    sr = model.sample_rate

    # A legacy (non-conditioned) model has a SINGLE operating point: it renders
    # the SAME output at every gain, which a multi-gain sweep would score
    # against DIFFERENT SPICE targets — silently misleading "follows gain" data.
    if not model._conditioned and len(gains) > 1:
        print("[warn] model is NOT gain-conditioned (single operating point): "
              "the NAM output is IDENTICAL at every gain and is being scored "
              "against different SPICE targets — the per-gain trend is not a "
              "'model follows gain' signal, only a render-vs-legacy diagnostic.")

    dry, sr_dry = sf.read(args.dry, always_2d=False)
    dry = np.asarray(dry, dtype=np.float64)
    if sr_dry != sr:
        dry = _resample(dry, sr_dry, sr)
    dry = dry[: int(args.max_seconds * sr)]

    print("=" * 64)
    print("NAM vs ACME vs SPICE — BA283 T3K gain sweep")
    print(f"  model   : {args.nam}")
    print(f"            arch={model.arch}, SR={sr}, "
          f"conditioned={model._conditioned}")
    print(f"  dry     : {args.dry.name}  ({len(dry)/sr:.1f}s)")
    print(f"  gains   : {gains}")
    print(f"  ACME    : {SR_ACME} Hz (dry upsampled), compared at {sr} Hz, bias={args.bias}")
    print("=" * 64)

    # Per-gain sweep: one bad gain (missing target / ACME or Julia error) is a
    # RECORDED FAILURE, not a sweep-killer — render the rest and still write a
    # summary.  Successful rows carry the ESR keys; skipped rows carry `error`.
    per_gain: list[dict] = []
    rows: list[dict] = []
    skipped: list[dict] = []
    for g in gains:
        try:
            s, row = _score_one_gain(model, dry, sr, g, args.bias, args.out)
        except Exception as e:  # noqa: BLE001 — record any per-gain failure
            print(f"[skip] gain_norm={g:.3f}: {e}")
            skipped.append({"gain_norm": g, "error": str(e)})
            continue
        per_gain.append(s)
        rows.append(row)

    if not rows:
        raise SystemExit("all gains failed; no summary written")

    # Worst-gain gate: the conditioned model must hold accuracy across the
    # whole sweep, so the verdict is driven by the WORST (highest) NAM-vs-SPICE
    # ESR over all gains.  Computed ONLY over SUCCESSFUL rows (skipped rows have
    # no ESR key, so filtering them avoids a KeyError in max/min).
    worst = max(rows, key=lambda r: r["nam_vs_spice_esr_db"])
    best = min(rows, key=lambda r: r["nam_vs_spice_esr_db"])
    worst_ns = worst["nam_vs_spice_esr_db"]
    quality = _quality_label(worst_ns)
    verdict = (f"{quality} — worst-gain NAM vs SPICE {worst_ns:+.1f} dB "
               f"@ gain_norm={worst['gain_norm']:.3f} "
               f"(best {best['nam_vs_spice_esr_db']:+.1f} dB @ "
               f"gain_norm={best['gain_norm']:.3f}); gate = worst gain over the sweep."
               + (f" [{len(skipped)} gain(s) skipped]" if skipped else ""))

    summary = {
        "config": {"nam": str(args.nam), "arch": model.arch,
                   "conditioned": model._conditioned, "sr": sr,
                   "acme_sr": SR_ACME, "gains": gains, "bias": args.bias,
                   "dry": str(args.dry), "seconds": round(len(dry) / sr, 2),
                   "max_lag_samples": SPAN, "onset_skip_s": ONSET_SKIP,
                   "metric": "level-matched ESR (optimal LS scalar), 20Hz HP, lag-aligned"},
        "per_gain_table": rows,
        "skipped_gains": skipped,
        "worst_gain": worst,
        "best_gain": best,
        "verdict": verdict,
        "per_gain": per_gain,
        "notes": [
            "Gain-conditioned NAM (2-ch [audio,gain] WaveNet) swept over the ds48 "
            "9-gain grid; each gain feeds the NAM conditioning channel + the matching "
            "R_KJ to ACME + the per-gain SPICE reamp target.",
            "NAM audio channel scaled by audio_input_scale (=2.0, the training "
            "INPUT_SCALE) and front-padded RF-1 zeros for time alignment.",
            "NAM output is normalized (not volts); ESR is level-matched (fit_gain ~ the "
            "target volt scale, near OUTPUT_REF_V≈64).",
            "ACME at 96 kHz (dry upsampled from 48 kHz), wet downsampled to 48 kHz; "
            "SPICE target is the on-disk 48 kHz training wet.",
            "Lag-align (|lag| <= SPAN) + 20 Hz HP remove the calibrated delay and the "
            "DC/sub-bass offset before scoring.",
            "Worst-gain gate: the verdict tracks the worst NAM-vs-SPICE ESR across the "
            "sweep so a single weak operating point is not hidden by the average.",
        ],
    }
    (args.out / "summary.json").write_text(json.dumps(summary, indent=2))

    print("\n" + "=" * 64)
    print("PER-GAIN ESR (NAM vs SPICE, matched dB):")
    for r in rows:
        print(f"  gain_norm={r['gain_norm']:.3f}  "
              f"NAM-SPICE {r['nam_vs_spice_esr_db']:+6.2f}  "
              f"NAM-ACME {r['nam_vs_acme_esr_db']:+6.2f}  "
              f"ACME-SPICE {r['acme_vs_spice_esr_db']:+6.2f}")
    print(f"VERDICT: {verdict}")
    print(f"Plots + summary in: {args.out}")
    print("=" * 64)


def _quality_label(esr_db: float) -> str:
    """Map a NAM-vs-SPICE matched ESR (dB) to a verdict label (single source of
    truth for the -25/-18/-12 thresholds, shared by per-gain + worst-gain gates)."""
    return ("STRONG" if esr_db <= -25 else "FAIR" if esr_db <= -18 else
            "MODERATE" if esr_db <= -12 else "WEAK")


def _verdict_one(m_na, m_ns, m_as, gain) -> str:
    na, ns, as_ = m_na["esr_matched_db"], m_ns["esr_matched_db"], m_as["esr_matched_db"]
    quality = _quality_label(ns)
    return (f"{quality} — gain_norm={gain:.3f}: NAM vs SPICE {ns:+.1f} dB, "
            f"NAM vs ACME {na:+.1f} dB; ACME tracks SPICE at {as_:+.1f} dB.")


if __name__ == "__main__":
    main()
