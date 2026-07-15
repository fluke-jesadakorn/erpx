#!/usr/bin/env bash
# folio/db/migrate.sh — apply all folio migrations in order.
# Run from /Users/fluke/Desktop/Work/Contents/folio/db
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
DB="folio_db"
USER="contract"
HOST="localhost"
export PGPASSWORD="${PGPASSWORD:-contractpw}"

PSQL=(psql -h "$HOST" -U "$USER" -d "$DB" -v ON_ERROR_STOP=1)

echo "→ 00_schemas"
"${PSQL[@]}" -f "$ROOT/00_schemas.sql"

for phase in 01_perm 02_finance 03_hook 04_hr 05_law 06_folio_cockpit; do
  if [[ -d "$ROOT/$phase" ]]; then
    echo "→ $phase"
    for f in "$ROOT/$phase"/*.sql; do
      [[ -f "$f" ]] || continue
      echo "    $(basename "$f")"
      "${PSQL[@]}" -f "$f"
    done
  fi
done

echo "Done."