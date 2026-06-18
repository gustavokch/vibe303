# Plan: os=4 oversampling option (alias-reduction, opt-in)

## Context

Heavy-drive aliasing in the WDF backend is real and measurable: the `alias_sweep`
harness reads a floor of **−22 dB moderate / −10.5 dB hot** at the shipped os=2, and the
`headroom_probe` shows **~30.76 dB recoverable** at hot drive by oversampling further
(os2→os16), with **~15 dB recoverable in the first octave (os2→os4)**. The aliasing
originates as in-solve harmonic foldback from the BJT transport exponentials.

The in-solve ADAA route to fix this *without* oversampling is a proven dead end —
per-term ADAA destroys Newton convergence (avgIters ~100–200 vs ~9, output diverges),
and the per-port Albertini reformulation is out of scope (BA283 is MNA not WDF, with a
vector nonlinearity that is the shared unsolved wall of the literature). See
[[ba283-insolve-adaa]] and `docs/plans/2026-06-17-insolve-adaa-findings.md`.

**That leaves plain oversampling as the only viable alias-reduction lever today.** This
plan adds an opt-in **os=4** rate. Expected payoff: **+10…15 dB lower alias floor at hot
drive**. Expected cost: **~2× the WDF solve work of os=2** (twice the Newton solves per
host sample) → opt-in only, not a new default. WDF backend only — the NeuralNet backend
ignores the oversampling param.

## Current architecture (what we extend)

- **Oversampling param** `Oversampling { Auto, X1, X2 }` — host-facing enum (`lib.rs:337`),
  mirrored by `wdf::Oversampling` (`config.rs:129`), folded into `QualityConfig` via
  `with_oversampling` (`config.rs:247`): `X1`→os=1/no-decimate, `X2`→os=2/decimate,
  `Auto`→tier default. `os` is a plain config field (`config.rs:54`); the solve loop reads
  `self.os` dynamically (`process/mod.rs`, `capture_os_raw`), so a higher os needs **no
  array resizing** in the solver.
- **Internal decimation** = a 4th-order Butterworth biquad pair `dec1`/`dec2`
  (`build.rs:269-270`, Q=0.541/1.307) at `internal_sr = host × os`. The `alias_sweep`
  already established that at os=2 this decimator is **not** the limiter (4th ≈ 8th ≈
  raw2x — foldback-limited). **But [[ba283-insolve-adaa]] warns this Butterworth
  self-aliases past 4:1**, so at os=4 it may leak the new images between host-Nyquist and
  2×host-Nyquist back in-band.
- **Clean halfband already exists**: `Oversample2x` — a 63-tap Kaiser polyphase halfband
  (`oversample.rs:126`), currently used only by the HostBridge (host ≥ 72 kHz). Its
  `downsample` is a proven 2:1 anti-alias decimator we can cascade.

## Approach — measurement-first, two stages

### Stage 1 — Add X4 with the existing decimator + measure (cheap)

1. Add `Oversampling::X4` to both enums (`lib.rs:337`, `config.rs:129`) and the param
   mapping (`lib.rs:347 to_wdf`), and a `with_oversampling` arm:
   `X4 => Self { os: 4, decimate: true, ..self }` (`config.rs:250`).
2. No solver changes needed — the `0..self.os` upsample loop and Newton path already run
   at arbitrary os. The biquad decimator gets reconfigured at `internal_sr = host×4`.
3. **Measure** in `alias_sweep.rs`: add an os=4 column (alias floor via the *shipped*
   biquad decimator) and, critically, a **clean-reference os=4 column** decimated with the
   `headroom_probe` Blackman-Harris sinc. The gap between them quantifies how much the
   biquad self-aliases at 4:1.
   - If the biquad column ≈ the sinc reference (foldback still dominates, as at os=2) →
     **ship Stage 1 as-is**; the decimator is good enough.
   - If the biquad column is materially worse (self-aliasing leaks) → proceed to Stage 2.
4. **Profile CPU** (`eval/wdf_profile/`) at os=4 realtime: confirm it stays within the
   realtime budget on the x86-64-v3 floor; record the multiplier vs os=2 (~2× expected).

### Stage 2 — Proper 4:1 decimation (only if Stage 1 measures biquad self-aliasing)

Replace the single biquad cascade with a **two-stage halfband** for os=4: decimate-by-2
(4×→2×) then decimate-by-2 (2×→1×), reusing the proven `Oversample2x` 63-tap Kaiser
halfband `downsample` for each stage. Gate by os: os=2 keeps the existing biquad path
(bit-identical, no regression); os=4 uses the halfband cascade. Update reported latency to
include the added FIR group delay (two 63-tap halfbands ≈ +31 base-rate samples each at
their stage rate; compute exact and reflect in the plugin's reported latency).

## Files

- `plugin/src/lib.rs` — `Oversampling` enum + `to_wdf` (param wiring; the EnumParam at
  `lib.rs:573` picks up the new variant automatically).
- `plugin/src/wdf/config.rs` — `Oversampling` enum + `with_oversampling` arm.
- `plugin/src/wdf/build.rs` — decimator setup (Stage 2: halfband cascade for os=4).
- `plugin/src/oversample.rs` — (Stage 2) expose/cascade `Oversample2x::downsample` for the
  internal 4:1 path.
- `plugin/tests/alias_sweep.rs` — os=4 column + clean-sinc reference column.
- GUI: the OS popup (top bar, see [[ba283-gui-signal-flow]]) gains the X4 entry via the
  EnumParam — verify the label renders.
- `CHANGELOG.md` — note the new opt-in rate.

## Verification

- **Alias win:** `cargo test --features measure --release --test alias_sweep -- --nocapture`
  shows os=4 alias floor ≈ **+10…15 dB lower than os=2 at hot drive**; biquad-vs-sinc gap
  reported (the Stage-1/Stage-2 decision number).
- **No regression at os=2/Auto:** the default (`Auto`) and `X2` paths stay **bit-identical**
  (golden `test_wdf`), since only the new os=4 arm changes behaviour.
- **CPU:** `eval/wdf_profile/` realtime report at os=4 — confirm realtime-safe; record the
  ~2× multiplier so the param is documented as a quality/CPU trade.
- **Latency:** the latency/probe test reflects the os=4 decimation group delay (Stage 2).
- **Build:** default + `--no-default-features` + all-targets clean, no warnings.

## Out of scope

os=8/16 (diminishing returns vs CPU; os=16 ≈ +30 dB but ~8× cost), any in-solve ADAA
(dead end, do not retry — [[ba283-insolve-adaa]]), and NeuralNet-backend oversampling
(the learned model has its own aliasing characteristics, unrelated).
