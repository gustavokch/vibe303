#!/usr/bin/env bash
#
# Cross-compile TB303 (VST3 + CLAP, or Windows standalone exe) from macOS.
# Bundles only; nothing is installed (the result cannot run on the build host).
#
#   scripts/build-cross-from-macos.sh linux     # x86_64-unknown-linux-gnu  (zig)
#   scripts/build-cross-from-macos.sh windows   # x86_64-pc-windows-msvc    (cargo-xwin)
#   scripts/build-cross-from-macos.sh windows-exe  # standalone.exe (GUI + WASAPI)
#   VARIANT=v2 scripts/build-cross-from-macos.sh linux   # x86-64-v2 instead of v3
#
# Prereqs: brew install zig; cargo install cargo-zigbuild cargo-xwin
# Output:  target/cross-<os>/{TB303.vst3,TB303.clap}; windows-exe: target/cross-windows/TB303-standalone.exe
#
# nih_plug_xtask hardcodes `Command::new("cargo") build ...`. A temporary PATH shim
# reroutes that subcommand to cargo-zigbuild / cargo-xwin; everything else passes
# through to the real cargo.
set -euo pipefail

OS="${1:-}"
VARIANT="${VARIANT:-v3}"
PKG="plugin303"

case "$OS" in
  linux)
    TARGET="x86_64-unknown-linux-gnu"
    BUILD_SUB="zigbuild"
    # Headless: `standalone` needs alsa/jack and `gui` needs X11/GL; pkg-config needs a
    # target sysroot for both. Use the native Linux script (or a container) for those.
    FEATURES=(--no-default-features)
    NEEDS=(zig cargo-zigbuild)
    ;;
  windows)
    TARGET="x86_64-pc-windows-msvc"
    BUILD_SUB="xwin build"
    # Same feature set as scripts/build-windows-x86_64.ps1.
    FEATURES=(--no-default-features --features gui)
    NEEDS=(cargo-xwin)
    ;;
  windows-exe)
    TARGET="x86_64-pc-windows-msvc"
    NEEDS=(cargo-xwin)
    ;;
  *) echo "usage: $0 {linux|windows|windows-exe}" >&2; exit 2 ;;
esac

for t in "${NEEDS[@]}"; do
  command -v "$t" >/dev/null || { echo "!! missing tool: $t" >&2; exit 1; }
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# Use the rustup toolchain (where `rustup target add` installs std), even if a
# Homebrew rust shadows it on PATH.
REAL_CARGO="$(rustup which cargo)"
export PATH="$(dirname "$REAL_CARGO"):$PATH"
export CARGO="$REAL_CARGO"   # cargo-metadata calls bypass the shim

if [ "$OS" = "windows-exe" ]; then
  export RUSTFLAGS="-C target-cpu=x86-64-$VARIANT"
  rustup target add "$TARGET" 2>/dev/null || true
  echo ">> building standalone.exe for $TARGET ($RUSTFLAGS)"
  "$REAL_CARGO" xwin build --release --target "$TARGET" -p "$PKG" --bin standalone
  mkdir -p target/cross-windows
  cp "target/$TARGET/release/standalone.exe" target/cross-windows/TB303-standalone.exe
  echo "OK -> target/cross-windows/TB303-standalone.exe"
  file target/cross-windows/TB303-standalone.exe
  exit 0
fi

# Build the host-side xtask runner BEFORE exporting RUSTFLAGS: x86-64 flags
# would otherwise leak into this arm64 host build and crash rustc.
"$REAL_CARGO" build --release --package xtask
XTASK="$REPO_ROOT/target/release/xtask"

SHIM_DIR="$(mktemp -d)"
trap 'rm -rf "$SHIM_DIR"' EXIT
cat > "$SHIM_DIR/cargo" <<EOF
#!/usr/bin/env bash
if [ "\${1:-}" = "build" ]; then
  shift
  exec "$REAL_CARGO" $BUILD_SUB "\$@"
fi
exec "$REAL_CARGO" "\$@"
EOF
chmod +x "$SHIM_DIR/cargo"
export PATH="$SHIM_DIR:$PATH"

# Plain RUSTFLAGS replaces the .cargo/config.toml x86_64 bucket (see the .ps1).
export RUSTFLAGS="-C target-cpu=x86-64-$VARIANT"

rustup target add "$TARGET" 2>/dev/null || true

echo ">> bundling $PKG for $TARGET ($RUSTFLAGS)"
"$XTASK" bundle "$PKG" --release --target "$TARGET" ${FEATURES[@]+"${FEATURES[@]}"}

# The bundler writes to target/bundled (shared with native macOS builds) or
# target/<triple>/bundled. Move the result out so a foreign-arch bundle never
# sits where the macOS install script looks.
OUT="target/cross-$OS"
rm -rf "$OUT/TB303.vst3" "$OUT/TB303.clap"; mkdir -p "$OUT"
for SRC in "target/$TARGET/bundled" "target/bundled"; do
  if [ -e "$SRC/TB303.vst3" ] && [ -e "$SRC/TB303.clap" ]; then
    mv "$SRC/TB303.vst3" "$SRC/TB303.clap" "$OUT/"
    break
  fi
done
[ -e "$OUT/TB303.vst3" ] && [ -e "$OUT/TB303.clap" ] || { echo "!! bundles not found" >&2; exit 1; }
echo "OK -> $OUT"
find "$OUT" -type f -name 'TB303*' -exec file {} \;
