# train/loss.py
import torch
import torch.nn.functional as F


def pre_emphasis(x: torch.Tensor, alpha: float = 0.85) -> torch.Tensor:
    return x[:, 1:, :] - alpha * x[:, :-1, :]


def esr(pred: torch.Tensor, target: torch.Tensor, eps: float = 1e-3) -> torch.Tensor:
    # ESR is error_energy / target_energy. ``eps`` floors the denominator: as the
    # target energy -> 0 the ratio (and its backward num/denom**2 term) blows up
    # to a non-finite gradient. The dataset already drops near-silent windows
    # (see train.dataset.MIN_TARGET_ENERGY); this floor is a second line of
    # defense, sized for the normalized (~unit) target domain so it is negligible
    # for energetic windows.
    return ((pred - target) ** 2).sum() / ((target ** 2).sum() + eps)


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
    alpha: float = 0.85,
    stft_weight: float = 0.0,
    fft_sizes: tuple[int, ...] = _MRSTFT_FFT_SIZES,
) -> torch.Tensor:
    # Default is pure pre-emphasised ESR (esr_weight=1.0, stft_weight=0.0), the
    # arvidf/GuitarML recipe -- backward-compatible. ``esr_weight`` keeps the
    # ESR+MSE blend available for A/B; ``stft_weight`` adds the multi-resolution
    # spectral term (raw, un-pre-emphasised, on the normalised output domain) to
    # cover the harmonic structure ESR misses.
    pe = pre_emphasis(pred, alpha)
    te = pre_emphasis(target, alpha)
    loss = esr_weight * esr(pe, te) + (1 - esr_weight) * F.mse_loss(pred, target)
    if stft_weight > 0.0:
        loss = loss + stft_weight * mrstft(pred, target, fft_sizes)
    return loss
