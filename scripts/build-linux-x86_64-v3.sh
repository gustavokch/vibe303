#!/usr/bin/env bash
#
# Build + bundle + install TB303 (VST3 + CLAP) for linux-x86_64-v3 (AVX2).
#
# x86-64-v3 microarchitecture level requires AVX, AVX2, BMI1/2, FMA, F16C,
# LZCNT, MOVBE. Runs on Haswell (2013) and newer Intel / Excavator+ AMD.
# Older CPUs will SIGILL — build the generic target instead if unsure.
#
# Output: target/<triple>/bundled/{TB303.vst3,TB303.clap}
# Install: ~/.vst3/  and  ~/.clap/
#
set -euo pipefail

TARGET="x86_64-unknown-linux-gnu"
PROFILE="release"
PKG="plugin303"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

VST3_DIR="${VST3_DIR:-$HOME/.vst3}"
CLAP_DIR="${CLAP_DIR:-$HOME/.clap}"

# x86-64-v3 == AVX2 baseline. Tune for the build host's exact core.
export RUSTFLAGS="${RUSTFLAGS:-} -C target-cpu=x86-64-v3"

echo ">> ensuring rust target $TARGET"
rustup target add "$TARGET" 2>/dev/null || true

echo ">> RUSTFLAGS: $RUSTFLAGS"
echo ">> bundling $PKG ($PROFILE, $TARGET)"
cargo xtask bundle "$PKG" --profile "$PROFILE" --target "$TARGET"

# Bundler writes to target/<triple>/bundled when --target is set, else target/bundled.
BUNDLE_DIR="target/$TARGET/bundled"
[ -d "$BUNDLE_DIR" ] || BUNDLE_DIR="target/bundled"

VST3_SRC="$BUNDLE_DIR/TB303.vst3"
CLAP_SRC="$BUNDLE_DIR/TB303.clap"

[ -e "$VST3_SRC" ] || { echo "!! VST3 bundle not found in $BUNDLE_DIR" >&2; exit 1; }
[ -e "$CLAP_SRC" ] || { echo "!! CLAP bundle not found in $BUNDLE_DIR" >&2; exit 1; }

echo ">> installing VST3 -> $VST3_DIR"
mkdir -p "$VST3_DIR"
rm -rf "$VST3_DIR/TB303.vst3"
cp -r "$VST3_SRC" "$VST3_DIR/"

echo ">> installing CLAP -> $CLAP_DIR"
mkdir -p "$CLAP_DIR"
rm -f "$CLAP_DIR/TB303.clap"
cp -r "$CLAP_SRC" "$CLAP_DIR/"

echo "OK"
echo "  VST3: $VST3_DIR/TB303.vst3"
echo "  CLAP: $CLAP_DIR/TB303.clap"
