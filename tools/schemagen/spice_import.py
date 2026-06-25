"""SPICE-deck → draft circuit IR (a deterministic, non-vision front-end).

The SINA image front-end (``vlm.py``) digitises *schematics*. When a circuit
already exists as a text SPICE deck (e.g. ``vibe303/netlist.txt``), parsing it is
deterministic and lossless — no VLM needed. This module reads a flat ``.cir``/
``.net`` deck (R, C, L, D, Q, J, V/I sources, ``.MODEL``, ``.ic``) and emits a
``circuit/<name>.draft.toml`` for the circuitgen pipeline. The draft is always
human-reviewed before promotion (set materials/dielectrics, mark driven inputs,
pick rail/listening nodes), exactly like the VLM path.

Stdlib only; run via ``uv run --no-project python -m tools.schemagen.spice_import``.
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass, field

# SPICE engineering suffixes (lower-cased; ``meg``/``mil`` handled before ``m``).
_SUFFIX = {"f": 1e-15, "p": 1e-12, "n": 1e-9, "u": 1e-6, "m": 1e-3,
           "k": 1e3, "g": 1e9, "t": 1e12}
_NUM = r"[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?"

# SPICE .MODEL type -> IR model type.
_MODEL_TYPE = {"d": "d", "npn": "npn", "pnp": "pnp", "njf": "njf"}


def parse_value(tok: str) -> float:
    """Parse a SPICE numeric+suffix token (``220k``, ``14.2P``, ``1.00E-08``, ``-.5``)."""
    m = re.fullmatch(rf"({_NUM})\s*([a-zA-Z]*)", tok.strip())
    if not m:
        raise ValueError(f"cannot parse SPICE value {tok!r}")
    num = float(m.group(1))
    suf = m.group(2).lower()
    if not suf:
        return num
    if suf.startswith("meg"):
        return num * 1e6
    if suf.startswith("mil"):
        return num * 25.4e-6
    return num * _SUFFIX.get(suf[0], 1.0)


def _net(name: str) -> str:
    """Canonicalise a SPICE net name: ``0`` -> ``GND``; digit-leading -> ``n``-prefixed
    (so it is a legal Julia/Rust identifier downstream)."""
    if name == "0":
        return "GND"
    return f"n{name}" if name[0].isdigit() else name


@dataclass
class Deck:
    name: str
    nodes: list[str] = field(default_factory=list)      # first-seen order, GND excluded
    known: set[str] = field(default_factory=set)        # source-driven nets
    src_value: dict[str, float] = field(default_factory=dict)
    resistors: list[tuple[str, str, str, float]] = field(default_factory=list)  # name,a,b,ohms
    capacitors: list[tuple[str, str, str, float]] = field(default_factory=list)
    bjts: list[tuple[str, str, str, str, str]] = field(default_factory=list)    # name,c,b,e,model
    diodes: list[tuple[str, str, str, str]] = field(default_factory=list)        # name,a,k,model
    jfets: list[tuple[str, str, str, str, str]] = field(default_factory=list)    # name,d,g,s,model
    models: dict[str, tuple[str, dict[str, float]]] = field(default_factory=dict)
    ic: dict[str, float] = field(default_factory=dict)

    def touch(self, *nets: str) -> None:
        for n in nets:
            if n != "GND" and n not in self.nodes:
                self.nodes.append(n)


def parse_netlist(text: str, name: str) -> Deck:
    d = Deck(name=name)
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line[0] in "*;":
            continue
        low = line.lower()
        if low.startswith(".model"):
            _parse_model(line, d)
            continue
        if low.startswith(".ic"):
            _parse_ic(line, d)
            continue
        if line[0] == ".":  # other directives (.tran/.wave/.param/.end) — ignored
            continue
        _parse_element(line, d)
    return d


def _parse_model(line: str, d: Deck) -> None:
    m = re.match(r"\.model\s+(\S+)\s+(\w+)\s*\((.*)\)\s*$", line, re.I | re.S)
    if not m:
        raise ValueError(f"bad .MODEL line: {line!r}")
    mname, mtype, body = m.group(1), m.group(2).lower(), m.group(3)
    if mtype not in _MODEL_TYPE:
        raise ValueError(f".MODEL {mname}: unsupported type {mtype!r}")
    params: dict[str, float] = {}
    for kv in re.findall(r"(\w+)\s*=\s*([^\s]+)", body):
        params[kv[0].lower()] = parse_value(kv[1])
    d.models[mname] = (_MODEL_TYPE[mtype], params)


def _parse_ic(line: str, d: Deck) -> None:
    for node, val in re.findall(rf"V\((\w+)\)\s*=?\s*({_NUM})", line, re.I):
        d.ic[_net(node)] = float(val)


def _parse_element(line: str, d: Deck) -> None:
    tok = line.split()
    name, kind = tok[0], tok[0][0].upper()
    if kind in ("R", "C", "L"):
        a, b = _net(tok[1]), _net(tok[2])
        d.touch(a, b)
        val = parse_value(tok[3])
        if kind == "R":
            d.resistors.append((name, a, b, val))
        elif kind == "C":
            d.capacitors.append((name, a, b, val))
        # L (uncoupled inductor) is unsupported by the IR; skip with a marker net touch
    elif kind == "Q":
        c, b, e = _net(tok[1]), _net(tok[2]), _net(tok[3])
        d.touch(c, b, e)
        d.bjts.append((name, c, b, e, tok[4]))
    elif kind == "D":
        a, k = _net(tok[1]), _net(tok[2])
        d.touch(a, k)
        d.diodes.append((name, a, k, tok[3]))
    elif kind == "J":
        dr, g, s = _net(tok[1]), _net(tok[2]), _net(tok[3])
        d.touch(dr, g, s)
        d.jfets.append((name, dr, g, s, tok[4]))
    elif kind in ("V", "I"):
        np_, nn = _net(tok[1]), _net(tok[2])
        d.touch(np_, nn)
        for n in (np_, nn):
            if n != "GND":
                d.known.add(n)
        # capture a numeric DC value for np_ if present (skip {expr} sources)
        rest = " ".join(tok[3:])
        mv = re.search(rf"\bDC\b\s*({_NUM})", rest, re.I)
        if mv:
            d.src_value[np_] = float(mv.group(1))
    else:
        raise ValueError(f"unsupported element {name!r} (kind {kind})")


# --- TOML emission -----------------------------------------------------------
def _f(v: float) -> str:
    return repr(v)


def to_toml(d: Deck, *, rail_node: str | None = None, listening_node: str | None = None) -> str:
    rails = [n for n in d.nodes if n in d.known and n in d.src_value]
    if rail_node is None and rails:
        rail_node = max(rails, key=lambda n: d.src_value[n])  # highest DC = the supply
    out: list[str] = []
    out.append("# DRAFT circuit IR — generated by tools/schemagen/spice_import.py.")
    out.append("# Review before promotion: set materials/dielectrics, mark driven inputs,")
    out.append("# confirm rail/listening nodes, then move to circuit/<name>.toml.\n")
    out.append("[meta]")
    out.append(f'name = "{d.name}"')
    out.append('ground = "GND"')
    if rail_node:
        out.append(f'rail_node = "{rail_node}"')
        if rail_node in d.src_value:
            out.append(f"rail_voltage = {_f(d.src_value[rail_node])}")
    out.append(f"num_nodes = {len(d.nodes)}")
    if listening_node:
        out.append(f'listening_node = "{listening_node}"')
    out.append("")

    for n in d.nodes:
        out.append("[[node]]")
        out.append(f'name = "{n}"')
        if n in d.known:
            out.append("known = true")
            role = "rail" if n == rail_node else "input"
            out.append(f'role = "{role}"')
        out.append("")

    for name, a, b, ohms in d.resistors:
        out += ["[[resistor]]", f'name = "{name}"', f'nodes = ["{a}", "{b}"]',
                f"ohms = {_f(ohms)}", 'material = "carbon_film"', ""]
    for name, a, b, far in d.capacitors:
        # heuristic: bulk values (>=1uF) are electrolytic, small values film.
        diel = "electrolytic" if far >= 1e-6 else "film"
        out += ["[[capacitor]]", f'name = "{name}"', f'nodes = ["{a}", "{b}"]',
                f"farads = {_f(far)}", f'dielectric = "{diel}"', ""]
    for name, c, b, e, model in d.bjts:
        out += ["[[bjt]]", f'name = "{name}"', f'model = "{model}"',
                f'base = "{b}"', f'emitter = "{e}"', f'collector = "{c}"', ""]
    for name, a, k, model in d.diodes:
        out += ["[[diode]]", f'name = "{name}"', f'nodes = ["{a}", "{k}"]',
                f'model = "{model}"', ""]
    for name, dr, g, s, model in d.jfets:
        out += ["[[jfet]]", f'name = "{name}"', f'model = "{model}"',
                f'drain = "{dr}"', f'gate = "{g}"', f'source = "{s}"', ""]

    for mname, (mtype, params) in d.models.items():
        out.append(f"[model.{mname}]")
        out.append(f'type = "{mtype}"')
        for k, v in params.items():
            out.append(f"{k} = {_f(v)}")
        out.append("")

    if d.ic:
        out.append("[params]")
        for node, val in d.ic.items():
            out.append(f"ic_{node} = {_f(val)}  # initial condition V({node})")
        out.append("")
    return "\n".join(out) + "\n"


def main(argv: list[str]) -> int:
    if not argv:
        print("usage: python -m tools.schemagen.spice_import <deck> --name <name> "
              "[--out <path>] [--rail <node>] [--listen <node>]", file=sys.stderr)
        return 2
    deck_path = argv[0]
    opts = dict(zip(argv[1::2], argv[2::2]))
    name = opts.get("--name", "circuit")
    with open(deck_path) as fh:
        d = parse_netlist(fh.read(), name)
    toml = to_toml(d, rail_node=opts.get("--rail"), listening_node=opts.get("--listen"))
    out = opts.get("--out")
    if out:
        with open(out, "w") as fh:
            fh.write(toml)
        print(f"wrote {out}: {len(d.nodes)} nodes, {len(d.resistors)} R, "
              f"{len(d.capacitors)} C, {len(d.bjts)} Q, {len(d.diodes)} D, "
              f"{len(d.jfets)} J, models={list(d.models)}", file=sys.stderr)
    else:
        sys.stdout.write(toml)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
