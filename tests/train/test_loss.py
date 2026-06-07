import torch

from train.loss import composite_loss, esr, pre_emphasis


def test_pre_emphasis_high_passes_signal():
    sr = 96_000
    t = torch.arange(sr) / sr
    low = torch.sin(2 * 3.14159 * 50 * t).unsqueeze(0).unsqueeze(-1)
    high = torch.sin(2 * 3.14159 * 10_000 * t).unsqueeze(0).unsqueeze(-1)
    low_e = pre_emphasis(low, alpha=0.85)
    high_e = pre_emphasis(high, alpha=0.85)
    assert high_e.pow(2).mean() > 10 * low_e.pow(2).mean()


def test_esr_zero_when_pred_equals_target():
    t = torch.randn(2, 100, 1)
    assert esr(t, t).item() < 1e-6


def test_composite_loss_defaults_to_pure_preemph_esr():
    # Default esr_weight=1.0 (arvidf/GuitarML recipe): composite_loss must equal
    # ESR on the pre-emphasised signals, with no MSE contribution.
    pred = torch.randn(2, 100, 1)
    target = torch.randn(2, 100, 1)
    expected = esr(pre_emphasis(pred, 0.85), pre_emphasis(target, 0.85))
    assert torch.allclose(composite_loss(pred, target), expected)


def test_composite_loss_decreases_when_pred_improves():
    target = torch.randn(2, 100, 1)
    bad = target + torch.randn_like(target)
    good = target + 0.01 * torch.randn_like(target)
    l_bad = composite_loss(bad, target).item()
    l_good = composite_loss(good, target).item()
    assert l_good < l_bad
