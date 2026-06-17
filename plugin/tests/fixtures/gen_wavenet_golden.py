#!/usr/bin/env python3
"""Regenerate tests/fixtures/wavenet_golden.json — the PyTorch reference vector the Rust WaveNet
streaming engine is checked against in tests/test_wavenet.rs::golden_parity_with_pytorch_reference.

Loads the conditioned WaveNet (`exported_models/model.cond.nam`, in-repo) via the `nam`
fork, renders a deterministic dry clip at a fixed gain using the SAME inner-net call as
eval/nam_vs_acme/run_nam.py::_run_conditioned, and stores the NORMALIZED net output (pre output_ref_v).

Padding note: both the audio AND the gain channel are zero-padded by `RF-1` (run_nam pads gain with
the constant gain, but the plugin starts from all-zero history, so we zero-pad both — the Rust test
only compares the post-RF steady-state region anyway, where the forward is bit-exact).

Run from the ba283-vst3 repo root with the eval venv (has torch + the nam fork):
    .venv/bin/python plugin/tests/fixtures/gen_wavenet_golden.py
"""
import json
from pathlib import Path

import numpy as np
import torch
from nam.models import init_from_nam

# In-repo canonical export (this file: plugin/tests/fixtures → repo root via parents[3]).
NAM = Path(__file__).resolve().parents[3] / "exported_models/model.cond.nam"
OUT = Path(__file__).with_name("wavenet_golden.json")
GAIN = 0.5
N = 8000

d = json.loads(NAM.read_text())
meta = d.get("metadata", {})
cond = meta.get("condition", {})
audio_input_scale = float(cond.get("audio_input_scale", meta.get("input_scale", 1.0)))
gain_channel = int(cond.get("gain_channel", 1))
output_ref_v = float(meta.get("output_ref_v", 64.0))

wrapper = init_from_nam(json.loads(NAM.read_text()))
net = wrapper._net if hasattr(wrapper, "_net") else wrapper
rf = int(net.receptive_field)
print("RF", rf, "audio_input_scale", audio_input_scale, "gain_channel", gain_channel)

rng = np.random.default_rng(283)
t = np.arange(N)
dry = (
    0.3 * np.sin(2 * np.pi * 110 * t / 48000)
    + 0.2 * np.sin(2 * np.pi * 1500 * t / 48000)
    + 0.05 * rng.standard_normal(N)
).astype(np.float32)

audio = np.concatenate([np.zeros(rf - 1, np.float32), dry * np.float32(audio_input_scale)])
gain_ch = np.concatenate(
    [np.zeros(rf - 1, np.float32), np.full(N, np.float32(GAIN), np.float32)]
)  # zero pad matches the plugin's zero-history start
inp = np.empty((2, rf - 1 + N), np.float32)
inp[0 if gain_channel != 0 else 1] = audio
inp[gain_channel] = gain_ch
with torch.no_grad():
    wet = net(torch.from_numpy(inp).unsqueeze(0)).squeeze().numpy().astype(np.float64)[:N]
print("wet len", len(wet), "pk", float(np.max(np.abs(wet))))

OUT.write_text(
    json.dumps(
        {
            "gain": GAIN,
            "audio_input_scale": audio_input_scale,
            "output_ref_v": output_ref_v,
            "receptive_field": rf,
            "n": N,
            "dry": [float(v) for v in dry],
            "wet": [float(v) for v in wet],  # normalized net output (pre output_ref_v)
        }
    )
)
print("wrote", OUT)
