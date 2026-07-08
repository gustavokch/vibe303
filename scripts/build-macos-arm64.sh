#!/usr/bin/env bash
#
# Build + bundle + install TB303 (VST3 + CLAP) for macOS arm64 (Apple Silicon).
#
# Targets aarch64-apple-darwin, tuned for apple-m1 (NEON baseline, runs on all
# M-series). Run on an Apple Silicon Mac; cross-compiling from Linux needs an
# osxcross/SDK setup not covered here.
#
# Output: target/<triple>/bundled/{TB303.vst3,TB303.clap}
# Install: ~/Library/Audio/Plug-Ins/VST3/  and  ~/Library/Audio/Plug-Ins/CLAP/
#
set -euo pipefail

TARGET="aarch64-apple-darwin"
PROFILE="release"
PKG="plugin303"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

VST3_DIR="${VST3_DIR:-$HOME/Library/Audio/Plug-Ins/VST3}"
CLAP_DIR="${CLAP_DIR:-$HOME/Library/Audio/Plug-Ins/CLAP}"

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

# apple-m1 == ARMv8.4-A + NEON; safe across all Apple Silicon. Override to
# apple-m2/m3 to tune for a specific host.
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-11.0}"
export RUSTFLAGS="${RUSTFLAGS:-} -C target-cpu=apple-m3"

echo ">> ensuring rust target $TARGET"
rustup target add "$TARGET" 2>/dev/null || true

echo ">> RUSTFLAGS: $RUSTFLAGS  (deployment target $MACOSX_DEPLOYMENT_TARGET)"
echo ">> bundling $PKG ($PROFILE, $TARGET, name: $VIBE303_NAME)"
cargo xtask bundle "$PKG" --profile "$PROFILE" --target "$TARGET"

# Bundler writes to target/<triple>/bundled when --target is set, else target/bundled.
BUNDLE_DIR="target/$TARGET/bundled"
[ -d "$BUNDLE_DIR" ] || BUNDLE_DIR="target/bundled"

VST3_SRC="$BUNDLE_DIR/${VIBE303_NAME}.vst3"
CLAP_SRC="$BUNDLE_DIR/${VIBE303_NAME}.clap"

[ -e "$VST3_SRC" ] || { echo "!! VST3 bundle not found in $BUNDLE_DIR" >&2; exit 1; }
[ -e "$CLAP_SRC" ] || { echo "!! CLAP bundle not found in $BUNDLE_DIR" >&2; exit 1; }

echo ">> installing VST3 -> $VST3_DIR"
mkdir -p "$VST3_DIR"
rm -rf "$VST3_DIR/${VIBE303_NAME}.vst3"
cp -R "$VST3_SRC" "$VST3_DIR/"

echo ">> installing CLAP -> $CLAP_DIR"
mkdir -p "$CLAP_DIR"
rm -rf "$CLAP_DIR/${VIBE303_NAME}.clap"
cp -R "$CLAP_SRC" "$CLAP_DIR/"

# Ad-hoc codesign so Gatekeeper / hardened DAWs load the bundles.
if command -v codesign >/dev/null 2>&1; then
  echo ">> ad-hoc codesigning"
  codesign --force --deep --sign - "$VST3_DIR/${VIBE303_NAME}.vst3" || true
  codesign --force --deep --sign - "$CLAP_DIR/${VIBE303_NAME}.clap" || true
fi

echo "OK"
echo "  VST3: $VST3_DIR/${VIBE303_NAME}.vst3"
echo "  CLAP: $CLAP_DIR/${VIBE303_NAME}.clap"
