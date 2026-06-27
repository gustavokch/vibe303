# ACME.jl project environment (shared)

This directory is the **Julia [ACME.jl](https://hsu-ant.github.io/ACME.jl/dev/)
project environment** used by the 303 ACME validators under `eval/303/`
(`vco303_acme.jl`, `vcf303_acme.jl`). It holds only the env:

- `Project.toml` / `Manifest.toml` — the pinned ACME.jl dependency set.

The per-circuit ACME value modules (`eval/303/_vco303_gen.jl`,
`eval/303/_vcf303_gen.jl`) are generated from the circuit IR by
`tools/circuitgen` (drift-guarded); the hand-written `*_acme.jl` drivers live
beside them in `eval/303/`.

## Prerequisites

Julia (installed per-user via [juliaup](https://github.com/JuliaLang/juliaup)):

```bash
curl -fsSL https://install.julialang.org | sh -s -- --yes   # installs to ~/.juliaup
julia --project=eval/acme_vs_spice -e 'import Pkg; Pkg.add("ACME"); Pkg.instantiate()'
```

## Usage

The 303 ACME scripts reference this project explicitly, e.g.:

```bash
julia --project=eval/acme_vs_spice eval/303/vco303_acme.jl 5 48000 0.15
```

> **Note (history):** this project was created for a BA283 preamp ACME-vs-SPICE
> feasibility spike. That product was removed when the repo became the standalone
> 303 project; only the ACME project env was kept (the 303 reuses it). The ACME
> parity verdict for both 303 sub-circuits is NO-GO — ngspice is the ground truth.
> See `eval/303/VCF_MAP.md` §12 and `eval/303/compare_vcf303.py`.
