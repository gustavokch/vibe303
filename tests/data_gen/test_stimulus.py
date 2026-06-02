# tests/data_gen/test_stimulus.py
import numpy as np
import pytest
import soundfile as sf

from data_gen.stimulus import (
    SAMPLE_RATE,
    build_synthetic_segments,
    load_dry,
    load_user_music,
    log_sine_sweep,
    multisine_schroeder,
    pink_noise_bursts,
    two_tone_imd,
)


def test_sample_rate_is_96k():
    assert SAMPLE_RATE == 96_000


def test_log_sine_sweep_peak_amplitude():
    sweep = log_sine_sweep(duration_s=0.5, f_start=100.0, f_end=1000.0, level_db=-6.0)
    assert sweep.dtype == np.float32
    assert sweep.shape == (int(0.5 * SAMPLE_RATE),)
    target_peak = 10 ** (-6.0 / 20.0)
    assert np.max(np.abs(sweep)) == pytest.approx(target_peak, rel=0.05)


def test_pink_noise_bursts_alternates_loud_and_silent():
    bursts = pink_noise_bursts(duration_s=2.0, burst_ms=200, gap_ms=800, peak_db=-6.0)
    assert np.any(np.abs(bursts) > 0.01)
    assert np.any(np.abs(bursts) < 1e-6)


def test_two_tone_has_two_frequencies():
    sig = two_tone_imd(duration_s=0.5, f1=200.0, f2=300.0, level_db=-6.0)
    spec = np.abs(np.fft.rfft(sig))
    freqs = np.fft.rfftfreq(len(sig), 1 / SAMPLE_RATE)
    top_two = freqs[np.argsort(spec)[-2:]]
    assert {round(f, -1) for f in top_two} == {200.0, 300.0}


def test_multisine_schroeder_low_crest_factor():
    sig = multisine_schroeder(duration_s=0.5, num_tones=20, f_low=100.0, f_high=5000.0)
    crest = np.max(np.abs(sig)) / np.sqrt(np.mean(sig**2))
    assert crest < 3.0


def test_load_dry_resamples_and_downmixes(tmp_path):
    src_sr = 44_100
    dur = 0.1
    t = np.arange(int(dur * src_sr)) / src_sr
    left = 0.8 * np.sin(2 * np.pi * 440.0 * t)
    right = 0.4 * np.sin(2 * np.pi * 440.0 * t)
    stereo = np.stack([left, right], axis=1).astype(np.float32)
    path = tmp_path / "dry.wav"
    sf.write(str(path), stereo, src_sr, subtype="FLOAT")

    samples, sr = load_dry(path)

    assert sr == 96_000
    assert samples.ndim == 1
    assert samples.dtype == np.float32
    expected_len = len(stereo) * 96_000 / src_sr
    assert abs(len(samples) - expected_len) <= 5


def test_load_user_music_concatenates_with_silence(tmp_path):
    src_sr = 48_000
    dur = 0.1
    t = np.arange(int(dur * src_sr)) / src_sr
    paths = []
    for i in range(2):
        ch = 0.7 * np.sin(2 * np.pi * 330.0 * t)
        stereo = np.stack([ch, 0.5 * ch], axis=1).astype(np.float32)
        p = tmp_path / f"music{i}.wav"
        sf.write(str(p), stereo, src_sr, subtype="FLOAT")
        paths.append(p)

    out = load_user_music(paths)

    assert out.ndim == 1
    assert out.dtype == np.float32
    assert np.max(np.abs(out)) == pytest.approx(0.5, rel=0.05)

    clip_len = int(dur * src_sr) * 96_000 / src_sr
    silence_len = int(0.05 * 96_000)
    expected_len = 2 * clip_len + silence_len
    assert abs(len(out) - expected_len) <= 10


def test_build_synthetic_segments_multi_file_and_levels():
    segs = build_synthetic_segments()
    # enough distinct files for an 80/10/10 split
    assert len(segs) >= 9
    # all float32, all peak <= 0.5 (SPICE input ceiling), all non-empty
    for name, sig in segs.items():
        assert sig.dtype == np.float32, name
        assert sig.size > 0, name
        assert np.max(np.abs(sig)) <= 0.5 + 1e-6, name
    # levels actually differ (level-dependent nonlinearity coverage)
    peaks = {name: float(np.max(np.abs(sig))) for name, sig in segs.items()}
    assert max(peaks.values()) > 3 * min(v for v in peaks.values() if v > 0)


def test_pink_noise_reproducible():
    a = pink_noise_bursts(1.0, 200, 800, -6.0)
    b = pink_noise_bursts(1.0, 200, 800, -6.0)
    assert np.array_equal(a, b)

    c = pink_noise_bursts(1.0, 200, 800, -6.0, seed=1)
    d = pink_noise_bursts(1.0, 200, 800, -6.0, seed=2)
    assert not np.array_equal(c, d)
