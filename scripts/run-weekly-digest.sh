#!/usr/bin/env bash
# # Run Weekly Digest (Cron Wrapper)
#
#   Invokes `pepper --send-if-due` when the calendar-aware schedule says it is due.
#
# INPUT:
#   - `PEPPER_HOME/.env` with SMTP, `DIGEST_RECIPIENT`, optional `GOOGLE_CALENDAR_ICS_URL`.
#   - Optional `DIGEST_FORCE=1` to send immediately; optional `PEPPER_BIN` override.
#
# OUTPUT:
#   - Appends run logs to `logs/weekly-digest.log`; sends digest when due.
#
# NOTES:
#   - Cron should invoke hourly; pepper exits quietly when not in the send window.
#   - Monday 6:00 local in trip timezone (from calendar SUMMARY), else US Eastern.
#
# Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PEPPER_HOME="${PEPPER_HOME:-$(cd "${SCRIPT_DIR}/.." && pwd)}"
LOG_DIR="${PEPPER_LOG_DIR:-${PEPPER_HOME}/logs}"
LOG_FILE="${LOG_DIR}/weekly-digest.log"

mkdir -p "${LOG_DIR}"

resolve_pepper_bin() {
  if [[ -n "${PEPPER_BIN:-}" && -x "${PEPPER_BIN}" ]]; then
    echo "${PEPPER_BIN}"
    return
  fi
  local candidates=(
    "${PEPPER_HOME}/pepper"
    "${PEPPER_HOME}/bin/pepper"
    "${PEPPER_HOME}/target/release/pepper"
    "${PEPPER_HOME}/target/debug/pepper"
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

if ! PEPPER_BIN_RESOLVED="$(resolve_pepper_bin)"; then
  echo "pepper binary not found under ${PEPPER_HOME} (set PEPPER_BIN)" >&2
  exit 1
fi

if [[ ! -f "${PEPPER_HOME}/.env" ]]; then
  echo "Missing ${PEPPER_HOME}/.env — copy from .env.example and configure SMTP + DIGEST_RECIPIENT" >&2
  exit 1
fi

{
  echo "=== $(date -Is 2>/dev/null || date) weekly digest start ==="
  echo "PEPPER_HOME=${PEPPER_HOME}"
  echo "PEPPER_BIN=${PEPPER_BIN_RESOLVED}"
  cd "${PEPPER_HOME}"
  PEPPER_ARGS=(--send-if-due)
  if [[ -n "${DIGEST_FORCE:-}" ]]; then
    echo "DIGEST_FORCE set — sending without schedule check"
    PEPPER_ARGS=()
  fi
  exec "${PEPPER_BIN_RESOLVED}" "${PEPPER_ARGS[@]}"
} >>"${LOG_FILE}" 2>&1
