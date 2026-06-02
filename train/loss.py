# train/loss.py
import torch
import torch.nn.functional as F


def pre_emphasis(x: torch.Tensor, alpha: float = 0.85) -> torch.Tensor:
    return x[:, 1:, :] - alpha * x[:, :-1, :]


def esr(pred: torch.Tensor, target: torch.Tensor) -> torch.Tensor:
    return ((pred - target) ** 2).sum() / ((target ** 2).sum() + 1e-8)


def composite_loss(
    pred: torch.Tensor, target: torch.Tensor, *, esr_weight: float = 0.75, alpha: float = 0.85
) -> torch.Tensor:
    pe = pre_emphasis(pred, alpha)
    te = pre_emphasis(target, alpha)
    return esr_weight * esr(pe, te) + (1 - esr_weight) * F.mse_loss(pred, target)
