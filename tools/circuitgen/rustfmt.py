"""Shared float -> Rust/Python/Julia literal helpers.

A Rust ``f64`` literal must parse to the IDENTICAL IEEE-754 bits as the value in
the IR for bit-exactness. Python's ``repr(float)`` emits the shortest decimal
that round-trips to the same f64; any conforming parser (Rust/Julia) recovers the
same bits. We only normalise the textual form so it is a valid literal.
"""

from __future__ import annotations


def rust_f64(x: float) -> str:
    """Round-trip-exact Rust f64 literal."""
    s = repr(float(x))
    # repr always yields '.', 'e', 'inf', or 'nan'; reject the non-finite cases.
    if "inf" in s or "nan" in s:
        raise ValueError(f"non-finite value {x!r} cannot be a literal")
    return s


# Python and Julia accept the same numeric forms repr() produces, so the float
# helper is shared; named aliases keep the emitter call sites self-documenting.
py_f64 = rust_f64
julia_f64 = rust_f64
