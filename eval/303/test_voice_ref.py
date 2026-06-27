import numpy as np
from voice_ref_spice import voice_reference, note_to_freq


def test_note_mapping_matches_rust():
    # voice_ref must use the SAME equal-tempered map as Rust midi_to_freq
    # (A4=note69=440). Plan's "note 57 = A2 ~110 Hz" comment is off by an octave:
    # note 57 = 220 Hz. The reference is pitched to whatever the Rust voice plays,
    # else the ESR compare is meaningless.
    assert abs(note_to_freq(69) - 440.0) < 0.01
    assert abs(note_to_freq(57) - 220.0) < 0.01
    assert abs(note_to_freq(45) - 110.0) < 0.01


def test_reference_is_pitched_and_nonsilent():
    sr = 48_000.0
    y = voice_reference(note=57, cutoff_itail=32e-6, rreso=47e3, sr=sr, secs=0.2)
    assert y.size > 0 and np.max(np.abs(y)) > 1e-4
    # Dominant spectral peak near the played pitch (note 57 = 220 Hz). The linear AD
    # VCA contour piles energy below ~50 Hz (its own spectrum); mask it so we test the
    # tone, not the envelope — both ref and Rust voice carry the same contour, so it
    # cancels in the ESR compare regardless.
    f = np.fft.rfftfreq(y.size, 1 / sr)
    mag = np.abs(np.fft.rfft(y))
    mag[f < 50.0] = 0.0
    peak = f[np.argmax(mag)]
    assert 205.0 < peak < 235.0
