#!/usr/bin/env bash
#
# Build a release layout of kakao-cli into a prefix.
#
#   scripts/build-release.sh [PREFIX]
#
# PREFIX defaults to ./dist. Produces:
#   PREFIX/bin/kakao-cli
#   PREFIX/libexec/kakao-cli/kakao-macos-bridge   (macOS)
#
# Both binaries get a free ad-hoc code signature with a stable identifier so
# macOS TCC (accessibility) permission survives upgrades. No Developer ID, no
# notarization (see docs/adr/0002-distribution.md).

set -euo pipefail

PREFIX="${1:-$(pwd)/dist}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

ID_MAIN="com.hhw12409.kakao-cli"
ID_BRIDGE="com.hhw12409.kakao-cli.macos-bridge"

echo "==> cargo build --release (common core)"
cargo build --release --locked -p kakao-core

mkdir -p "$PREFIX/bin" "$PREFIX/libexec/kakao-cli"
install -m 0755 "target/release/kakao-cli" "$PREFIX/bin/kakao-cli"

OS="$(uname -s)"
if [ "$OS" = "Darwin" ]; then
  # A universal build needs full Xcode (xcbuild). With Command Line Tools only,
  # fall back to a native single-arch build.
  if swift build -c release --package-path adapters/macos \
       --arch arm64 --arch x86_64 2>/dev/null; then
    echo "==> swift build -c release (macOS bridge, universal)"
    BIN_DIR="$(swift build -c release --package-path adapters/macos --arch arm64 --arch x86_64 --show-bin-path)"
  else
    echo "==> swift build -c release (macOS bridge, native arch — install full Xcode for universal)"
    swift build -c release --package-path adapters/macos
    BIN_DIR="$(swift build -c release --package-path adapters/macos --show-bin-path)"
  fi
  install -m 0755 "$BIN_DIR/kakao-macos-bridge" "$PREFIX/libexec/kakao-cli/kakao-macos-bridge"

  echo "==> ad-hoc codesign (free, stable identifier)"
  codesign --force --sign - --identifier "$ID_MAIN"   "$PREFIX/bin/kakao-cli"
  codesign --force --sign - --identifier "$ID_BRIDGE" "$PREFIX/libexec/kakao-cli/kakao-macos-bridge"
  codesign --verify --verbose "$PREFIX/bin/kakao-cli"           >/dev/null
  codesign --verify --verbose "$PREFIX/libexec/kakao-cli/kakao-macos-bridge" >/dev/null
elif [ "$OS" = "MINGW"* ] || [ "$OS" = "MSYS"* ]; then
  echo "==> Windows bridge not implemented yet; skipping"
fi

echo
echo "Built into: $PREFIX"
echo "  $("$PREFIX/bin/kakao-cli" --version 2>/dev/null || echo 'kakao-cli')"
echo
echo "Try:  $PREFIX/bin/kakao-cli doctor"
