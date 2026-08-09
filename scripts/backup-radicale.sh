#!/usr/bin/env bash
# # Backup Radicale Collections
#
#   Tars the Radicale data directory to a dated archive and prunes old backups,
#   so Pepper CardDAV writes are always reversible.
#
# INPUT:
#   - Optional `RADICALE_DATA_DIR` (default /var/lib/radicale).
#   - Optional `BACKUP_DIR` (default $PEPPER_HOME/backups/radicale).
#   - Optional `BACKUP_KEEP_DAYS` retention (default 30).
#
# OUTPUT:
#   - `${BACKUP_DIR}/radicale-YYYYMMDD-HHMMSS.tar.gz`; deletes archives older
#     than BACKUP_KEEP_DAYS.
#
# NOTES:
#   - /var/lib/radicale is usually owned by the radicale user — run via root cron:
#       sudo crontab -e
#       15 3 * * * PEPPER_HOME=/home/pi/pepper-crm /home/pi/pepper-crm/scripts/backup-radicale.sh
#   - Restore: stop radicale, extract the archive over RADICALE_DATA_DIR, restart.
#
# Written by Cursor for Ready Mouse and Pepper CRM. July 2026. All rights reserved.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PEPPER_HOME="${PEPPER_HOME:-$(cd "${SCRIPT_DIR}/.." && pwd)}"
RADICALE_DATA_DIR="${RADICALE_DATA_DIR:-/var/lib/radicale}"
BACKUP_DIR="${BACKUP_DIR:-${PEPPER_HOME}/backups/radicale}"
BACKUP_KEEP_DAYS="${BACKUP_KEEP_DAYS:-30}"

if [[ ! -d "${RADICALE_DATA_DIR}" ]]; then
  echo "Radicale data dir not found: ${RADICALE_DATA_DIR} (set RADICALE_DATA_DIR)" >&2
  exit 1
fi

mkdir -p "${BACKUP_DIR}"

STAMP="$(date +%Y%m%d-%H%M%S)"
ARCHIVE="${BACKUP_DIR}/radicale-${STAMP}.tar.gz"

# -C so archives extract relative to the data dir's parent, not absolute paths.
tar -czf "${ARCHIVE}" -C "$(dirname "${RADICALE_DATA_DIR}")" "$(basename "${RADICALE_DATA_DIR}")"

echo "Backed up ${RADICALE_DATA_DIR} to ${ARCHIVE} ($(du -h "${ARCHIVE}" | cut -f1))"

find "${BACKUP_DIR}" -name 'radicale-*.tar.gz' -mtime "+${BACKUP_KEEP_DAYS}" -print -delete |
  sed 's/^/Pruned old backup: /' || true
