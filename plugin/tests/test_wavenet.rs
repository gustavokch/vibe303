//! WaveNet (`model.cond.nam`) loading, shape, determinism, and golden parity against the PyTorch
//! reference (`eval/nam_vs_acme/run_nam.py::_run_conditioned`, captured into
//! `tests/fixtures/wavenet_golden.json`). The parity test is the load-bearing correctness gate for
//! the flat weight ordering + FiLM math: if the Rust streaming forward drifts from the reference, it
//! fails here.

use ba283::wavenet::WaveNet;
use serde_json::Value;

#[test]
fn embedded_loads_with_expected_topology() {
    let net = WaveNet::new_from_embedded();
    assert_eq!(net.receptive_field(), 6347, "receptive field");
    assert_eq!(net.num_layers(), 23, "layer count");
    assert_eq!(net.input_scale(), 2.0, "audio input scale");
    assert_eq!(net.output_ref_v(), 64.0, "output ref voltage");
    assert_eq!(net.sample_rate(), 48_000, "sample rate");
}

#[test]
fn output_is_finite() {
    let mut net = WaveNet::new_from_embedded();
    for i in 0..8192 {
        let x = (i as f32 * 0.01).sin() * 0.3;
        let y = net.process(x, 0.5);
        assert!(y.is_finite(), "non-finite output at sample {i}: {y}");
    }
}

#[test]
fn reset_restores_zero_history() {
    let mut net = WaveNet::new_from_embedded();
    // First pass from a clean state.
    let mut first = Vec::new();
    for i in 0..2048 {
        let x = (i as f32 * 0.013).sin() * 0.4;
        first.push(net.process(x, 0.3));
    }
    // Disturb state, then reset and re-run — must reproduce the first pass exactly.
    for i in 0..500 {
        net.process((i as f32).cos(), 0.9);
    }
    net.reset();
    for (i, &want) in first.iter().enumerate() {
        let x = (i as f32 * 0.013).sin() * 0.4;
        let got = net.process(x, 0.3);
        assert_eq!(got, want, "reset did not restore deterministic output at {i}");
    }
}

#[test]
fn golden_parity_with_pytorch_reference() {
    let doc: Value =
        serde_json::from_str(include_str!("fixtures/wavenet_golden.json")).expect("fixture JSON");
    let gain = doc["gain"].as_f64().unwrap() as f32;
    let dry: Vec<f32> = doc["dry"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap() as f32)
        .collect();
    let wet: Vec<f32> = doc["wet"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap() as f32)
        .collect();
    assert_eq!(dry.len(), wet.len());
    // Fixture's RF must match the engine (same topology).
    assert_eq!(
        doc["receptive_field"].as_u64().unwrap() as usize,
        WaveNet::new_from_embedded().receptive_field()
    );

    // The reference front-pads RF-1 zeros, so its first RF outputs reflect a pad-driven warmup of the
    // dilated-conv state. The streaming engine starts from zero rings (the plugin self-warms as the
    // host streams silence), so the two only become directly comparable once every conv tap reads real
    // signal — i.e. from sample index RF onward. There, the forward must agree to f32 precision; any
    // weight-ordering / FiLM / tap-orientation error would break this bit-exactness, not just shift a
    // transient. The pre-RF region is a warmup convention, not a correctness claim.
    let rf = WaveNet::new_from_embedded().receptive_field();
    assert!(dry.len() > rf + 256, "fixture too short for a steady window");

    let mut net = WaveNet::new_from_embedded();
    let mut max_err = 0.0f32;
    for (i, (&x, &want)) in dry.iter().zip(&wet).enumerate() {
        let got = net.process(x, gain);
        if i < rf {
            continue; // warmup region — not compared (see comment above)
        }
        let err = (got - want).abs();
        max_err = max_err.max(err);
        assert!(
            err < 1e-4,
            "steady-state sample {i}: rust {got} vs torch {want} (err {err}) exceeds 1e-4"
        );
    }
    // f32-vs-f32 across 23 layers is effectively bit-exact in steady state.
    assert!(
        max_err < 1e-5,
        "max steady-state parity error {max_err} exceeds 1e-5 — weight ordering / FiLM math suspect"
    );
    eprintln!("wavenet golden parity (steady state): max abs err = {max_err:e}");
}
