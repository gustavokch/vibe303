"""circuitgen — generate SPICE / Rust-WDF-topology / ACME backends from one IR.

Each circuit IR lives in ``circuit/<name>.toml`` (e.g. ``vco303``, ``vcf303``).
This package loads and validates it (``ir``) and emits each backend
(``emit_rust``, ``emit_spice``, ``emit_acme``). Stdlib-only (``tomllib``) so it
runs via ``uv run --no-project`` without resolving the heavy project deps.
"""
