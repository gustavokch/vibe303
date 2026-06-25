"""circuitgen — generate SPICE / Rust-WDF-topology / ACME backends from one IR.

The canonical circuit lives in ``circuit/ba283.toml``. This package loads and
validates it (``ir``) and emits each backend (``emit_rust``, ``emit_spice``,
``emit_acme``). Stdlib-only (``tomllib``) so it runs via
``uv run --no-project`` without resolving the heavy project deps.
"""
