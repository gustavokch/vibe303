"""Core metrics for evaluation harness."""
from __future__ import annotations
import numpy as np
import scipy.signal

def best_lag(ref: np.ndarray, mod: np.ndarray, max_lag: int = 256) -> int:
    n = min(len(ref), len(mod))
    if n <= 0:
        return 0
    r = ref[:n] - ref[:n].mean()
    m = mod[:n] - mod[:n].mean()
    
    best_lag_val = 0
    best_corr = -np.inf
    
    for lag in range(-max_lag, max_lag + 1):
        if lag > 0:
            corr_val = np.dot(r[lag:], m[:-lag])
        elif lag < 0:
            k = -lag
            corr_val = np.dot(r[:-k], m[k:])
        else:
            corr_val = np.dot(r, m)
            
        if corr_val > best_corr:
            best_corr = corr_val
            best_lag_val = lag
            
    return best_lag_val


def _align(ref, mod):
    lag = best_lag(ref, mod)
    if lag > 0:
        ref, mod = ref[lag:], mod[: len(ref) - lag]
    elif lag < 0:
        mod, ref = mod[-lag:], ref[: len(mod) + lag]
    n = min(len(ref), len(mod))
    return ref[:n], mod[:n]


def esr_db(esr: float) -> float:
    return float(10.0 * np.log10(esr + 1e-30))


def esr_metrics(ref: np.ndarray, mod: np.ndarray) -> dict:
    ref = np.asarray(ref, float)
    mod = np.asarray(mod, float)
    lag = best_lag(ref, mod)
    r, m = _align(ref, mod)
    err = r - m
    esr = float(np.sum(err**2) / max(np.sum(r**2), 1e-20))
    nrmse = float(np.sqrt(np.mean(err**2)) / max(r.max() - r.min(), 1e-20))
    return {"lag": lag, "esr": esr, "esr_db": esr_db(esr), "nrmse": nrmse}


def pre_emphasis(x: np.ndarray, alpha: float = 0.8613) -> np.ndarray:
    """Perceptual HF weighting (matched-z of the paper's 0.85 @ 48 kHz)."""
    y = np.empty_like(x)
    y[0] = x[0]
    y[1:] = x[1:] - alpha * x[:-1]
    return y


def lowpass_decimate_2(x: np.ndarray) -> np.ndarray:
    """Kaiser-sinc lowpass filter and decimate by 2.
    Converts 192 kHz signal to 96 kHz.
    Cutoff frequency fc = 44.1 kHz (normalized fc = 44.1 / 192.0 ≈ 0.23).
    """
    N = 65
    fc = 0.23
    t = np.arange(N) - (N - 1) / 2
    with np.errstate(divide='ignore', invalid='ignore'):
        h = np.sin(2 * np.pi * fc * t) / (np.pi * t)
    h[(N - 1) // 2] = 2 * fc
    h *= np.kaiser(N, 6.0)
    h /= np.sum(h)
    
    y = np.convolve(x, h, mode="same")
    return y[::2]


def welch_pow(x: np.ndarray, seg: int = 1024) -> np.ndarray:
    """Hann, 50 % overlap, averaged power spectrum normalised to unit sum."""
    win = np.hanning(seg)
    hop = seg // 2
    nseg = max((len(x) - seg) // hop + 1, 1)
    acc = np.zeros(seg // 2)
    for s in range(0, nseg * hop, hop):
        seg_data = x[s:s + seg]
        if len(seg_data) < seg:
            break
        sp = np.fft.rfft(seg_data * win)[: seg // 2]
        acc += np.abs(sp) ** 2
    acc /= nseg
    return acc / max(acc.sum(), 1e-300)


def spectral_centroid(x: np.ndarray, sr: float, seg: int = 1024) -> float:
    """Calculate the spectral centroid (Hz) of the signal using Welch power spectrum."""
    p = welch_pow(x, seg)
    freqs = np.fft.rfftfreq(seg, 1 / sr)[:seg // 2]
    return float(np.sum(freqs * p) / max(np.sum(p), 1e-300))


def spectral_match(gold: np.ndarray, model: np.ndarray, sr: float, seg: int = 1024,
                   band: tuple[float, float] = (40.0, 16000.0)) -> tuple[float, float]:
    """Calculate power-weighted LSD (dB) and Pearson correlation of dB spectra."""
    gp = welch_pow(gold, seg)
    mp = welch_pow(model, seg)
    df = sr / seg
    lo, hi = int(band[0] / df), int(band[1] / df)
    gp, mp = gp[lo:hi], mp[lo:hi]
    gdb = 10 * np.log10(np.maximum(gp, 1e-300))
    mdb = 10 * np.log10(np.maximum(mp, 1e-300))
    gdb -= gdb.mean()
    mdb -= mdb.mean()
    lsd = float(np.sqrt(np.sum(gp * (gdb - mdb) ** 2) / max(gp.sum(), 1e-300)))
    corr_matrix = np.corrcoef(gdb, mdb)
    corr = float(corr_matrix[0, 1]) if np.all(np.isfinite(corr_matrix)) else 0.0
    return lsd, corr


def rms_dbfs(x: np.ndarray) -> float:
    """Root-mean-square level in dBFS."""
    rms = np.sqrt(np.mean(x ** 2))
    return float(20.0 * np.log10(rms + 1e-15))


def crest_factor_db(x: np.ndarray) -> float:
    """Crest factor (peak-to-RMS ratio) in dB."""
    peak = np.max(np.abs(x))
    rms = np.sqrt(np.mean(x ** 2))
    return float(20.0 * np.log10(peak / max(rms, 1e-15) + 1e-15))


def norm_signal(x: np.ndarray) -> np.ndarray:
    """Normalize signal to unit RMS."""
    rms = np.sqrt(np.mean(x ** 2))
    return x / rms if rms > 1e-20 else x


def pitch_tracking_error_cents(ref_hz: float, sig: np.ndarray, sr: float) -> float:
    """Calculate pitch tracking error in cents using autocorrelation."""
    corr = np.correlate(sig, sig, mode='full')
    corr = corr[len(corr)//2:]
    peaks, _ = scipy.signal.find_peaks(corr)
    if len(peaks) == 0:
        return 0.0
    
    p = peaks[0]
    if 0 < p < len(corr) - 1:
        alpha = corr[p-1]
        beta = corr[p]
        gamma = corr[p+1]
        p_interp = p + 0.5 * (alpha - gamma) / (alpha - 2*beta + gamma)
    else:
        p_interp = p
        
    measured_hz = sr / p_interp
    return 1200.0 * np.log2(measured_hz / ref_hz)


def t60_decay_time(env_sig: np.ndarray, sr: float) -> float:
    """Calculate the T60 decay time (time to drop by 60dB or a factor of 1000)."""
    peak = np.max(env_sig)
    if peak <= 1e-10:
        return 0.0
    thresh = peak * 0.001
    
    # Find the first index after the peak where the envelope drops below thresh
    peak_idx = np.argmax(env_sig)
    post_peak = env_sig[peak_idx:]
    
    below_thresh = np.where(post_peak < thresh)[0]
    if len(below_thresh) == 0:
        # Never dropped below threshold
        return float(len(post_peak)) / sr
        
    return float(below_thresh[0]) / sr


def dc_offset_drift(sig: np.ndarray, sr: float = 48000.0) -> np.ndarray:
    """Calculate the DC offset drift over time using a 5Hz lowpass filter."""
    b, a = scipy.signal.butter(1, 5.0 / (sr / 2.0), btype='low')
    return scipy.signal.filtfilt(b, a, sig)

