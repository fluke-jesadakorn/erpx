#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
MANIFEST="$SCRIPT_DIR/runtime-versions.json"

: "${ERPX_CLAMD_SOCKET:?Set ERPX_CLAMD_SOCKET to an absolute local Unix socket}"
: "${ERPX_CLAMAV_DATABASE_DIR:?Set ERPX_CLAMAV_DATABASE_DIR to the FreshClam database directory}"
: "${ERPX_CLAMD_CONF:=/Library/Application Support/ERPX-Attachments/clamd.conf}"
: "${ERPX_FRESHCLAM_CONF:=/Library/Application Support/ERPX-Attachments/freshclam.conf}"

manifest_value() {
  /usr/bin/python3 - "$MANIFEST" "$1" "$2" <<'PY'
import json,sys
with open(sys.argv[1],encoding='utf-8') as source:
    manifest=json.load(source)
value=manifest[sys.argv[2]][sys.argv[3]]
if not isinstance(value,(str,int,float)):
    raise SystemExit('invalid attachment runtime manifest value')
print(value)
PY
}

CLAMAV_VERSION=$(manifest_value clamav version)
CLAMDSCAN=$(manifest_value clamav clamdscan)
FRESHCLAM=$(manifest_value clamav freshclam)
LIBHEIF_VERSION=$(manifest_value libheif version)
ERPX_HEIF_CONVERT=${ERPX_HEIF_CONVERT:-$(manifest_value libheif heifConvert)}
POPPLER_VERSION=$(manifest_value poppler version)
ERPX_PDFTOPPM_BIN=${ERPX_PDFTOPPM_BIN:-$(manifest_value poppler pdftoppm)}

case "$ERPX_CLAMD_SOCKET" in /*) ;; *) exit 1;; esac
case "$ERPX_CLAMAV_DATABASE_DIR" in /*) ;; *) exit 1;; esac
case "$ERPX_CLAMD_CONF" in /*) ;; *) exit 1;; esac
case "$ERPX_FRESHCLAM_CONF" in /*) ;; *) exit 1;; esac
test -S "$ERPX_CLAMD_SOCKET"
test -r "$ERPX_CLAMD_CONF"
test -r "$ERPX_FRESHCLAM_CONF"
test -x "$CLAMDSCAN"
test -x "$FRESHCLAM"
test -x "$ERPX_HEIF_CONVERT"
test -x "$ERPX_PDFTOPPM_BIN"

CONFIGURED_SOCKET=$(/usr/bin/awk '$1=="LocalSocket" {$1="";sub(/^ /,"");print;exit}' "$ERPX_CLAMD_CONF")
CLAMD_DATABASE=$(/usr/bin/awk '$1=="DatabaseDirectory" {$1="";sub(/^ /,"");print;exit}' "$ERPX_CLAMD_CONF")
FRESHCLAM_DATABASE=$(/usr/bin/awk '$1=="DatabaseDirectory" {$1="";sub(/^ /,"");print;exit}' "$ERPX_FRESHCLAM_CONF")
test "$CONFIGURED_SOCKET" = "$ERPX_CLAMD_SOCKET"
test "$CLAMD_DATABASE" = "$ERPX_CLAMAV_DATABASE_DIR"
test "$FRESHCLAM_DATABASE" = "$ERPX_CLAMAV_DATABASE_DIR"
/usr/bin/grep -Eq '^[[:space:]]*OfficialDatabaseOnly[[:space:]]+yes[[:space:]]*$' "$ERPX_CLAMD_CONF"
/usr/bin/grep -Eq '^[[:space:]]*FailIfCvdOlderThan[[:space:]]+1[[:space:]]*$' "$ERPX_CLAMD_CONF"
if /usr/bin/grep -Eq '^[[:space:]]*(TCPSocket|TCPAddr)[[:space:]]' "$ERPX_CLAMD_CONF"; then
  printf '%s\n' 'Refusing clamd config with a TCP listener.' >&2
  exit 1
fi
test "$(/usr/bin/stat -f '%Sg' "$ERPX_CLAMD_SOCKET")" = '_erpx_scanner'
test "$(/usr/bin/stat -f '%Lp' "$ERPX_CLAMD_SOCKET")" = '660'

LATEST_DEFINITION=0
for name in daily.cvd daily.cld main.cvd main.cld; do
  path="$ERPX_CLAMAV_DATABASE_DIR/$name"
  if test -f "$path"; then
    modified=$(/usr/bin/stat -f '%m' "$path")
    if test "$modified" -gt "$LATEST_DEFINITION"; then LATEST_DEFINITION=$modified; fi
  fi
done
test "$LATEST_DEFINITION" -gt 0
AGE_SECONDS=$(( $(/bin/date +%s) - LATEST_DEFINITION ))
if test "$AGE_SECONDS" -lt 0 || test "$AGE_SECONDS" -gt 86400; then
  printf '%s\n' 'ClamAV definitions are missing, future-dated, or older than 24 hours.' >&2
  exit 1
fi

CLAM_VERSION_OUTPUT=$("$CLAMDSCAN" --config-file="$ERPX_CLAMD_CONF" --version 2>&1)
case "$CLAM_VERSION_OUTPUT" in "ClamAV $CLAMAV_VERSION/"*) ;; *) printf '%s\n' 'ClamAV runtime does not match the pinned manifest.' >&2; exit 1;; esac
FRESHCLAM_VERSION_OUTPUT=$("$FRESHCLAM" --config-file="$ERPX_FRESHCLAM_CONF" --version 2>&1)
case "$FRESHCLAM_VERSION_OUTPUT" in "ClamAV $CLAMAV_VERSION/"*) ;; *) printf '%s\n' 'FreshClam runtime does not match the pinned manifest.' >&2; exit 1;; esac
HEIF_VERSION_OUTPUT=$("$ERPX_HEIF_CONVERT" --version 2>&1)
case "$HEIF_VERSION_OUTPUT" in "$LIBHEIF_VERSION"*) ;; *) printf '%s\n' 'libheif runtime does not match the pinned manifest.' >&2; exit 1;; esac
POPPLER_VERSION_OUTPUT=$("$ERPX_PDFTOPPM_BIN" -v 2>&1)
case "$POPPLER_VERSION_OUTPUT" in "pdftoppm version $POPPLER_VERSION"*) ;; *) printf '%s\n' 'Poppler runtime does not match the pinned manifest.' >&2; exit 1;; esac

/bin/launchctl print system/com.erpx.clamd >/dev/null
/bin/launchctl print system/com.erpx.freshclam >/dev/null
"$CLAMDSCAN" --config-file="$ERPX_CLAMD_CONF" --ping 1 >/dev/null
printf '%s\n' "Attachment runtime ready: ClamAV $CLAMAV_VERSION; libheif $LIBHEIF_VERSION; Poppler $POPPLER_VERSION; definitions ${AGE_SECONDS}s old"
