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


def composite_loss(
    pred: torch.Tensor, target: torch.Tensor, *, esr_weight: float = 1.0, alpha: float = 0.85
) -> torch.Tensor:
    # Default is pure pre-emphasised ESR (esr_weight=1.0), the arvidf/GuitarML
    # recipe. The esr_weight knob keeps the ESR+MSE blend available for A/B; at
    # 1.0 the MSE term vanishes.
    pe = pre_emphasis(pred, alpha)
    te = pre_emphasis(target, alpha)
    return esr_weight * esr(pe, te) + (1 - esr_weight) * F.mse_loss(pred, target)
