import torch

from train.loss import (
    PREEMPH_ALPHA_48K,
    aweight,
    composite_loss,
    dc_loss,
    esr,
    pre_emphasis,
)


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


def test_composite_loss_isolates_pure_preemph_esr():
    # With dc_weight=0 (DC term defaults ON at 0.1) and esr_weight=1.0, composite_loss
    # must equal ESR on the pre-emphasised signals -- no MSE, no DC contribution.
    # alpha read from the function default (the 48 kHz matched-z 0.8613), not a literal.
    pred = torch.randn(2, 100, 1)
    target = torch.randn(2, 100, 1)
    expected = esr(
        pre_emphasis(pred, PREEMPH_ALPHA_48K),
        pre_emphasis(target, PREEMPH_ALPHA_48K),
    )
    assert torch.allclose(composite_loss(pred, target, dc_weight=0.0), expected)


def test_dc_loss_penalises_offset():
    target = torch.randn(2, 200, 1)
    assert dc_loss(target + 0.3, target).item() > 0.0
    # A zero-mean error has no DC component -> negligible DC loss.
    zero_mean = target.clone()
    zero_mean[:, ::2] += 0.1
    zero_mean[:, 1::2] -= 0.1
    assert dc_loss(zero_mean, target).item() < 1e-3


def test_dc_weight_adds_term():
    pred = torch.randn(2, 100, 1)
    target = torch.randn(2, 100, 1)
    base = composite_loss(pred, target, dc_weight=0.0)
    with_dc = composite_loss(pred, target, dc_weight=0.5)
    expected = base + 0.5 * dc_loss(pred, target)
    assert torch.allclose(with_dc, expected)


def test_aweight_is_bandpass():
    # A-weighting attenuates both lows (50 Hz) and extreme highs (20 kHz) relative to
    # its ~2-4 kHz peak band. Tones long enough that the FIR is not edge-dominated.
    sr = 48_000
    t = torch.arange(sr) / sr

    def energy(freq):
        tone = torch.sin(2 * 3.14159 * freq * t).unsqueeze(0).unsqueeze(-1)
        return aweight(tone, sr).pow(2).mean().item()

    peak = energy(3_000)
    assert energy(50) < 0.5 * peak
    assert energy(20_000) < 0.5 * peak


def test_composite_loss_aweight_finite_and_zero_at_match():
    x = torch.randn(2, 4096, 1)
    assert composite_loss(x, x, preemph="aweight", dc_weight=0.0).item() < 1e-5
    loss = composite_loss(torch.randn(2, 4096, 1), torch.randn(2, 4096, 1), preemph="aweight")
    assert torch.isfinite(loss)


def test_composite_loss_decreases_when_pred_improves():
    target = torch.randn(2, 100, 1)
    bad = target + torch.randn_like(target)
    good = target + 0.01 * torch.randn_like(target)
    l_bad = composite_loss(bad, target).item()
    l_good = composite_loss(good, target).item()
    assert l_good < l_bad
