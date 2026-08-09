#!/usr/bin/env bash
# # Run Digest Every 3 Days (launchd wrapper)
#
#   Sends the digest immediately (no Monday window) when the last successful
#   send was 3+ days ago. Designed to be invoked hourly by launchd on a Mac
#   that sleeps: each run is a cheap stamp check, so missed hours don't matter.
#
# INPUT:
#   - `PEPPER_HOME/.env` with SMTP + `DIGEST_RECIPIENT` (and CardDAV vars).
#   - Optional `DIGEST_INTERVAL_SECS` (default 259200 = 3 days).
#   - Optional `DIGEST_FORCE=1` to send now regardless of the stamp.
#   - Optional `PEPPER_BIN` override.
#
# OUTPUT:
#   - Sends digest when due; appends to `logs/digest-3day.log`.
#   - Updates `.cache/digest-last-sent` (epoch seconds) only on SUCCESS,
#     so a failed send (e.g. Pi unreachable away from home) retries next hour.
#
# NOTES:
#   - Install the schedule with scripts/com.pepper.digest.plist (launchd).
#   - Exits silently when not due to keep the log readable.
#
# Written by Cursor for Ready Mouse and Pepper CRM. August 2026. All rights reserved.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PEPPER_HOME="${PEPPER_HOME:-$(cd "${SCRIPT_DIR}/.." && pwd)}"
LOG_DIR="${PEPPER_LOG_DIR:-${PEPPER_HOME}/logs}"
LOG_FILE="${LOG_DIR}/digest-3day.log"
STAMP_FILE="${PEPPER_HOME}/.cache/digest-last-sent"
INTERVAL_SECS="${DIGEST_INTERVAL_SECS:-259200}"

now="$(date +%s)"
last="$(cat "${STAMP_FILE}" 2>/dev/null || echo 0)"

if [[ -z "${DIGEST_FORCE:-}" ]] && (( now - last < INTERVAL_SECS )); then
  exit 0
fi

mkdir -p "${LOG_DIR}" "$(dirname "${STAMP_FILE}")"

resolve_pepper_bin() {
  if [[ -n "${PEPPER_BIN:-}" && -x "${PEPPER_BIN}" ]]; then
    echo "${PEPPER_BIN}"
    return
  fi
  local candidates=(
    "${PEPPER_HOME}/target/release/pepper"
    "${PEPPER_HOME}/target/debug/pepper"
    "${PEPPER_HOME}/pepper"
    "${PEPPER_HOME}/bin/pepper"
  )
  local c
  for c in "${candidates[@]}"; do
    if [[ -x "${c}" ]]; then
      echo "${c}"
      return
    fi
  done
  return 1
}

{
  echo "=== $(date -Is 2>/dev/null || date) 3-day digest attempt (last sent: $(date -r "${last}" 2>/dev/null || echo never)) ==="
  if ! PEPPER_BIN_RESOLVED="$(resolve_pepper_bin)"; then
    echo "pepper binary not found under ${PEPPER_HOME} (set PEPPER_BIN)"
    exit 1
  fi
  cd "${PEPPER_HOME}"
  if "${PEPPER_BIN_RESOLVED}"; then
    echo "${now}" > "${STAMP_FILE}"
    echo "=== digest sent OK; next send after $(date -r "$((now + INTERVAL_SECS))" 2>/dev/null || echo "+3 days") ==="
  else
    echo "=== digest send FAILED (exit $?); will retry on the next hourly check ==="
    exit 1
  fi
} >>"${LOG_FILE}" 2>&1
