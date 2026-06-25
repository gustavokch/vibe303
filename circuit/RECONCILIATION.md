# BA283 three-backend reconciliation (Phase 0)

Element-by-element diff of the three current circuit definitions, performed before
authoring `circuit/ba283.toml`. Result: **the three are in sync today**; the only
divergence is justified and becomes an emitter rule.

Sources:
- SPICE: `data_gen/netlist.py` (`build_netlist`, `_TRANSISTOR_MODELS`, `_LO1166_SUBCKT`)
- Rust WDF: `plugin/src/wdf/build.rs` (`build_with`) + `plugin/src/wdf/components.rs` (`bc184c`/`q2n3055`/`BjtParams`) + node enum in `plugin/src/wdf/mod.rs`
- Julia ACME: `eval/acme_vs_spice/ba283_acme.jl` (`build_circuit`)

## Resistors (17) — all match
R_esr_C7=1Ω, R1=2k2, R3=68k, R2=56k, R4=1k2, R_KJ=*param*, R_esr_Cgain=1Ω,
R5=3k3, R8=33k, R6=18k, R7=47Ω, RV1_upper=*param*, RV1_lower=*param*,
R_pri=14.5Ω, R_sec=44Ω, R_term=600Ω, R_zobel=1k5. Identical nodes + values in all three.

## Capacitors — match, except C6
C1=10µF, Cgain=80µF, C4=100µF, C3=4700pF, C2=220pF, C5=330pF, Czobel=0.01µF: identical.

**C6 (80µF rail decoupling, A→GND): present in SPICE + ACME, ABSENT in WDF.**
Justified: node A is a *known* (fixed 24V rail) node and GND is the reference, so a
capacitor between two fixed potentials injects zero current into the unknown MNA
system — electrically inert in the WDF. → **Emitter rule: the Rust topology emitter
drops any element whose BOTH terminals are `known` nodes.** C6 is the only element
that hits this rule (Czobel ZOBEL→A, R8 A→TR1_C, C3 TR1_C→GND all have one unknown
terminal and are kept).

## BJT models — WDF set is fully derivable from the SPICE `.MODEL` card
The full Gummel-Poon `.MODEL` card is the canonical superset. Each backend projects:
- **SPICE**: verbatim card.
- **ACME**: `is, βf, βr, vaf, var, ikf, ise, ηe(NE), isc, ηc(NC), rb, re, rc` (only the keys ACME's `bjt()` supports and the card provides).
- **Rust `BjtParams`**: `is, bf, br, vaf, var, ikf` direct; `cje/vje/mje, cjc/vjc/mjc, tf, tr` direct; derived `alpha_f=bf/(bf+1)`, `alpha_r=br/(br+1)`, `vcrit=vt·ln(vt/(√2_trunc·is))` with `√2_trunc=1.41421356`, dep-charge sat consts via `dep_sat_consts`; constants `vt=0.02585`, `ikr=0`. `vjc` falls back to SPICE default `0.75` when the card omits it (2N3055).

Verified bit-for-bit: BC184C CJE=1.40E-11→14e-12, alpha_f=600/601=0.99834, alpha_r=0.1/1.1=0.0909; 2N3055 alpha_r=2/3=0.66667, CJE=5.802E-10, vjc=0.75 (default), var=0 (card has no VAR). All consistent with `components.rs`.

## Transformer (LO1166) — match
Lp=1.9H, Ls=5.36H, K=0.9995 in all three. Winding DCRs R_pri/R_sec are ordinary
resistors (already in the resistor list); the coupled-inductor companion (`gl`) is the
only transformer-specific construct. Primary winding spans PA↔A, secondary SA↔GND.

## Nodes — 1:1 mapping (WDF enum order 0..20, `NUM_NODES=21`)
| idx | WDF enum | SPICE/ACME name | flags |
|--|--|--|--|
|0|N_A|A|known (rail, 24V)|
|1|N_INPUT|INPUT|known (source)|
|2|N_C1_MID|C1_MID||
|3|N_R1_MID|R1_MID||
|4|N_TR1_B|TR1_B||
|5|N_TR1_BI|—|intrinsic base (RB-off → pinned/known)|
|6|N_TR1_E|TR1_E||
|7|N_TR1_C|TR1_C|= TR2 external base|
|8|N_TR2_BI|—|intrinsic base (pinned)|
|9|N_J|J||
|10|N_CGAIN_MID|Cgain_MID||
|11|N_TR2_E|TR2_E|= TR3 external base|
|12|N_TR3_BI|—|intrinsic base (pinned)|
|13|N_TR3_E|TR3_E||
|14|N_RV1_W|RV1_WIPER||
|15|N_F|F|TR2/TR3 collector / xfmr primary in|
|16|N_C4_MID|C4_MID||
|17|N_PA|pa|xfmr primary internal (after R_pri)|
|18|N_SA|sa / SEC_W|xfmr secondary internal (before R_sec)|
|19|N_SEC_IN|SEC_IN|listening point|
|20|N_ZOBEL|ZOBEL_MID||

Node enum ORDER is load-bearing: `build.rs` assigns `urow` unknown indices in enum
order, fixing the MNA matrix layout. The IR lists nodes in this exact order and the
Rust emitter must preserve it for bit-exactness.

## CI wiring (manual — workflow edits are hook-guarded)

The drift guard runs as `tests/circuitgen/test_no_drift.py` and as
`python -m tools.circuitgen --check`. Add this step to `.github/workflows/ci.yml`
(right after `actions/checkout`, before the Rust build) — the editing of workflow
files is intentionally gated by a security-reminder hook, so it is left as a
reviewed manual edit:

```yaml
      - name: Circuit-IR drift guard
        # stdlib-only (Python 3.11+ tomllib); committed SPICE/Rust/ACME backends
        # must match circuit/ba283.toml. Regenerate: `python -m tools.circuitgen`.
        run: python3 -m tools.circuitgen --check
```

## Conclusion
No latent drift exists today. The IR captures the superset (incl. C6 and full BJT
cards); per-backend projection + the "drop both-known elements" rule reproduce each
current backend exactly. The drift-guard test then makes future divergence impossible.
