"""The circuit.toml schema, described for a vision-language model.

Kept in sync with tools/circuitgen/ir.py (the loader) and circuit/ba283.toml (the
reference instance). The VLM is asked to emit exactly this format so the output
drops straight into the circuitgen pipeline.
"""

from __future__ import annotations

SCHEMA_SPEC = r"""
Emit a TOML circuit netlist with EXACTLY these sections (omit nothing required):

[meta]
name           = string         # circuit name
drawing        = string         # source drawing id if visible, else ""
rail_node      = string         # the positive-supply node name
rail_voltage   = float          # supply volts
ground         = "GND"          # always the literal "GND" (implicit reference)
num_nodes      = int            # total [[node]] entries
listening_node = string         # the output/probe node

# One [[node]] per electrical net, in a stable order. GND is implicit — do NOT
# list it. Mark fixed-potential nets known=true (rails, driven inputs).
[[node]]
name = string                   # canonical net name (alnum + underscore)
known = bool                    # optional, default false
role  = string                  # optional: rail|input|listening|...

# Passive/active elements. `nodes = [a, b]` are net names (or "GND").
[[resistor]]
name = string; nodes = [string, string]; ohms = float; material = string
# material ∈ {carbon_comp, carbon_film, metal_film, wirewound}
# optional: role = string ; param = string (if a build-time variable leg)

[[capacitor]]
name = string; nodes = [string, string]; farads = float; dielectric = string
# dielectric ∈ {electrolytic, film, styrene}
# set wdf_drop = true ONLY when BOTH terminals are known nodes (e.g. rail↔GND)

[[bjt]]
name = string; model = string   # model must match a [model.<NAME>] below
base = string; emitter = string; collector = string

[transformer]                   # optional, if a transformer is present
name = string; lp = float; ls = float; k = float
pri_nodes = [string, string]; sec_nodes = [string, string]

# Full SPICE .MODEL card for each transistor model, keys lower-cased.
[model.<NAME>]
type = "npn" | "pnp"
is = float; bf = float; ...     # every parameter legible on the schematic/datasheet

[params]                        # optional analog plumbing constants you can read off
# e.g. pot totals, gain map endpoints

RULES:
- Use net names that reflect the schematic labels where legible; otherwise invent
  stable names (e.g. node connecting R3 and TR1 base -> "TR1_B").
- Every non-known net must touch >= 1 element (no dangling nodes).
- Numbers are plain floats (e.g. 2200.0 or 2.2e3); units are implied by the field.
- Output ONLY the TOML, no prose, no code fences.
"""
