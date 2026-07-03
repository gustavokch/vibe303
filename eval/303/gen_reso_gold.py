"""Generate the committed ngspice VCF resonance fixture (F6.1).

Wraps ``run_vcf303_spice.py``'s ``--reso`` machinery: at the Rreso ~47 kΩ Q
optimum (VCF_MAP §11), sweep the bias current I_tail ∈ {16,32,64,128} µA and
record the resonance peak frequency at each — the fc-tracking map the Ultra Q9
feedback (F6.3) must reproduce. Writes ``vcf303_reso_gold.json``.

    uv run --no-project --with PySpice --with numpy python eval/303/gen_reso_gold.py
"""

from __future__ import annotations

import json
import sys
import argparse
from pathlib import Path

from run_vcf303_spice import ac_response, resonance_peak
from parallel import run_parallel_with_fallback

RRESO_OPT = 47e3  # the §11 Q optimum (stock loop self-limits, no self-oscillation)
ITAILS_UA = [16, 32, 64, 128]


def main() -> int:
    parser = argparse.ArgumentParser(description="Generate VCF reso gold.")
    parser.add_argument("-w", "--workers", type=int, default=4,
                        help="Number of workers for parallel execution")
    args = parser.parse_args()

    peak_hz: dict[str, float] = {}
    peak_db: dict[str, float] = {}
    
    def eval_ua(ua):
        f, mag = ac_response(ua * 1e-6, RRESO_OPT)
        pk, fpk = resonance_peak(f, mag)
        return ua, pk, fpk
        
    results = run_parallel_with_fallback(eval_ua, ITAILS_UA, desc="gen_reso_gold", max_workers=args.workers)
    
    for ua, pk, fpk in results:
        peak_hz[str(ua)] = round(fpk, 1)
        peak_db[str(ua)] = round(pk, 2)
        print(f"I_tail={ua:4d}uA  peak {pk:6.2f}dB @ {fpk:8.1f}Hz")

    gold = {
        "_source": "eval/303/gen_reso_gold.py (ngspice-46, vcf303.toml, Rreso=47k)",
        "rreso_opt": RRESO_OPT,
        "peak_hz_by_itail_ua": peak_hz,
        "reso_peak_db_by_itail_ua": peak_db,
    }
    out = Path(__file__).parent / "vcf303_reso_gold.json"
    out.write_text(json.dumps(gold, indent=2) + "\n")
    print(f"wrote {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
