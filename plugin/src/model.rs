//! Hand-coded pure-Rust stacked LSTM(num_layers x H)+FiLM+Dense(1) matching PyTorch nn.LSTM exactly.
//! Parses the self-describing `ba283-lstm-v2` JSON (see train/export.py).
//! forward() is zero-alloc: fixed `[f32; _]` arrays, no Vec/heap in the audio path.
//! The hidden width `H`, the layer count `L` and the per-layer learned initial hidden/cell state are
//! all read from the JSON (`H <= HIDDEN_MAX`, `L <= LAYERS_MAX`). The NAM-style export is 3 layers of
//! width 18; all configs load into the same fixed storage.

use serde_json::Value;

/// Largest hidden width supported by the fixed-size storage. Covers the 18-wide NAM export with
/// headroom to 32; bump (and rebuild) if a wider model is exported. The two large per-layer weight
/// tensors (`w_ih`/`w_hh`, which dominate the size as `LAYERS_MAX * 4*HIDDEN_MAX * HIDDEN_MAX`) are
/// heap-`Box`ed, so this cap is no longer bounded by the 2 MB test-thread stack — it can grow without
/// the by-value struct overflowing the stack at construction.
const HIDDEN_MAX: usize = 32;
/// Largest stack depth supported by the fixed-size storage (NAM export is 3 layers).
const LAYERS_MAX: usize = 4;
const INPUT: usize = 2; // [audio, gain]
const GATES_MAX: usize = 4 * HIDDEN_MAX; // i, f, g, o stacked
/// Per-layer input-weight column count: layer 0 uses `lstm_input_size` (1|2) columns, layers >= 1
/// take the previous layer's hidden output, so `HIDDEN_MAX` columns cover every layer.
const IN_COLS_MAX: usize = HIDDEN_MAX;

/// Stacked LSTM(num_layers x H) + optional FiLM + Dense(1) with fixed-size weight storage.
///
/// Gate ordering follows PyTorch `nn.LSTM` (`gate_order: "ifgo"`): the `4H` rows of each layer's gate
/// tensors are input i = [0..H], forget f = [H..2H], cell g = [2H..3H], output o = [3H..4H].
///
/// Stacking: layer 0 sees the audio (FiLM, `lstm_input_size = 1`) or `[audio, gain]` (concat,
/// `lstm_input_size = 2`); each subsequent layer takes the previous layer's hidden output (`H`
/// columns). FiLM conditioning (`conditioning: "film"`) applies the `Linear(1, 2H)` affine
/// `(gamma, beta)` to the TOP layer's output before the dense readout; concat feeds gain as layer 0's
/// 2nd input column and has no FiLM layer.
///
/// Each layer carries a learned initial hidden/cell state (`init_h`/`init_c`); `reset()` loads them
/// instead of zeros, matching the trained `h0`/`c0` parameters.
pub struct Ba283Model {
    // Per-layer weights. Unused rows/cols stay 0. Heap-boxed (they dominate the struct size) so
    // HIDDEN_MAX is not bounded by the construction-time stack; indexing through the Box is a plain
    // pointer deref, so `forward`/`gate` stay zero-alloc.
    w_ih: Box<[[[f32; IN_COLS_MAX]; GATES_MAX]; LAYERS_MAX]>, // L x 4H x in_cols
    w_hh: Box<[[[f32; HIDDEN_MAX]; GATES_MAX]; LAYERS_MAX]>,  // L x 4H x H
    b_ih: [[f32; GATES_MAX]; LAYERS_MAX],                // L x 4H
    b_hh: [[f32; GATES_MAX]; LAYERS_MAX],                // L x 4H
    // Learned initial state per layer (loaded on reset()).
    init_h: [[f32; HIDDEN_MAX]; LAYERS_MAX],
    init_c: [[f32; HIDDEN_MAX]; LAYERS_MAX],
    // FiLM layer (film mode only): the [2H,1] weight flattened + bias 2H.
    film_w: [f32; 2 * HIDDEN_MAX],
    film_b: [f32; 2 * HIDDEN_MAX],
    // Dense readout.
    w_dense: [f32; HIDDEN_MAX], // H
    b_dense: f32,
    // Running state per layer.
    h: [[f32; HIDDEN_MAX]; LAYERS_MAX],
    c: [[f32; HIDDEN_MAX]; LAYERS_MAX],
    /// Actual hidden width parsed from the weights JSON (`<= HIDDEN_MAX`).
    hidden: usize,
    /// Actual stack depth parsed from the weights JSON (`<= LAYERS_MAX`).
    num_layers: usize,
    lstm_input_size: usize,
    film: bool,
    /// Training-time input pre-scale (`x[:,0] = dry * input_scale`). Documented in the de-norm
    /// contract; NOT applied inside `forward` (the LSTM weights already bake the trained domain).
    /// Exposed so the host chain can reason about the contract — see `dsp.rs`.
    input_scale: f32,
    /// Reference voltage the training target was divided by (`target = wet_volts / output_ref_v`).
    /// The host chain multiplies `forward()` output by this to recover volts.
    output_ref_v: f32,
    /// Sample rate the model was trained at (Hz); the host chain resamples to/from it.
    sample_rate: u32,
}

#[inline(always)]
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

impl Ba283Model {
    /// Construct from the JSON weights embedded at build time.
    ///
    /// The embedded weights are a build-time invariant; any shape/format
    /// mismatch panics with a clear message rather than failing silently.
    pub fn new_from_embedded_weights() -> Self {
        const WEIGHTS_JSON: &str = include_str!("../weights/ba283_lstm_3x18.json");
        Self::from_json_str(WEIGHTS_JSON)
    }

    /// Construct from a weights JSON file at runtime (offline tools / A-B harnesses that need a
    /// different export than the embedded one). Panics on read or parse failure — callers are dev
    /// tools, not the realtime audio path.
    pub fn from_json_path(path: &str) -> Self {
        let s = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("weights: cannot read {path}: {e}"));
        Self::from_json_str(&s)
    }

    fn from_json_str(s: &str) -> Self {
        let doc: Value = serde_json::from_str(s).expect("embedded weights: invalid JSON");

        let format = doc["format"].as_str().expect("weights: missing `format`");
        assert_eq!(
            format, "ba283-lstm-v2",
            "weights: unexpected format `{format}`"
        );
        // Loud warning so a build embedding the random placeholder export (rather than a real
        // Colab-trained model) can never ship silently — the NN backend would emit garbage.
        if doc["meta"]["placeholder"].as_bool() == Some(true) {
            eprintln!(
                "WARNING: BA283 NN weights are a PLACEHOLDER (meta.placeholder=true) — the \
                 NeuralNet backend output is meaningless until trained weights are embedded."
            );
        }
        let input_size = doc["input_size"]
            .as_u64()
            .expect("weights: missing `input_size`");
        assert_eq!(
            input_size, INPUT as u64,
            "weights: input_size must be {INPUT}"
        );
        let hidden = doc["hidden_size"]
            .as_u64()
            .expect("weights: missing `hidden_size`") as usize;
        assert!(
            (1..=HIDDEN_MAX).contains(&hidden),
            "weights: hidden_size {hidden} out of range 1..={HIDDEN_MAX}"
        );
        let num_layers = doc["num_layers"]
            .as_u64()
            .expect("weights: missing `num_layers`") as usize;
        assert!(
            (1..=LAYERS_MAX).contains(&num_layers),
            "weights: num_layers {num_layers} out of range 1..={LAYERS_MAX}"
        );
        let gates = 4 * hidden;
        let gate_order = doc["gate_order"]
            .as_str()
            .expect("weights: missing `gate_order`");
        assert_eq!(gate_order, "ifgo", "weights: gate_order must be `ifgo`");

        let conditioning = doc["conditioning"].as_str().unwrap_or("concat");
        let film = conditioning == "film";

        // De-norm contract constants (see train/dataset.py + data_gen/stimulus.py). Default to the
        // canonical values if an older weights file omits them.
        let input_scale = doc["input_scale"].as_f64().unwrap_or(2.0) as f32;
        let output_ref_v = doc["output_ref_v"].as_f64().unwrap_or(64.0) as f32;
        let sample_rate = doc["sample_rate"].as_u64().unwrap_or(48_000) as u32;
        let lstm_input_size = doc["lstm_input_size"]
            .as_u64()
            .expect("weights: missing `lstm_input_size`") as usize;
        assert!(
            (1..=LSTM_IN_MAX_CONST).contains(&lstm_input_size),
            "weights: lstm_input_size {lstm_input_size} out of range 1..={LSTM_IN_MAX_CONST}"
        );

        let layers = doc["layers"]
            .as_array()
            .expect("weights: `layers` must be an array");
        let dense = layers
            .iter()
            .find(|l| l["type"].as_str() == Some("dense"))
            .expect("weights: missing `dense` layer");

        let mut model = Self {
            w_ih: Box::new([[[0.0; IN_COLS_MAX]; GATES_MAX]; LAYERS_MAX]),
            w_hh: Box::new([[[0.0; HIDDEN_MAX]; GATES_MAX]; LAYERS_MAX]),
            b_ih: [[0.0; GATES_MAX]; LAYERS_MAX],
            b_hh: [[0.0; GATES_MAX]; LAYERS_MAX],
            init_h: [[0.0; HIDDEN_MAX]; LAYERS_MAX],
            init_c: [[0.0; HIDDEN_MAX]; LAYERS_MAX],
            film_w: [0.0; 2 * HIDDEN_MAX],
            film_b: [0.0; 2 * HIDDEN_MAX],
            w_dense: [0.0; HIDDEN_MAX],
            b_dense: 0.0,
            h: [[0.0; HIDDEN_MAX]; LAYERS_MAX],
            c: [[0.0; HIDDEN_MAX]; LAYERS_MAX],
            hidden,
            num_layers,
            lstm_input_size,
            film,
            input_scale,
            output_ref_v,
            sample_rate,
        };

        // Collect the LSTM layers in declared order, keyed by their `layer` index.
        for l in 0..num_layers {
            let lstm = layers
                .iter()
                .find(|x| {
                    x["type"].as_str() == Some("lstm")
                        && x["layer"].as_u64() == Some(l as u64)
                })
                .unwrap_or_else(|| panic!("weights: missing lstm layer {l}"));

            // Layer 0 input width = lstm_input_size; layers >= 1 take the previous hidden (H).
            let in_cols = if l == 0 { lstm_input_size } else { hidden };

            // weight_ih: 4H x in_cols
            let w_ih = lstm["weight_ih"]
                .as_array()
                .unwrap_or_else(|| panic!("lstm[{l}].weight_ih missing"));
            assert_eq!(w_ih.len(), gates, "lstm[{l}].weight_ih rows must be {gates}");
            for (row, row_v) in w_ih.iter().enumerate() {
                let cols = row_v
                    .as_array()
                    .unwrap_or_else(|| panic!("lstm[{l}].weight_ih row not an array"));
                assert_eq!(
                    cols.len(),
                    in_cols,
                    "lstm[{l}].weight_ih row {row} must have {in_cols} cols"
                );
                for (col, v) in cols.iter().enumerate() {
                    model.w_ih[l][row][col] = as_f32(v, "lstm.weight_ih");
                }
            }

            // weight_hh: 4H x H
            let w_hh = lstm["weight_hh"]
                .as_array()
                .unwrap_or_else(|| panic!("lstm[{l}].weight_hh missing"));
            assert_eq!(w_hh.len(), gates, "lstm[{l}].weight_hh rows must be {gates}");
            for (row, row_v) in w_hh.iter().enumerate() {
                let cols = row_v
                    .as_array()
                    .unwrap_or_else(|| panic!("lstm[{l}].weight_hh row not an array"));
                assert_eq!(
                    cols.len(),
                    hidden,
                    "lstm[{l}].weight_hh row {row} must have {hidden} cols"
                );
                for (col, v) in cols.iter().enumerate() {
                    model.w_hh[l][row][col] = as_f32(v, "lstm.weight_hh");
                }
            }

            // bias_ih / bias_hh: 4H each
            fill_vec(&lstm["bias_ih"], &mut model.b_ih[l], gates, "lstm.bias_ih");
            fill_vec(&lstm["bias_hh"], &mut model.b_hh[l], gates, "lstm.bias_hh");

            // Learned initial state: init_hidden / init_cell, length H each.
            fill_vec(
                &lstm["init_hidden"],
                &mut model.init_h[l],
                hidden,
                "lstm.init_hidden",
            );
            fill_vec(
                &lstm["init_cell"],
                &mut model.init_c[l],
                hidden,
                "lstm.init_cell",
            );
        }

        // FiLM layer: weight [2H, 1] -> flattened 2H, bias [2H].
        if film {
            let f = layers
                .iter()
                .find(|l| l["type"].as_str() == Some("film"))
                .expect("weights: conditioning=film but no `film` layer");
            let fw = f["weight"].as_array().expect("film.weight missing");
            assert_eq!(
                fw.len(),
                2 * hidden,
                "film.weight rows must be {}",
                2 * hidden
            );
            for (row, row_v) in fw.iter().enumerate() {
                let cols = row_v.as_array().expect("film.weight row not an array");
                assert_eq!(cols.len(), 1, "film.weight row {row} must have 1 col");
                model.film_w[row] = as_f32(&cols[0], "film.weight");
            }
            fill_vec(&f["bias"], &mut model.film_b, 2 * hidden, "film.bias");
        }

        // Dense weight: 1 x H
        let d_w = dense["weight"].as_array().expect("dense.weight missing");
        assert_eq!(d_w.len(), 1, "dense.weight must have 1 row");
        let d_row = d_w[0].as_array().expect("dense.weight row not an array");
        assert_eq!(
            d_row.len(),
            hidden,
            "dense.weight row must have {hidden} cols"
        );
        for (i, v) in d_row.iter().enumerate() {
            model.w_dense[i] = as_f32(v, "dense.weight");
        }

        // Dense bias: 1
        let d_b = dense["bias"].as_array().expect("dense.bias missing");
        assert_eq!(d_b.len(), 1, "dense.bias must have 1 element");
        model.b_dense = as_f32(&d_b[0], "dense.bias");

        // Seed the running state from the learned initial state.
        model.reset();
        model
    }

    /// Run one timestep. `x = [audio, gain]`. Zero-alloc: only fixed stack arrays.
    ///
    /// Implements PyTorch nn.LSTM exactly (gate_order `ifgo`) for each stacked layer:
    /// ```text
    /// i  = sigmoid(W_ii x + b_ii + W_hi h + b_hi)
    /// f  = sigmoid(W_if x + b_if + W_hf h + b_hf)
    /// g  = tanh   (W_ig x + b_ig + W_hg h + b_hg)
    /// o  = sigmoid(W_io x + b_io + W_ho h + b_ho)
    /// c' = f * c + i * g
    /// h' = o * tanh(c')
    /// ```
    /// Layer 0's input `x` is the audio (FiLM) or `[audio, gain]` (concat); each later layer's input
    /// is the previous layer's `h'`. FiLM mode then applies `z = (1 + gamma) * h_top + beta` with
    /// `[gamma, beta] = film(gain)`; concat mode uses `z = h_top`. Output is `y = W_dense z + b_dense`.
    pub fn forward(&mut self, x: &[f32; 2]) -> [f32; 1] {
        let hidden = self.hidden;

        // Layer 0 input vector.
        let mut layer_in = [0.0f32; IN_COLS_MAX];
        layer_in[0] = x[0];
        if self.lstm_input_size > 1 {
            layer_in[1] = x[1];
        }
        let mut in_len = self.lstm_input_size;

        for l in 0..self.num_layers {
            let mut new_h = [0.0f32; HIDDEN_MAX];
            for (k, h_slot) in new_h.iter_mut().enumerate().take(hidden) {
                // gate rows (stacked by H): i=[0..H], f=[H..2H], g=[2H..3H], o=[3H..4H]
                let i = sigmoid(self.gate(l, k, &layer_in, in_len));
                let f = sigmoid(self.gate(l, hidden + k, &layer_in, in_len));
                let g = self.gate(l, 2 * hidden + k, &layer_in, in_len).tanh();
                let o = sigmoid(self.gate(l, 3 * hidden + k, &layer_in, in_len));

                let c_new = f * self.c[l][k] + i * g;
                self.c[l][k] = c_new;
                *h_slot = o * c_new.tanh();
            }
            self.h[l] = new_h;

            // The next layer's input is this layer's hidden output (H columns).
            layer_in = [0.0f32; IN_COLS_MAX];
            layer_in[..hidden].copy_from_slice(&self.h[l][..hidden]);
            in_len = hidden;
        }

        // FiLM + dense on the TOP layer's output.
        let top = self.num_layers - 1;
        let g_in = x[1];
        let mut y = self.b_dense;
        for k in 0..hidden {
            let z = if self.film {
                let gamma = self.film_w[k] * g_in + self.film_b[k];
                let beta = self.film_w[hidden + k] * g_in + self.film_b[hidden + k];
                (1.0 + gamma) * self.h[top][k] + beta
            } else {
                self.h[top][k]
            };
            y += self.w_dense[k] * z;
        }

        [y]
    }

    /// Pre-activation gate value for layer `l`, row `row`:
    /// `W_i row · input + b_ih + W_h row · h[l] + b_hh`.
    #[inline(always)]
    fn gate(&self, l: usize, row: usize, input: &[f32; IN_COLS_MAX], in_len: usize) -> f32 {
        let mut acc = self.b_ih[l][row] + self.b_hh[l][row];
        for j in 0..in_len {
            acc += self.w_ih[l][row][j] * input[j];
        }
        for (j, &hv) in self.h[l].iter().enumerate().take(self.hidden) {
            acc += self.w_hh[l][row][j] * hv;
        }
        acc
    }

    /// Reset the hidden and cell state of every layer to the learned initial state.
    pub fn reset(&mut self) {
        self.h = self.init_h;
        self.c = self.init_c;
    }

    /// Training-time input pre-scale from the de-norm contract. NOT applied inside `forward`.
    #[inline]
    pub fn input_scale(&self) -> f32 {
        self.input_scale
    }

    /// Reference voltage to multiply `forward()` output by to recover volts (de-norm contract).
    #[inline]
    pub fn output_ref_v(&self) -> f32 {
        self.output_ref_v
    }

    /// Sample rate (Hz) the model was trained at; the host chain resamples to/from it.
    #[inline]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

/// FiLM uses 1 input column, concat uses 2; the loader validates `lstm_input_size` against this.
const LSTM_IN_MAX_CONST: usize = 2;

#[inline]
fn as_f32(v: &Value, field: &str) -> f32 {
    v.as_f64()
        .unwrap_or_else(|| panic!("{field}: non-numeric value")) as f32
}

fn fill_vec(v: &Value, dst: &mut [f32], len: usize, field: &str) {
    let arr = v
        .as_array()
        .unwrap_or_else(|| panic!("{field} missing or not an array"));
    assert_eq!(arr.len(), len, "{field} must have {len} elements");
    for (i, e) in arr.iter().enumerate() {
        dst[i] = as_f32(e, field);
    }
}
