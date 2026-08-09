#!/usr/bin/env bash
# Read-only CardDAV probe: fetch a single contact straight from Radicale on the Pi
# and report whether it currently contains a `Todo:` line. This bypasses Syncthing
# so it answers "does the SERVER have the edit?" independent of the local synced copy.
#
# Usage:
#   CARDDAV_BASE="https://<pi-host>:5232/<user>/922f8a55-1ab3-49d5-f5a6-29a4fbdd8937" \
#   CARDDAV_USER="<user>" CARDDAV_PASS="<pass>" \
#   ./scripts/check-pi-contact.sh [UID]
#
# - CARDDAV_BASE must point at the addressbook COLLECTION (no trailing slash needed).
# - UID defaults to Paul Brody's card.
# - Set CARDDAV_INSECURE=1 for self-signed TLS.
# This script only issues GET requests — it never writes.

set -euo pipefail

UID_ARG="${1:-986da39c-c90a-4ff5-81c9-f79f3326ef63}"
: "${CARDDAV_BASE:?Set CARDDAV_BASE to the Radicale addressbook collection URL}"
: "${CARDDAV_USER:?Set CARDDAV_USER}"
: "${CARDDAV_PASS:?Set CARDDAV_PASS}"

CURL_OPTS=(-sS --fail-with-body -u "${CARDDAV_USER}:${CARDDAV_PASS}")
if [[ "${CARDDAV_INSECURE:-0}" == "1" || "${CARDDAV_INSECURE:-}" == "true" ]]; then
  CURL_OPTS+=(-k)
fi

base="${CARDDAV_BASE%/}"
url="${base}/${UID_ARG}.vcf"

echo "GET ${url}"
echo "----------------------------------------"
body="$(curl "${CURL_OPTS[@]}" "${url}")"
echo "${body}"
echo "----------------------------------------"

if printf '%s' "${body}" | grep -qiE '(^|\\n|[[:space:]])todo:'; then
  echo "RESULT: Radicale HAS a Todo line for this contact (server-side edit is present)."
else
  echo "RESULT: NO Todo line in Radicale's copy — the phone edit never reached this addressbook."
fi
