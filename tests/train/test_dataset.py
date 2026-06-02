from pathlib import Path

import torch

from train.dataset import BA283Dataset, make_loader

MANIFEST = Path("tests/fixtures/manifest_tiny.csv")


def test_dataset_returns_correct_shapes():
    ds = BA283Dataset(MANIFEST, window=4096, stride=2048)
    x, y = ds[0]
    assert x.shape == (4096, 2)
    assert y.shape == (4096, 1)
    assert x.dtype == torch.float32
    assert y.dtype == torch.float32


def test_bias_constant_across_window():
    ds = BA283Dataset(MANIFEST, window=4096, stride=2048)
    x, _ = ds[0]
    bias_col = x[:, 1]
    assert torch.all(bias_col == bias_col[0])


def test_dataset_covers_all_files_and_biases():
    ds = BA283Dataset(MANIFEST, window=4096, stride=2048)
    seen_bias = set()
    for i in range(len(ds)):
        x, _ = ds[i]
        seen_bias.add(round(float(x[0, 1]), 4))
    assert seen_bias == {0.0, 0.5, 1.0}


def test_input_scale_applied():
    ds = BA283Dataset(MANIFEST, window=4096, stride=2048)
    x, _ = ds[0]
    audio = x[:, 0]
    assert 0.7 < audio.abs().max().item() < 0.9


def test_make_loader_yields_batches():
    loader = make_loader(
        MANIFEST, batch_size=4, window=4096, stride=2048, num_workers=0, shuffle=False
    )
    batch_x, batch_y = next(iter(loader))
    assert batch_x.shape == (4, 4096, 2)
    assert batch_y.shape == (4, 4096, 1)
