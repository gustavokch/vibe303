#!/usr/bin/env bash
#
# Build + bundle + install TB303 (VST3 + CLAP) for macOS (Apple Silicon by default).
#
# Targets aarch64-apple-darwin, tuned for apple-m3 by default.
#   --m1     tune for apple-m1 (runs on all M-series); builds in target-m1/
#   --intel  cross-compile x86_64-apple-darwin; builds in target-intel/ and
#            "installs" to target-intel/dist/{VST3,CLAP} (not ~/Library)
# Run on an Apple Silicon Mac; Linux cross-compiling needs an osxcross/SDK setup.
#
# Output: target/<triple>/bundled/{TB303.vst3,TB303.clap}  (target-m1/, target-intel/ with flags)
# Install: ~/Library/Audio/Plug-Ins/VST3/  and  ~/Library/Audio/Plug-Ins/CLAP/
#
set -euo pipefail

ADD_VERSION=""
CPU="apple-m3"
TARGET_DIR="target"
TARGET="aarch64-apple-darwin"
INTEL=""
for arg in "$@"; do
  case $arg in
    --add-version) ADD_VERSION=1 ;;
    --m1) CPU="apple-m1"; TARGET_DIR="target-m1" ;;
    --intel)
      INTEL=1
      TARGET="x86_64-apple-darwin"
      CPU=""  # .cargo/config.toml already sets x86-64-v3 for x86_64 targets
      TARGET_DIR="target-intel"
      ;;
    *) echo "!! unknown option: $arg (supported: --add-version, --m1, --intel)" >&2; exit 2 ;;
  esac
done
# --m1 builds into its own cargo target dir so it never mixes with default artifacts.
export CARGO_TARGET_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/$TARGET_DIR"

VERSION_SUFFIX=""
if [ -n "$ADD_VERSION" ]; then
  VERSION_SUFFIX="-$(date +%d-%m-%Y)"
fi

PLUGIN_DIR_ROOT="$HOME/Library/Audio/Plug-Ins"
PROFILE="release"
PKG="plugin303"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# Intel bundles must not replace the native arm64 install: keep them under the
# intel target dir unless VST3_DIR / CLAP_DIR are set explicitly.
if [ -n "$INTEL" ]; then
  VST3_DIR="${VST3_DIR:-$REPO_ROOT/$TARGET_DIR/dist/VST3}"
  CLAP_DIR="${CLAP_DIR:-$REPO_ROOT/$TARGET_DIR/dist/CLAP}"
else
  VST3_DIR="${VST3_DIR:-$PLUGIN_DIR_ROOT/VST3}"
  CLAP_DIR="${CLAP_DIR:-$PLUGIN_DIR_ROOT/CLAP}"
fi

# Custom plugin naming / IDs
export VIBE303_NAME="${NAME:-${VIBE303_NAME:-TB303}}"
if [ -n "${CLAP_ID:-${VIBE303_CLAP_ID:-}}" ]; then
  export VIBE303_CLAP_ID="${CLAP_ID:-$VIBE303_CLAP_ID}"
fi
if [ -n "${VST3_CLASS_ID:-${VIBE303_VST3_CLASS_ID:-}}" ]; then
  export VIBE303_VST3_CLASS_ID="${VST3_CLASS_ID:-$VIBE303_VST3_CLASS_ID}"
fi

# We use a trap to restore the original bundler.toml on exit/error
ORIG_BUNDLER_TOML=""
if [ -f "bundler.toml" ]; then
  ORIG_BUNDLER_TOML="$(cat bundler.toml)"
fi

restore_bundler_toml() {
  echo ">> restoring bundler.toml"
  if [ -n "$ORIG_BUNDLER_TOML" ]; then
    echo "$ORIG_BUNDLER_TOML" > bundler.toml
  else
    rm -f bundler.toml
  fi
}
trap restore_bundler_toml EXIT

echo ">> configuring bundler.toml with plugin name: $VIBE303_NAME"
cat <<EOF > bundler.toml
# nih-plug bundler metadata.
[plugin303]
name = "$VIBE303_NAME"
EOF

# Default tunes for apple-m3. --m1 targets apple-m1 (ARMv8.4-A + NEON), which
# runs on all Apple Silicon. --intel cross-compiles x86_64-apple-darwin; the
# x86-64-v3 baseline (AVX2+FMA, Haswell+) comes from .cargo/config.toml, and
# RUSTFLAGS is left alone so the host-built xtask is not given an x86 CPU.
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-11.0}"
if [ -n "$CPU" ]; then
  export RUSTFLAGS="${RUSTFLAGS:-} -C target-cpu=$CPU"
fi

echo ">> ensuring rust target $TARGET"
rustup target add "$TARGET" 2>/dev/null || true

echo ">> RUSTFLAGS: $RUSTFLAGS  (deployment target $MACOSX_DEPLOYMENT_TARGET)"
echo ">> bundling $PKG ($PROFILE, $TARGET, name: $VIBE303_NAME)"
cargo xtask bundle "$PKG" --profile "$PROFILE" --target "$TARGET"

# Bundler writes to <target dir>/<triple>/bundled when --target is set, else <target dir>/bundled.
BUNDLE_DIR="$TARGET_DIR/$TARGET/bundled"
[ -d "$BUNDLE_DIR" ] || BUNDLE_DIR="$TARGET_DIR/bundled"

BUNDLE_NAME="${VIBE303_NAME}${VERSION_SUFFIX}"

VST3_SRC="$BUNDLE_DIR/${VIBE303_NAME}.vst3"
CLAP_SRC="$BUNDLE_DIR/${VIBE303_NAME}.clap"

[ -e "$VST3_SRC" ] || { echo "!! VST3 bundle not found in $BUNDLE_DIR" >&2; exit 1; }
[ -e "$CLAP_SRC" ] || { echo "!! CLAP bundle not found in $BUNDLE_DIR" >&2; exit 1; }

echo ">> installing VST3 -> $VST3_DIR"
mkdir -p "$VST3_DIR"
rm -rf "$VST3_DIR/${BUNDLE_NAME}.vst3"
cp -R "$VST3_SRC" "$VST3_DIR/${BUNDLE_NAME}.vst3"

echo ">> installing CLAP -> $CLAP_DIR"
mkdir -p "$CLAP_DIR"
rm -rf "$CLAP_DIR/${BUNDLE_NAME}.clap"
cp -R "$CLAP_SRC" "$CLAP_DIR/${BUNDLE_NAME}.clap"

# Ad-hoc codesign so Gatekeeper / hardened DAWs load the bundles.
if command -v codesign >/dev/null 2>&1; then
  echo ">> ad-hoc codesigning"
  codesign --force --deep --sign - "$VST3_DIR/${BUNDLE_NAME}.vst3" || true
  codesign --force --deep --sign - "$CLAP_DIR/${BUNDLE_NAME}.clap" || true
fi

echo "OK"
echo "  VST3: $VST3_DIR/${BUNDLE_NAME}.vst3"
echo "  CLAP: $CLAP_DIR/${BUNDLE_NAME}.clap"
