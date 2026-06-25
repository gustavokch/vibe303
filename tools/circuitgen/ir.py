"""Load + validate the canonical circuit IR (``circuit/ba283.toml``).

The IR is the single source of truth; the emitters consume the typed objects
returned by :func:`load`. Validation is strict — a malformed IR must fail here,
not produce a silently-wrong backend.

Stdlib only (``tomllib``); run via ``uv run --no-project``.
"""

from __future__ import annotations

import tomllib
from dataclasses import dataclass, field
from pathlib import Path

GROUND = "GND"


@dataclass(frozen=True)
class Node:
    name: str
    spice: str | None = None
    acme: str | None = None
    known: bool = False
    intrinsic_base: bool = False
    role: str | None = None

    def spice_name(self) -> str:
        return self.spice or self.name

    def acme_name(self) -> str:
        return self.acme or self.spice_name()


@dataclass(frozen=True)
class Resistor:
    name: str
    nodes: tuple[str, str]
    ohms: float
    material: str
    param: str | None = None  # build-arg name when a variable leg
    wdf_drop: bool = False  # both-known terminals → inert in WDF MNA (same rule as caps)
    role: str | None = None


@dataclass(frozen=True)
class Capacitor:
    name: str
    nodes: tuple[str, str]
    farads: float
    dielectric: str
    wdf_drop: bool = False  # both-known terminals → inert in WDF MNA
    role: str | None = None


@dataclass(frozen=True)
class Bjt:
    name: str
    model: str
    base: str
    emitter: str
    collector: str
    intrinsic_base: str | None = None


@dataclass(frozen=True)
class Diode:
    name: str
    nodes: tuple[str, str]  # (anode, cathode)
    model: str


@dataclass(frozen=True)
class Jfet:
    name: str
    model: str
    drain: str
    gate: str
    source: str


@dataclass(frozen=True)
class Transformer:
    name: str
    lp: float
    ls: float
    k: float
    pri_nodes: tuple[str, str]
    sec_nodes: tuple[str, str]


@dataclass(frozen=True)
class Model:
    name: str
    type: str
    params: dict[str, float]  # full SPICE .MODEL card keys (lowercase)


@dataclass(frozen=True)
class Circuit:
    meta: dict
    nodes: list[Node]
    resistors: list[Resistor]
    capacitors: list[Capacitor]
    bjts: list[Bjt]
    diodes: list[Diode]
    jfets: list[Jfet]
    transformer: Transformer | None
    models: dict[str, Model]
    params: dict
    source_path: Path

    # --- convenience lookups -------------------------------------------------
    def node(self, name: str) -> Node:
        return self._node_by_name[name]

    def node_index(self, name: str) -> int:
        """WDF enum index (list order). ``GND`` is the MNA reference (-> -1 sentinel
        upstream); callers handle ground explicitly."""
        return self._node_index[name]

    def is_known(self, name: str) -> bool:
        if name == GROUND:
            return True
        return self.node(name).known

    def element_dropped_in_wdf(self, nodes: tuple[str, str]) -> bool:
        """The emitter rule: an element with BOTH terminals on known nodes injects
        no current into the MNA unknowns, so the Rust topology omits it."""
        return self.is_known(nodes[0]) and self.is_known(nodes[1])

    _node_by_name: dict[str, Node] = field(default_factory=dict, repr=False)
    _node_index: dict[str, int] = field(default_factory=dict, repr=False)


def _as_pair(v: list) -> tuple[str, str]:
    if len(v) != 2:
        raise ValueError(f"expected 2 node names, got {v!r}")
    return (str(v[0]), str(v[1]))


def load(path: str | Path) -> Circuit:
    path = Path(path)
    with path.open("rb") as fh:
        raw = tomllib.load(fh)

    nodes = [
        Node(
            name=n["name"],
            spice=n.get("spice"),
            acme=n.get("acme"),
            known=n.get("known", False),
            intrinsic_base=n.get("intrinsic_base", False),
            role=n.get("role"),
        )
        for n in raw["node"]
    ]
    resistors = [
        Resistor(
            name=r["name"],
            nodes=_as_pair(r["nodes"]),
            ohms=float(r["ohms"]),
            material=r["material"],
            param=r.get("param"),
            wdf_drop=r.get("wdf_drop", False),
            role=r.get("role"),
        )
        for r in raw.get("resistor", [])
    ]
    capacitors = [
        Capacitor(
            name=c["name"],
            nodes=_as_pair(c["nodes"]),
            farads=float(c["farads"]),
            dielectric=c["dielectric"],
            wdf_drop=c.get("wdf_drop", False),
            role=c.get("role"),
        )
        for c in raw.get("capacitor", [])
    ]
    bjts = [
        Bjt(
            name=b["name"],
            model=b["model"],
            base=b["base"],
            emitter=b["emitter"],
            collector=b["collector"],
            intrinsic_base=b.get("intrinsic_base"),
        )
        for b in raw.get("bjt", [])
    ]
    diodes = [
        Diode(
            name=d["name"],
            nodes=_as_pair(d["nodes"]),
            model=d["model"],
        )
        for d in raw.get("diode", [])
    ]
    jfets = [
        Jfet(
            name=j["name"],
            model=j["model"],
            drain=j["drain"],
            gate=j["gate"],
            source=j["source"],
        )
        for j in raw.get("jfet", [])
    ]
    xf = raw.get("transformer")
    transformer = (
        None
        if xf is None
        else Transformer(
            name=xf["name"],
            lp=float(xf["lp"]),
            ls=float(xf["ls"]),
            k=float(xf["k"]),
            pri_nodes=_as_pair(xf["pri_nodes"]),
            sec_nodes=_as_pair(xf["sec_nodes"]),
        )
    )
    models = {
        name: Model(
            name=name,
            type=m["type"],
            params={k: float(v) for k, v in m.items() if k != "type"},
        )
        for name, m in raw.get("model", {}).items()
    }

    circuit = Circuit(
        meta=raw["meta"],
        nodes=nodes,
        resistors=resistors,
        capacitors=capacitors,
        bjts=bjts,
        diodes=diodes,
        jfets=jfets,
        transformer=transformer,
        models=models,
        params=raw.get("params", {}),
        source_path=path,
    )
    # frozen dataclass: populate the lookup caches via object.__setattr__.
    object.__setattr__(circuit, "_node_by_name", {n.name: n for n in nodes})
    object.__setattr__(circuit, "_node_index", {n.name: i for i, n in enumerate(nodes)})

    validate(circuit)
    return circuit


def validate(c: Circuit) -> None:
    """Raise ``ValueError`` on any structural defect."""
    errs: list[str] = []

    # node count matches declared NUM_NODES
    declared = c.meta.get("num_nodes")
    if declared is not None and declared != len(c.nodes):
        errs.append(f"meta.num_nodes={declared} but {len(c.nodes)} nodes listed")

    # unique node names
    names = [n.name for n in c.nodes]
    dupes = {x for x in names if names.count(x) > 1}
    if dupes:
        errs.append(f"duplicate node names: {sorted(dupes)}")
    if GROUND in names:
        errs.append(f"'{GROUND}' is the implicit reference; do not list it as a node")

    known_names = set(names) | {GROUND}

    def check_ref(elem: str, node: str) -> None:
        if node not in known_names:
            errs.append(f"{elem} references unknown node '{node}'")

    def check_wdf_drop(kind: str, name: str, nodes: tuple[str, str], flag: bool) -> None:
        """Enforce the WDF emitter rule for ANY two-terminal element (R and C alike):
        an element with BOTH terminals on known nodes injects no current into the MNA
        unknowns, so the Rust topology omits it. The author MUST acknowledge that with
        ``wdf_drop = true`` — otherwise the SPICE/ACME backends keep the element while
        the WDF silently drops it (the exact 3-way drift this harness exists to kill)."""
        both_known = c.element_dropped_in_wdf(nodes)
        if flag and not both_known:
            errs.append(f"{kind} {name} has wdf_drop=true but a terminal is an unknown node")
        if both_known and not flag:
            errs.append(
                f"{kind} {name} spans two known nodes but is missing wdf_drop=true "
                "(it would be silently inert in the WDF while SPICE/ACME keep it)"
            )

    touched: set[str] = set()
    for r in c.resistors:
        for nd in r.nodes:
            check_ref(f"resistor {r.name}", nd)
            touched.add(nd)
        if r.param is None and r.ohms <= 0:
            errs.append(f"resistor {r.name} has non-positive ohms {r.ohms}")
        check_wdf_drop("resistor", r.name, r.nodes, r.wdf_drop)
    for cap in c.capacitors:
        for nd in cap.nodes:
            check_ref(f"capacitor {cap.name}", nd)
            touched.add(nd)
        if cap.farads <= 0:
            errs.append(f"capacitor {cap.name} has non-positive farads {cap.farads}")
        check_wdf_drop("capacitor", cap.name, cap.nodes, cap.wdf_drop)
    for b in c.bjts:
        for term, nd in (("base", b.base), ("emitter", b.emitter), ("collector", b.collector)):
            check_ref(f"bjt {b.name}.{term}", nd)
            touched.add(nd)
        if b.model not in c.models:
            errs.append(f"bjt {b.name} references undefined model '{b.model}'")
        if b.intrinsic_base and b.intrinsic_base not in known_names:
            errs.append(f"bjt {b.name} intrinsic_base '{b.intrinsic_base}' is not a node")
    for d in c.diodes:
        for nd in d.nodes:
            check_ref(f"diode {d.name}", nd)
            touched.add(nd)
        if d.model not in c.models:
            errs.append(f"diode {d.name} references undefined model '{d.model}'")
    for j in c.jfets:
        for term, nd in (("drain", j.drain), ("gate", j.gate), ("source", j.source)):
            check_ref(f"jfet {j.name}.{term}", nd)
            touched.add(nd)
        if j.model not in c.models:
            errs.append(f"jfet {j.name} references undefined model '{j.model}'")
    if c.transformer is not None:
        for nd in (*c.transformer.pri_nodes, *c.transformer.sec_nodes):
            check_ref("transformer", nd)
            touched.add(nd)

    # every non-known, non-intrinsic node must be touched by at least one element
    for n in c.nodes:
        if n.known or n.intrinsic_base:
            continue
        if n.name not in touched:
            errs.append(f"node '{n.name}' is dangling (no element connects to it)")

    if errs:
        raise ValueError(
            "circuit IR validation failed:\n  - " + "\n  - ".join(errs)
        )


if __name__ == "__main__":
    import sys

    p = sys.argv[1] if len(sys.argv) > 1 else "circuit/ba283.toml"
    circ = load(p)
    print(
        f"OK {circ.meta['name']}: {len(circ.nodes)} nodes, "
        f"{len(circ.resistors)} resistors, {len(circ.capacitors)} caps "
        f"({sum(c.wdf_drop for c in circ.capacitors)} wdf-dropped), "
        f"{len(circ.bjts)} bjts, {len(circ.diodes)} diodes, "
        f"{len(circ.jfets)} jfets, "
        f"{'no transformer' if circ.transformer is None else circ.transformer.name}, "
        f"models={list(circ.models)}"
    )
