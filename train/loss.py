# train/loss.py
import numpy as np
import torch
import torch.nn.functional as F
from scipy.signal import firwin2

# First-order pre-emphasis coefficient. Wright & Välimäki (arXiv:1911.08922) use
# H(z) = 1 - 0.85 z^-1 at 44.1 kHz. Our data is 48 kHz, so the real-axis zero z=c
# is matched-z mapped to the new rate (c = e^{-sigma/fs} => c_new = c_old^(fs_old/fs_new)):
#   0.85 ** (44100/48000) = 0.8613
# This keeps the pre-emphasis corner at the same frequency in Hz as the paper.
PREEMPH_ALPHA_48K = 0.8613


def pre_emphasis(x: torch.Tensor, alpha: float = PREEMPH_ALPHA_48K) -> torch.Tensor:
    return x[:, 1:, :] - alpha * x[:, :-1, :]


def dc_loss(
    pred: torch.Tensor, target: torch.Tensor, eps: float = 1e-3
) -> torch.Tensor:
    """Wright & Välimäki DC loss: mean-error squared over mean target energy.

    The high-pass pre-emphasis filter zeroes DC/low-freq in the error signal, so a
    pure pre-emphasised ESR cannot see (and therefore cannot penalise) a constant
    output offset. The paper pairs pre-emphasis with this term as the explicit
    safeguard. Computed on the RAW signals -- DC is exactly what pre-emphasis
    strips, so it must be measured before any filtering. Per-(batch,channel) time
    mean, then summed and normalised by mean target energy (mirrors ``esr``).
    """
    err = (pred - target).mean(dim=1)  # (B, C): mean over time
    return (err ** 2).sum() / ((target ** 2).mean(dim=1).sum() + eps)


def esr(pred: torch.Tensor, target: torch.Tensor, eps: float = 1e-3) -> torch.Tensor:
    # ESR is error_energy / target_energy. ``eps`` floors the denominator: as the
    # target energy -> 0 the ratio (and its backward num/denom**2 term) blows up
    # to a non-finite gradient. The dataset already drops near-silent windows
    # (see train.dataset.MIN_TARGET_ENERGY); this floor is a second line of
    # defense, sized for the normalized (~unit) target domain so it is negligible
    # for energetic windows.
    return ((pred - target) ** 2).sum() / ((target ** 2).sum() + eps)


# --- A-weighting pre-emphasis variant (Wright & Välimäki, paper section IV) ---
# A fixed linear-phase FIR matching the IEC 61672 A-weighting magnitude. Used as an
# alternative pre-emphasis filter: instead of the flat first-order high-pass, weight
# the error by perceptual loudness (peak ~2-4 kHz, rolling off lows AND extreme highs).
# FIR (not the analog IIR) keeps it differentiable and numerically stable under
# backprop; applied identically to pred and target, its linear-phase group delay
# cancels inside ESR. Cached per (num_taps, fs).
_AWEIGHT_CACHE: dict[tuple[int, int], torch.Tensor] = {}


def _aweight_magnitude(f: np.ndarray) -> np.ndarray:
    # IEC 61672 R_A(f); poles at 20.6, 107.7, 737.9, 12194 Hz. R_A(0)=0 (f^4 num).
    f2 = f ** 2
    num = (12194.0 ** 2) * f2 ** 2
    den = (
        (f2 + 20.6 ** 2)
        * np.sqrt((f2 + 107.7 ** 2) * (f2 + 737.9 ** 2))
        * (f2 + 12194.0 ** 2)
    )
    return num / den


def _aweight_fir(num_taps: int = 513, fs: int = 48000) -> torch.Tensor:
    """Linear-phase A-weighting FIR as a ``(1, 1, num_taps)`` conv kernel.

    ``num_taps`` is odd for a type-I (symmetric) linear-phase filter so the delay is
    an integer ``(num_taps-1)//2`` samples and 'same'-length convolution is exact.
    """
    key = (num_taps, fs)
    cached = _AWEIGHT_CACHE.get(key)
    if cached is not None:
        return cached
    grid = np.linspace(0.0, fs / 2.0, 512)
    gains = _aweight_magnitude(grid)
    gains = gains / gains.max()  # normalise peak to unity
    taps = firwin2(num_taps, grid / (fs / 2.0), gains)
    kernel = torch.from_numpy(taps).float().reshape(1, 1, num_taps)
    _AWEIGHT_CACHE[key] = kernel
    return kernel


def aweight(x: torch.Tensor, fs: int = 48000) -> torch.Tensor:
    """A-weight a ``(batch, time, channels)`` signal along time (linear phase)."""
    kernel = _aweight_fir(fs=fs).to(x.device, x.dtype)
    pad = (kernel.shape[-1] - 1) // 2
    # conv1d wants (B, C, T); fold channels into the batch so one shared kernel
    # filters every channel independently, then restore the (B, T, C) layout.
    b, t, c = x.shape
    xc = x.permute(0, 2, 1).reshape(b * c, 1, t)
    yc = F.conv1d(xc, kernel, padding=pad)
    return yc.reshape(b, c, t).permute(0, 2, 1)


# Default STFT resolutions for the multi-resolution spectral loss. Three scales
# (short -> long window) cover transient/click structure through low harmonics;
# hop = n_fft // 4 (75% overlap) is the auraloss/Yamamoto default.
_MRSTFT_FFT_SIZES = (512, 1024, 2048)


def _stft_mag(x: torch.Tensor, n_fft: int, eps: float = 1e-7) -> torch.Tensor:
    """Magnitude STFT of a ``(batch, time)`` signal at one resolution.

    ``center=True`` reflect-pads by ``n_fft // 2`` so signals shorter than
    ``n_fft`` still transform (the train window minus warmup can be ~3072, below
    the 2048-pt scale only by a margin); a Hann window matches the analysis the
    plugin's spectral checks use.
    """
    # torch.stft returns a complex tensor, which the MPS backend cannot allocate
    # ("Trying to convert ComplexFloat to the MPS backend"). Run the transform on
    # CPU when the input is on MPS and move the REAL magnitude back to the model
    # device; autograd flows back across the device boundary. On CUDA/CPU
    # ``stft_dev == dev`` so this path is unchanged (bit-identical).
    dev = x.device
    stft_dev = "cpu" if dev.type == "mps" else dev
    xin = x.to(stft_dev)
    window = torch.hann_window(n_fft, device=stft_dev, dtype=xin.dtype)
    spec = torch.stft(
        xin,
        n_fft=n_fft,
        hop_length=n_fft // 4,
        win_length=n_fft,
        window=window,
        center=True,
        return_complex=True,
    )
    return spec.abs().clamp_min(eps).to(dev)


def mrstft(
    pred: torch.Tensor,
    target: torch.Tensor,
    fft_sizes: tuple[int, ...] = _MRSTFT_FFT_SIZES,
    eps: float = 1e-7,
) -> torch.Tensor:
    """Multi-resolution STFT loss = mean over scales of (spectral-convergence +
    log-magnitude L1).

    A time-domain ESR underweights the harmonic / aliasing structure that *is* a
    clipping circuit's character; the spectral term penalises magnitude-spectrum
    mismatch the waveform metric is blind to. Inputs are ``(batch, time, 1)``
    (the model/loss tensor layout); the trailing unit channel is squeezed off.
    """
    p = pred.squeeze(-1)
    t = target.squeeze(-1)
    # ``center=True`` reflect-pads by n_fft//2, which must be < the signal length;
    # drop scales too large for the current window (keeps short smoke/eval windows
    # safe). At the default 3072-sample train window all three scales apply.
    n = p.shape[-1]
    scales = [k for k in fft_sizes if k // 2 < n]
    if not scales:
        # Signal shorter than every configured scale (defensive: real training
        # windows are ~3072 and fit all three). Fall back to the largest
        # power-of-two whose half-window still fits, floored at 8.
        scales = [1 << max(3, (n - 1).bit_length() - 1)]
    total = pred.new_zeros(())
    for n_fft in scales:
        sp = _stft_mag(p, n_fft, eps)
        st = _stft_mag(t, n_fft, eps)
        # Spectral convergence: Frobenius norm of the magnitude error, normalised
        # by the target magnitude norm (per-item, then averaged over the batch).
        sc = torch.linalg.norm(st - sp, dim=(-2, -1)) / (
            torch.linalg.norm(st, dim=(-2, -1)) + eps
        )
        log_mag = F.l1_loss(torch.log(sp), torch.log(st))
        total = total + sc.mean() + log_mag
    return total / len(scales)


def composite_loss(
    pred: torch.Tensor,
    target: torch.Tensor,
    *,
    esr_weight: float = 1.0,
    alpha: float = PREEMPH_ALPHA_48K,
    stft_weight: float = 0.0,
    dc_weight: float = 0.1,
    preemph: str = "first_order",
    fs: int = 48000,
    fft_sizes: tuple[int, ...] = _MRSTFT_FFT_SIZES,
) -> torch.Tensor:
    # Default is the Wright & Välimäki perceptual recipe: pre-emphasised ESR
    # (esr_weight=1.0) PLUS the DC term (dc_weight=0.1) the paper pairs with
    # pre-emphasis. ``esr_weight`` keeps the ESR+MSE blend available for A/B;
    # ``stft_weight`` adds the multi-resolution spectral term (raw, on the
    # normalised output domain) to cover harmonic structure ESR misses.
    # ``preemph`` selects the pre-emphasis filter: 'first_order' (flat high-pass,
    # alpha) or 'aweight' (IEC A-weighting FIR, perceptually weighted). The chosen
    # filter is applied to BOTH pred and target so its delay cancels inside ESR.
    if preemph == "aweight":
        pe = aweight(pred, fs)
        te = aweight(target, fs)
    elif preemph == "first_order":
        pe = pre_emphasis(pred, alpha)
        te = pre_emphasis(target, alpha)
    else:
        raise ValueError(f"unknown preemph {preemph!r}; use 'first_order' or 'aweight'")
    loss = esr_weight * esr(pe, te) + (1 - esr_weight) * F.mse_loss(pred, target)
    if stft_weight > 0.0:
        loss = loss + stft_weight * mrstft(pred, target, fft_sizes)
    if dc_weight > 0.0:
        loss = loss + dc_weight * dc_loss(pred, target)
    return loss
