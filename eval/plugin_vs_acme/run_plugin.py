"""
Drive the Rust ``ba283_standalone`` (NeuralNet backend, full chain) on a dry
stimulus and return its output, with the de-norm contract handled here so the
comparison against the ACME ground truth is in **volts**.

The de-norm contract (see ``plugin/src/dsp.rs::INPUT_FS_VOLTS``):
  - 0 dBFS ≡ ``INPUT_FS_VOLTS`` (0.5 V) at the circuit input.
  - The plugin reads a WAV sample ``x`` and feeds the model ``x`` directly
    (``input_scale`` cancels the 0.5 V/dBFS factor), so to present the dry
    stimulus (volts, peak ≤ 0.5) the way the model trained, we write the WAV as
    ``dry / INPUT_FS_VOLTS`` (= the dBFS-domain signal, peak ≤ 1.0).
  - The plugin output is dBFS; ``plugin_volts = out_dBFS * INPUT_FS_VOLTS``.

So ``plugin_volts`` should equal the circuit's wet volts up to model accuracy —
which is exactly what nails the ×128 de-norm (``output_ref_v / INPUT_FS_VOLTS``).
"""
from __future__ import annotations

import subprocess
import tempfile
from pathlib import Path

import numpy as np
import soundfile as sf

# Mirror the Rust constant `dsp::INPUT_FS_VOLTS`.
INPUT_FS_VOLTS = 0.5

REPO = Path(__file__).resolve().parents[2]
DEFAULT_BIN = REPO / "target/release/ba283_standalone"
# The real Colab-trained 48-wide export (the embedded weights are a placeholder).
DEFAULT_WEIGHTS = Path.home() / "Downloads/ba283_lstm32.json"


def run_plugin(
    dry_volts: np.ndarray,
    sr: int,
    *,
    backend: str = "nn",
    bias: float,
    circuit_gain: float | None = None,
    level_norm: bool = False,
    weights: Path | None = None,
    standalone: Path | None = None,
    timeout: float = 600.0,
) -> np.ndarray:
    """Run the plugin full chain on *dry_volts* with the chosen backend.

    Args:
        dry_volts:    mono dry stimulus in volts (peak ≤ 0.5), 96 kHz.
        sr:           sample rate (Hz); use 96 kHz so no oversampling / latency.
        backend:      ``"nn"`` (NeuralNet) or ``"wdf"`` (circuit MNA solver).
        bias:         the plugin ``--bias`` value. For ``nn`` this is the model gain
                      conditioning (``gain_norm`` ∈ [0,1]); for ``wdf`` it is the RV1
                      wiper ∈ [0,1] (the physical bias trim).
        circuit_gain: ``wdf`` only — the K↔J gain shunt R_KJ in ohms.
        level_norm:   if True, pass ``--level-norm`` (RMS-match output to input).
        weights:      ``nn`` only — weights JSON (defaults to the trained 48-wide export).

    Returns:
        If ``level_norm`` is False: plugin output in **volts** (``out_dBFS * 0.5``),
        for a like-for-like compare against the ACME wet volts. The de-norm differs
        per backend but both land in the same 0 dBFS ≡ 0.5 V scale, so the ``× 0.5``
        recovery is uniform.
        If ``level_norm`` is True: plugin output in **dBFS** (compare to the input
        dBFS = ``dry_volts / 0.5``).
    """
    binp = Path(standalone) if standalone else DEFAULT_BIN
    if not binp.exists():
        raise RuntimeError(
            f"missing standalone binary: {binp}\n"
            "build it: cargo build -p ba283 --release --bin ba283_standalone"
        )

    dry_volts = np.ascontiguousarray(np.asarray(dry_volts, dtype=np.float32))
    # Present the dry signal in the dBFS domain (0 dBFS ≡ 0.5 V) the chain expects.
    in_dbfs = dry_volts / INPUT_FS_VOLTS

    with tempfile.TemporaryDirectory(prefix="plugin_run_") as _tmp:
        tmp = Path(_tmp)
        in_wav, out_wav = tmp / "in.wav", tmp / "out.wav"
        sf.write(in_wav, in_dbfs, int(sr), subtype="FLOAT")

        cmd = [
            str(binp),
            "--input", str(in_wav),
            "--output", str(out_wav),
            "--backend", backend,
            "--bias", repr(float(bias)),
        ]
        if backend == "wdf":
            if circuit_gain is None:
                raise ValueError("wdf backend needs circuit_gain (R_KJ ohms)")
            cmd += ["--circuit-gain", repr(float(circuit_gain))]
        else:  # nn
            wpath = Path(weights) if weights else DEFAULT_WEIGHTS
            if wpath.exists():
                cmd += ["--weights", str(wpath)]
        if level_norm:
            cmd += ["--level-norm"]

        res = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        if res.returncode != 0:
            raise RuntimeError(f"standalone failed (rc={res.returncode}):\n{res.stderr}")
        if not out_wav.exists():
            raise RuntimeError(f"standalone wrote no output:\n{res.stdout}\n{res.stderr}")

        out, _ = sf.read(out_wav, always_2d=False)
        out = np.asarray(out, dtype=np.float64)

    return out if level_norm else out * INPUT_FS_VOLTS
