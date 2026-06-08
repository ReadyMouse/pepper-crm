#!/usr/bin/env bash
# # Cross-Compile Linux ARM64 Binaries
#
#   Builds `pepper` and `pepper-web` for Raspberry Pi 5 (aarch64 Linux) from an Apple Silicon Mac.
#
# INPUT:
#   - Homebrew cross toolchain and `rustup target add aarch64-unknown-linux-gnu`.
#
# OUTPUT:
#   - `target/aarch64-unknown-linux-gnu/release/{pepper,pepper-web,...}`
#
# NOTES:
#   - Prerequisites: `brew install messense/macos-cross-toolchains/aarch64-unknown-linux-gnu`
#
# Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

set -euo pipefail
cd "$(dirname "$0")/.."

export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="${CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER:-aarch64-unknown-linux-gnu-gcc}"

TARGET=aarch64-unknown-linux-gnu
PACKAGES=(pepper pepper-web)

echo "Building for ${TARGET} (Pi / Linux ARM64)…"
echo "Linker: ${CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER}"

cargo build --release --target "${TARGET}" "${PACKAGES[@]/#/-p }"

echo
echo "Binaries:"
for pkg in "${PACKAGES[@]}"; do
  bin="target/${TARGET}/release/${pkg}"
  if [[ -f "${bin}" ]]; then
    file "${bin}"
  fi
done

echo
echo "Copy to Pi, e.g.:"
echo "  scp target/${TARGET}/release/pepper target/${TARGET}/release/pepper-web pi@your-pi:~/pepper-crm/"
