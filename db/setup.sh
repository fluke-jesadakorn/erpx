#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")" && pwd)"
project_root="$(cd "$root/.." && pwd)"
backup_root="$(cd "$project_root/../infra/backups" && pwd)"
db_name="${POSTGRES_DB:-folio_db}"
db_user="${POSTGRES_USER:-contract}"
db_host="${POSTGRES_HOST:-localhost}"
db_port="${POSTGRES_PORT:-5432}"
export PGPASSWORD="${POSTGRES_PASSWORD:-contractpw}"
mode="${1:-init}"

psql_args=(
  psql
  -h "$db_host"
  -p "$db_port"
  -U "$db_user"
  -d "$db_name"
  -v ON_ERROR_STOP=1
)

apply_schema() {
  "${psql_args[@]}" -f "$root/schema.sql"
  "${psql_args[@]}" -f "$root/seed.sql"
  "${psql_args[@]}" -f "$root/migrations/2026-07-18-F-ai-selection-whitelist.sql"
  "${psql_args[@]}" -f "$root/migrations/2026-07-18-G-notification-inbox.sql"
  "${psql_args[@]}" -f "$root/migrations/2026-07-19-H-operational-accounting.sql"
  "${psql_args[@]}" -f "$root/migrations/2026-07-19-I-ai-openrouter-free-default.sql"
  "${psql_args[@]}" -f "$root/migrations/2026-07-19-J-local-reference-users.sql"
  "${psql_args[@]}" -f "$root/migrations/2026-07-19-K-partial-expense-payments.sql"
  "${psql_args[@]}" -f "$root/migrations/2026-07-19-L-sales-branch.sql"
  "${psql_args[@]}" -f "$root/migrations/2026-07-19-M-local-vision-default.sql"
  "${psql_args[@]}" -f "$root/migrations/2026-07-19-N-staff-full-chat.sql"
}

if [[ "$mode" == "--reset-local" ]]; then
  if [[ "$db_host" != "localhost" || "$db_name" != "folio_db" ]]; then
    echo "Reset requires POSTGRES_HOST=localhost and POSTGRES_DB=folio_db." >&2
    exit 1
  fi
  if [[ "${FOLIO_RESET_CONFIRM:-}" != "RESET_FOLIO_LOCAL" ]]; then
    echo "Reset requires FOLIO_RESET_CONFIRM=RESET_FOLIO_LOCAL." >&2
    exit 1
  fi
  target="$("${psql_args[@]}" -Atqc "SELECT current_database() || '|' || COALESCE(inet_server_addr()::text, 'local')")"
  if [[ "$target" != "folio_db|127.0.0.1" && "$target" != "folio_db|::1" && "$target" != "folio_db|::1/128" && "$target" != "folio_db|local" ]]; then
    echo "Refusing unexpected database target: $target" >&2
    exit 1
  fi
  umask 077
  server_major="$("${psql_args[@]}" -Atqc "SHOW server_version_num" | cut -c1-2 | sed 's/^0//')"
  pg_dump_bin="${PG_DUMP_BIN:-pg_dump}"
  pg_restore_bin="${PG_RESTORE_BIN:-pg_restore}"
  homebrew_pg="/opt/homebrew/opt/postgresql@$server_major/bin"
  if [[ -x "$homebrew_pg/pg_dump" && -x "$homebrew_pg/pg_restore" ]]; then
    pg_dump_bin="$homebrew_pg/pg_dump"
    pg_restore_bin="$homebrew_pg/pg_restore"
  fi
  dump_major="$("$pg_dump_bin" --version | awk '{print $3}' | cut -d. -f1)"
  if [[ "$dump_major" != "$server_major" ]]; then
    echo "pg_dump major $dump_major does not match PostgreSQL server major $server_major." >&2
    exit 1
  fi
  stamp="$(date -u +%Y%m%dT%H%M%SZ)"
  backup="$backup_root/accounting-reset-$stamp"
  mkdir -p "$backup/minio"
  chmod 700 "$backup" "$backup/minio"
  "$pg_dump_bin" -h "$db_host" -p "$db_port" -U "$db_user" -d "$db_name" -Fc -f "$backup/folio_db.dump"
  test -s "$backup/folio_db.dump"
  "$pg_restore_bin" -l "$backup/folio_db.dump" > "$backup/folio_db.restore-list.txt"
  test -s "$backup/folio_db.restore-list.txt"
  chmod 600 "$backup/folio_db.dump" "$backup/folio_db.restore-list.txt"
  (cd "$project_root" && bun db/storage-reset.ts backup "$backup/minio")
  test -s "$backup/minio/manifest.json"
  "${psql_args[@]}" -c "DROP SCHEMA IF EXISTS inventory, ai, auth, chat, finance, folio, hook, hr, law, n8n, perm CASCADE;"
  apply_schema
  (cd "$project_root" && bun db/storage-reset.ts recreate)
  "${psql_args[@]}" -Atqc "
    SELECT CASE WHEN
      (SELECT count(*) FROM finance.journals) = 0 AND
      (SELECT count(*) FROM inventory.stock_movements) = 0 AND
      (SELECT count(*) FROM inventory.products) = 0 AND
      (SELECT count(*) FROM folio.customers) = 0 AND
      (SELECT count(*) FROM folio.sales_orders) = 0 AND
      (SELECT count(*) FROM folio.expenses) = 0 AND
      (SELECT count(*) FROM folio.waybills) = 0 AND
      (SELECT count(*) FROM folio.exec_snapshots) = 0 AND
      (SELECT count(*) FROM folio.users WHERE employee_code LIKE 'DEV-%') = 17
    THEN 'reset-verified' ELSE 'reset-failed' END" | grep -qx 'reset-verified'
  echo "Reset $db_name to reference data only. Verified backup: $backup"
  exit 0
fi

if [[ "$mode" != "init" ]]; then
  echo "Usage: db/setup.sh [--reset-local]" >&2
  exit 1
fi

exists="$("${psql_args[@]}" -Atqc "SELECT to_regclass('folio.users') IS NOT NULL")"
if [[ "$exists" == "t" ]]; then
  echo "Refusing to initialize non-empty database: $db_name" >&2
  exit 1
fi

apply_schema
echo "Initialized $db_name with the current Folio schema and reference data."
