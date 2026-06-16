"""Reamp the NAM T3K sweep through the BA283 SPICE circuit at a few gains.

Drives ``simulate_one`` directly (not run_gain_batch) so the wet npz is tagged
with the normalized gain knob in [0,1] -- which ``resample.py`` parses as
``gain_value`` (the FiLM conditioning input) -- instead of the raw R_KJ ohms.
"""
import multiprocessing as mp
import os
import time
from pathlib import Path

from data_gen.netlist import BIAS_NORM_FIXED, gain_norm_to_rkj
from data_gen.run_spice import simulate_one

DRY96 = Path("scratch/nam_sweep/dry96/T3K-60s.wav")
WET_RAW = Path("scratch/nam_sweep/wet_raw")
GAINS = [0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0]
STEM = "T3K-60s"

# Optional override: GEN_GAINS="0.375,0.5" runs just those gains in this
# invocation. Lets the reamp be split into RAM-safe batches (one fresh process
# per batch) when ngspice's ~2.1 GB output buffer is contended by other load,
# without editing the committed 9-point GAINS default. Unset -> full GAINS.
_env_gains = os.environ.get("GEN_GAINS")
if _env_gains:
    GAINS = [float(x) for x in _env_gains.split(",") if x.strip()]


def _one(g: float):
    rkj = gain_norm_to_rkj(g)
    out = WET_RAW / f"{STEM}_gain{g:.3f}.npz"
    t0 = time.time()
    try:
        simulate_one(DRY96, BIAS_NORM_FIXED, out, gain_rkj=rkj)
        return (g, round(rkj, 1), round(time.time() - t0, 1), None)
    except Exception as exc:  # noqa: BLE001
        return (g, round(rkj, 1), round(time.time() - t0, 1), f"{type(exc).__name__}: {exc}")


if __name__ == "__main__":
    WET_RAW.mkdir(parents=True, exist_ok=True)
    print(f"reamp {STEM} @ bias={BIAS_NORM_FIXED} gains={GAINS}", flush=True)
    with mp.Pool(processes=1, maxtasksperchild=1) as pool:
        for g, rkj, dt, err in pool.map(_one, GAINS):
            print(f"gain_norm={g:.3f} R_KJ={rkj}ohm wall={dt}s {'OK' if err is None else err}", flush=True)
    print("done", flush=True)
