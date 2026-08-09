#!/usr/bin/env bash
# # Update Pepper on the Pi
#
#   One-command update: pull the latest code from GitHub and rebuild the
#   `pepper` binary in place. Run this ON the Pi after pushing changes.
#
#     ssh pepper-pi
#     ~/pepper-crm/scripts/update-pepper-pi.sh
#
# INPUT:
#   - Optional `PEPPER_HOME` (default: the repo this script lives in).
#   - Optional `BUILD_WEB=1` to also build the `pepper-web` dashboard.
#   - Optional `FORCE_BUILD=1` to rebuild even when git reports no changes.
#
# OUTPUT:
#   - Updated source tree and fresh `target/release/pepper`.
#   - First run installs the Rust toolchain into ~/.cargo (no sudo needed).
#
# NOTES:
#   - Untouched by updates: `.env` (settings/secrets), `.cache/` (geocode,
#     travel snapshots, digest stamp), `logs/`, and the cron table.
#   - Safe to re-run any time; does nothing when already up to date.
#
# Written by Cursor for Ready Mouse and Pepper CRM. August 2026. All rights reserved.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PEPPER_HOME="${PEPPER_HOME:-$(cd "${SCRIPT_DIR}/.." && pwd)}"
cd "${PEPPER_HOME}"

echo "==> Updating Pepper in ${PEPPER_HOME}"

# --- 1. Rust toolchain (first run only) -------------------------------------
if ! command -v cargo >/dev/null 2>&1 && [[ ! -x "${HOME}/.cargo/bin/cargo" ]]; then
  echo "==> Rust not found — installing (one-time, ~1 minute, no sudo)..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
fi
# Make cargo visible to this script regardless of shell setup.
export PATH="${HOME}/.cargo/bin:${PATH}"

if ! command -v cc >/dev/null 2>&1; then
  echo "ERROR: no C compiler (needed to link Rust programs)." >&2
  echo "Fix once with:  sudo apt install -y build-essential" >&2
  exit 1
fi

# --- 2. Pull the latest code -------------------------------------------------
BEFORE="$(git rev-parse HEAD)"
git pull --ff-only origin main
AFTER="$(git rev-parse HEAD)"

if [[ "${BEFORE}" == "${AFTER}" && -x target/release/pepper && -z "${FORCE_BUILD:-}" ]]; then
  echo "==> Already up to date (${AFTER:0:9}); binary present. Nothing to do."
  echo "    (Use FORCE_BUILD=1 to rebuild anyway.)"
  exit 0
fi

echo "==> Now at $(git log -1 --format='%h %s')"

# --- 3. Rebuild --------------------------------------------------------------
echo "==> Building pepper (a few minutes on the Pi)..."
cargo build --release -p pepper

if [[ -n "${BUILD_WEB:-}" ]]; then
  echo "==> Building pepper-web (BUILD_WEB=1)..."
  cargo build --release -p pepper-web
fi

# --- 4. Prove it runs ---------------------------------------------------------
echo "==> Build finished. Sanity check:"
./target/release/pepper --schedule-status || true

echo "==> Done. To preview the next digest without sending:"
echo "    cd ${PEPPER_HOME} && ./target/release/pepper --dry-run"
