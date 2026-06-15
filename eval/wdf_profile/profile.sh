#!/usr/bin/env bash
# WDF hot-path profiler — profile-first optimization round (Phase 1)
#
# Profiles the Rust WDF full-chain (ba283_standalone --backend wdf) for the
# three quality tiers. Outputs flat self-time tables and a caller-tree snippet
# for the top function. Uses `perf record/report` (task-clock, software event —
# works under perf_event_paranoid=2, no root required).
#
# Usage:
#   ./eval/wdf_profile/profile.sh [--dry-dir PATH] [--out DIR] [--tiers TIERS]
#
# Outputs: PER_TIER.report.txt, caller_tree_uf.txt in --out dir.
#
# Tip: perf.data files are large (800MB–1.5GB each).  Run with --hz 250 or
# lower if /tmp space is tight; the flat table is stable at 250–4000 Hz.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
DRY_DIR="${DRY_DIR:-$HOME/GoogleDrive/ba283_spice/dry}"
OUT_DIR="${OUT_DIR:-$REPO/eval/wdf_profile/out}"
HZ=999
TIERS="${TIERS:-ultra-fast fast}"   # skip realtime by default (saves space+time)
PERF_STORE="${PERF_STORE:-/tmp/wdf_perf_scratch}"

# ---- arg parse ----
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-dir)  DRY_DIR="$2";  shift 2 ;;
    --out)      OUT_DIR="$2";  shift 2 ;;
    --tiers)    TIERS="$2";    shift 2 ;;
    --hz)       HZ="$2";       shift 2 ;;
    *)          echo "unknown arg: $1"; exit 1 ;;
  esac
done

BIN="$REPO/target/profiling-thin/ba283_standalone"
if [[ ! -x "$BIN" ]]; then
  # NOTE: RUSTFLAGS REPLACES .cargo/config.toml's rustflags (it does not merge), so we MUST
  # re-add target-cpu=x86-64-v3 here — else `mul_add` falls back to the libm `fma()` software
  # routine (shows up as a fake ~25% `fma_with_fma`/`fma` on the fma=true tiers) and `floor`
  # becomes a libm call instead of `vroundsd`. The shipped plugin always has v3, so profiling
  # without it mis-attributes the hot path.
  echo "Build first:  RUSTFLAGS='-C target-cpu=x86-64-v3 -C force-frame-pointers=yes' cargo build --profile profiling-thin -p ba283 --bin ba283_standalone"
  exit 1
fi

mkdir -p "$OUT_DIR" "$PERF_STORE"

# ---- Build a ~10-15 s profiling stimulus (avoids /tmp space explosion) ----
STIM="$PERF_STORE/profile_stim.wav"
export STIM
if [[ ! -f "$STIM" ]]; then
  echo "[profile] building stimulus..."
  uv run python3 - <<'PYEOF'
import soundfile as sf, numpy as np, os, sys
dry = os.environ.get("DRY_DIR", os.path.expanduser("~/GoogleDrive/ba283_spice/dry"))
stim = os.environ.get("STIM")
srcs = ["sweep_m3_dry.wav", "music3_08 - Shed - Rave Echoes_m6_dry.wav", "pink_m3_s0_dry.wav"]
chunks, sr_ref = [], None
for s in srcs:
    path = os.path.join(dry, s)
    if not os.path.exists(path): continue
    d, sr = sf.read(path, dtype="float32")
    sr_ref = sr_ref or sr
    chunks.append(d)
if not chunks: sys.exit("no dry stims found in " + dry)
cat = np.concatenate(chunks[:2], axis=0)   # ~2 stims = ~15 s
sf.write(stim, cat, sr_ref)
print(f"[profile] stimulus: {len(cat)/sr_ref:.1f}s @ {sr_ref}Hz")
PYEOF
fi

export DRY_DIR STIM

# ---- Record + report each tier ----
for tier in $TIERS; do
  safe="${tier//-/_}"
  data="$PERF_STORE/perf_${safe}.data"
  echo "[profile] recording tier=$tier  (F=$HZ Hz) ..."
  perf record -e task-clock -F "$HZ" --call-graph dwarf \
    -o "$data" -- \
    "$BIN" --backend wdf --tier "$tier" \
           --input "$STIM" --output "$PERF_STORE/out_${safe}.wav" 2>&1 | grep -v '^$'
  echo "[profile] reporting tier=$tier ..."
  perf report --stdio --no-children -i "$data" 2>/dev/null \
    | grep -E '^\s+[0-9]' > "$OUT_DIR/${safe}.report.txt"
  echo "  top-10:"
  head -10 "$OUT_DIR/${safe}.report.txt"
done

# ---- Caller tree for newton_iterate (ultra-fast if present) ----
if [[ -f "$PERF_STORE/perf_ultra_fast.data" ]]; then
  perf report --stdio --no-children -i "$PERF_STORE/perf_ultra_fast.data" \
    --symbol-filter newton_iterate 2>/dev/null \
    | head -60 > "$OUT_DIR/caller_tree_uf.txt"
fi

echo "[profile] done. Reports in $OUT_DIR/"
