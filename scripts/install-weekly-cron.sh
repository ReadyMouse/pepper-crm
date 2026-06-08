#!/usr/bin/env bash
# # Install Weekly Digest Cron
#
#   Adds (or replaces) the Pepper hourly digest cron entry for the current user.
#
# INPUT:
#   - Optional `PEPPER_HOME` (defaults to repo root).
#   - Existing user crontab (merged without duplicate Pepper entries).
#
# OUTPUT:
#   - Hourly cron line invoking `run-weekly-digest.sh`.
#
# NOTES:
#   - `pepper --send-if-due` sends Monday at 6:00 in trip timezone (or US Eastern).
#   - Usage: `./scripts/install-weekly-cron.sh`
#
# Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PEPPER_HOME="${PEPPER_HOME:-$(cd "${SCRIPT_DIR}/.." && pwd)}"
RUN_SCRIPT="${PEPPER_HOME}/scripts/run-weekly-digest.sh"
MARKER="# pepper-crm weekly digest"

if [[ ! -x "${RUN_SCRIPT}" ]]; then
  chmod +x "${RUN_SCRIPT}"
fi

CRON_LINE="0 * * * * PEPPER_HOME=${PEPPER_HOME} ${RUN_SCRIPT} ${MARKER}"

existing="$(crontab -l 2>/dev/null || true)"
filtered="$(printf '%s\n' "${existing}" | grep -v 'pepper-crm weekly digest' | grep -v 'run-weekly-digest.sh' || true)"

{
  printf '%s\n' "${filtered}" | sed '/^[[:space:]]*$/d'
  echo "${CRON_LINE}"
} | crontab -

echo "Installed crontab entry:"
echo "  ${CRON_LINE}"
echo
echo "Logs: ${PEPPER_HOME}/logs/weekly-digest.log"
echo "Test now: PEPPER_HOME=${PEPPER_HOME} ${RUN_SCRIPT}"
