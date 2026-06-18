# Per-PORT ADAA for BA283 — feasibility & decision (all three formulations)

Investigation requested after the **per-residual-term** in-solve ADAA1 spike was measured
NO-GO (`docs/plans/2026-06-17-insolve-adaa-findings.md`). That spike's findings named one
remaining literature-backed lever — **Albertini per-PORT modular ADAA** — and deferred it
as out-of-scope. This doc reads the three relevant papers in full and scores them against
the *actual* BA283 solver, then decides. No code was written.

Papers (local, `docs/adaa/`):
- Albertini, Bernardini, Sarti — **DAFx-2020**, *Antiderivative Antialiasing in Nonlinear
  Wave Digital Filters* (`DAFx2020_paper_35.pdf`).
- Albertini, Bernardini, Sarti — **JAES 69(7/8) 2021**, *Antiderivative Antialiasing
  Techniques in Nonlinear Wave Digital Structures* (`21115.pdf`) — incl. the six-port WD BJT.
- Holters — **DAFx-2019**, *Antiderivative Antialiasing for Stateful Systems*
  (`adaa-for-stateful-systems.pdf`).

---

## The two load-bearing facts

### 1. BA283's solver is MNA, not a Wave Digital Filter

`plugin/src/wdf/mod.rs:1-5` says it directly: the circuit is *"NOT decomposable as a
single-root WDF tree, so this solves the full circuit by nodal MNA + Newton-Raphson — same
method SPICE uses."* Concretely:

- `self.v` is a **node-voltage** vector, not wave variables (`mod.rs:64,72`).
- `gauss_solve_16` solves `J·Δv = −F`; the Jacobian starts as the linear conductance
  matrix `G0` (`newton.rs:38-40`, `build.rs:274-276`).
- The 3 BJTs stamp terminal currents + Jacobian entries into the **shared** 16×16 system
  (`newton.rs:202-246`).
- There are **no** wave variables, adaptors, port resistances, or per-port scattering
  anywhere.

Albertini's method (DAFx-2020 §2-3) is built entirely on WDF machinery — wave variables
`a = v+Zi`, `b = v−Zi`; a Connection Tree of adaptors; a reflection-free port at the
nonlinear root; a scattering matrix `S`. **None of it exists here.** The earlier critique's
claim that "Albertini is BA283's domain almost exactly" assumed a WDF and is therefore
wrong.

### 2. BA283 has three cross-coupled BJTs inside global feedback

Six junctions, a joint nonlinear N-block in the Schur partition (`schur.rs`), all wrapped
in the BA283's DC feedback. **Every published ADAA-for-circuits result handles exactly one
nonlinear element:**

- **DAFx-2020:** one nonlinearity at the CT root. Multiple nonlinearities and multi-port
  transistors are listed as **future work** (§7: "extend … to WD structures with multiple
  nonlinearities … nonlinear blocks with multiple inputs, e.g. transistors").
- **JAES-2021 §6.3:** multi-port (vector) ADAA "is **not straightforward, since an
  integration variable for each vector component needs to be chosen**." Their escape:
  a single BJT's aliasing is "**dominated by certain variables**," so they integrate w.r.t.
  one dominant variable (`â1`) and tabulate 2D LUTs `F_BE,lut`, `F_BC,lut` (eq 40-45,
  Fig. 4). Validated on **one** common-emitter BJT (§7.3).
- **Holters DAFx-2019:** "the limitation to scalar-valued `pₓ(n)` … is **necessary**, as
  [Parker ADAA] is restricted to nonlinear functions with scalar argument" (§3). The
  3-diode Tube-Screamer only works by reducing the nonlinearity to **one scalar** via
  Holters–Zölzer k-d-tree dimensionality reduction (§4.2). Conclusion names lifting the
  scalar-input restriction as the open problem: "if [it] could be generalized to nonlinear
  functions with multiple inputs, the method … would immediately generalize to all stateful
  nonlinear systems."

So the multi-input/vector nonlinearity is the **shared, unsolved wall** of all three
methods. The failed per-term spike was, in effect, an unvalidated multi-input ADAA — the
exact gap the literature leaves open.

### Side-benefit banked

JAES-2021's "aliasing dominated by one variable" is the **same** observation as our own
Stage-0 attribution (transport-exp dominates foldback; depletion secondary,
`2026-06-17-insolve-adaa-findings.md`). That agreement is the one actionable design hint
this investigation yields — see Path 1.

---

## The three paths, scored against the real solver

### Path 1 — Schur-port analog (MNA-native, novel) — least-bad if ever revisited

The Schur complement already reduces the linear L-rows seen from the BJT N-block to an
effective conductance + current source — the MNA dual of a WDF "port"/Thévenin equivalent.
Apply JAES §6.3's **dominant-variable 2D-LUT ADAA** to each junction's transport term at
the converged port: integrate the transport map w.r.t. the dominant aliasing variable
(`vbe`, per Stage-0), hold the rest fixed, precompute `F_BE`/`F_BC` LUTs offline with the
existing Newton, divided-difference at runtime.

| | |
|---|---|
| **Pro** | reuses MNA+Schur, no parallel solver; the dominant-variable trick is the one published lever that may survive multi-port coupling; aligns with our Stage-0 finding. |
| **Con** | novel/unvalidated; the Schur N-block is a **joint** 3-BJT block, *not* an isolated per-device port, so the operating-point coupling that sank the per-term spike is still present; param changes (gain switch / bias / age rebuild `G0` via `set_rgain`/`set_bias`/`set_age`) move the port → LUT regen or parameterization. |
| **Effort / Risk** | medium / high (same multi-input wall, only partially mitigated). |

### Path 2 — Faithful Albertini WDF six-port — literature-pure, prohibitive

Rebuild BA283 as a WDF Connection Tree: a non-reciprocal R-type adaptor absorbing the
topology (plus a nullor for the feedback/opamp-like coupling, JAES §7.2), three six-port WD
BJT models (JAES Fig. 4, eq 34-45), each with 2D-LUT ADAA and delay compensation via the
expanded-period scattering matrix `S̃ = S(T̃s)`, `T̃s = (1+p/2)Ts` (DAFx-2020 eq 8-12).

| | |
|---|---|
| **Pro** | the only fully literature-validated machinery; modular once built. |
| **Con** | a **complete parallel solver** — and BA283 is explicitly *not* WDF-decomposable; no published example does three coupled BJTs (all do one); port resistances `Z_X0`/`Z_XY` are param-dependent → LUT regeneration on every knob. |
| **Effort / Risk** | very large (weeks) / very high (beyond any published result). |

### Path 3 — Holters state-space — cleanest delay-comp, wrong shape

Cast BA283 as a discrete nonlinear state-space (Holters eq 4-7), reduce the nonlinear
argument to a **scalar** `p(n)` via Holters–Zölzer, antialias the *solved* scalar map with
the divided difference (eq 2) + LUT, and delay-compensate by designing `A,b,c,d` at
`f̃s = (2/3)fs` (eq 9-12, the fused state-update form eq 29-30).

| | |
|---|---|
| **Pro** | closest to the MNA trapezoidal-companion state we already keep (`i_prev`, `qbe_prev`); delay-comp is exact and well-defined; cheapest **if** the scalar reduction exists. |
| **Con** | BA283's 3 BJTs at distinct nodes inside feedback do **not** collapse to one scalar argument (unlike the Tube-Screamer's 3 diodes sharing a node). That reduction is the method's hard precondition and is almost certainly unavailable here. Multi-input is Holters' own open problem. |
| **Effort / Risk** | medium-large / very high (precondition likely impossible). |

---

## Decision: NO-GO on all three

For a faithful 3-BJT BA283, every path terminates at the same wall — a multi-port (vector)
nonlinearity inside global feedback, which no literature method covers. Paths 2 and 3 also
demand a from-scratch parallel solver for a circuit whose own source comment says it is not
WDF-decomposable.

If the lever is ever revisited, **Path 1 (Schur-port + dominant-variable 2D-LUT)** is the
single most-promising experimental form: it reuses the existing solver, uses the one
published trick for multi-port BJT ADAA, and is backed by our own Stage-0 attribution. It
is *research*, not a spike, and must clear the **same** go/no-go gate the per-term form
failed: does antialiasing at the collapsed port reduce the os2→os16 alias floor *without*
shifting the DC operating point or exploding Newton cost? Until someone publishes
multi-coupled-nonlinearity ADAA, treat it as open.

**Practical mitigation today remains plain oversampling** (os2→os4 ≈ +10–15 dB at hot
drive, per Stage 1 of the per-term findings), already supported by the tier machinery.

This continues the NO-GO discipline of the per-term spike and Phase 2/3
(`2026-06-15-adaa-phase23-findings.md`). No shipped code, no params, no tier wiring.

---

## Path 1 Stage A — BUILT + MEASURED (2026-06-18) = NO-GO (below GO bar)

Built Stage A of the build plan (`~/.claude/plans/unaudhd-review-critique-and-flickering-river.md`):
the cheap **dominant-variable reduction in the dense path** before any Schur/2D-LUT work — antialiase
**only** the forward base-emitter transport arm per BJT (Stage-0's dominant aliasing variable), holding
the reverse (BC) arm and all cross terms instantaneous. New feature `adaa-port` (off by default, never
shipped), `adaa_port` toggle, one new branch in `newton.rs` reusing `adaa1_exp` +
`compute_bjt_preexp_batch_adaa` (the sole code delta from the per-term form is the `aa_bc` flag gating
the reverse arm). Gate `plugin/tests/adaa_port.rs` = byte-for-byte copy of `adaa_insolve.rs`.

**The dominant-variable reduction worked on the pathologies the per-term form failed:**

- **DC operating point now PRESERVED.** `Δ = 5.6e-9` (per-term form *shifted* the operating point —
  the named root cause of its NO-GO). Not antialiasing the reverse/cross terms stopped the
  feedback fight, exactly as hypothesised.
- **Alias floor now improves HOT instead of degrading.** Converged `Δhi` is **positive in 4 of 5
  hot cells** (+2.1..+7.6 dB) vs the per-term form which made every cell *worse*. Sign inverted.
- **FR bounded, loop stable.** Max |FR shift| = 0.272 dB to 20 kHz (≤0.5 dB target); all gains finite.
- **Floor is formulation-limited, not iter-limited.** `port@30 ≈ port@400` in every cell, so the
  result is the true formulation floor, not a convergence artefact (though hot cells still pin
  `maxIters` at the 400 cap — the joint residual does not fully converge).

**But it misses the GO bar.** Hot headroom closed = **−17.9 % .. 26.4 %** of os2→os16 (peak 26.4 % at
f0=0.30, mean ≈ +12 %, one cell still negative). The gate required **≥ ~30 %** (one OS octave). So:

| gate condition | result | |
|---|---|---|
| hot closure ≥ ~30 % | −17.9 .. 26.4 % (peak 26.4) | ❌ |
| DC Δ ≈ 0 | 5.6e-9 | ✅ |
| iters bounded | `port@30≈port@400` but `maxIters` pins 400 hot | ⚠️ |
| FR ≤ ~0.5 dB, finite | 0.272 dB, all finite | ✅ |

**Decision: NO-GO; do NOT build Stage B (Schur-port 2D-LUT).** The residual that caps Stage A at
~26 % is the **joint 3-BJT coupling** — the operating-point cross-talk that survives *because the port
is shared*, not per-device (the Con flagged in Path 1's table above). Stage B antialiases at the
**same joint Schur N-block**, so it inherits the same coupling ceiling; a faithful 2D-LUT at that port
is very unlikely to clear 30 % when the dense dominant-variable form — identical antialiasing math —
caps here. And even at its 26 % peak the alias reduction is a few dB, below the **+10–15 dB** plain
os2→os4 already delivers hot. The expected Stage-B prize does not justify the 2D-LUT + Schur-hook build.

This confirms the feasibility verdict empirically: the dominant-variable trick is the right lever (it
fixed the per-term DC/FR/sign failures) but the multi-coupled-nonlinearity wall caps the recoverable
headroom below one octave. **Mitigation stays oversampling.** Code is feature-gated, off by default,
never compiled into the shipped plugin; golden `test_wdf` 22/22, default + `--no-default-features`
builds clean, no `adaa_port` symbol leak.
