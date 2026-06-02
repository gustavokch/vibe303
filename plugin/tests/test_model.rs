use ba283::model::Ba283Model;

#[test]
fn model_loads_from_embedded_json() {
    let _ = Ba283Model::new_from_embedded_weights();
}

#[test]
fn model_output_is_finite() {
    let mut model = Ba283Model::new_from_embedded_weights();
    let y = model.forward(&[0.123, 0.5]);
    assert!(y[0].is_finite());
}

#[test]
fn reset_restores_fresh_state() {
    // NOTE: deviates from the plan's `peak < 1e-3` form. That assumed a TRAINED
    // model that rests at ~0 for zero input; the placeholder is random, so the
    // correct invariant is: after reset(), the model reproduces a freshly-
    // constructed model's output for the same input sequence (i.e. reset zeroes
    // the hidden state).
    let mut warmed = Ba283Model::new_from_embedded_weights();
    for _ in 0..100 {
        let _ = warmed.forward(&[1.0, 0.5]);
    }
    warmed.reset();

    let mut fresh = Ba283Model::new_from_embedded_weights();
    let probe = [
        [0.0_f32, 0.5],
        [0.2, 0.5],
        [-0.3, 0.5],
        [0.5, 0.5],
        [0.1, 0.5],
    ];
    for x in probe.iter() {
        let a = warmed.forward(x)[0];
        let b = fresh.forward(x)[0];
        assert!((a - b).abs() < 1e-6, "post-reset {a} != fresh {b}");
    }
}
