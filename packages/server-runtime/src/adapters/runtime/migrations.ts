import { runtimeResourcePath } from "@erpx/server-runtime/adapters/runtime/resources";
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { readdir, readFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { PostgresDatabase } from "@erpx/server-runtime/adapters/runtime/postgres";
import type { PoolClient } from 'pg';
import { catalogObjectExistsSql } from '../read-tail-sql';
const MIGRATION_LOCK_SQL = 'SELECT pg_advisory_lock(714928001)';
const MIGRATION_UNLOCK_SQL = 'SELECT pg_advisory_unlock(714928001)';

// The receipt insert is owned by the Rust crate (runtime_stmt_22) and mirrored
// by runtime-migrations-sql.json; the test in native_command_sql.rs keeps the
// two identical. It must run on the caller's connection inside the same
// transaction as the DDL, not over the HTTP command ingress: the ingress may
// point at a different database that has no ledger tables.
const { migrations_insert: migrationReceiptSql } = JSON.parse(
  readFileSync(join(dirname(fileURLToPath(import.meta.url)), '../../runtime-migrations-sql.json'), 'utf8'),
) as { migrations_insert: string };

export type MigrationStatus = {
  track: 'control' | 'control-read' | 'holding-write' | 'holding-read';
  applied: number;
  pending: string[];
  latestApplied: string | null;
};

/** Read migration receipts without changing the database. */
export async function inspectMigrations(
  db: PostgresDatabase,
  track: MigrationStatus['track'],
  root = runtimeResourcePath('migrations/postgres'),
): Promise<MigrationStatus> {
  const files = (await readdir(join(root, track))).filter(file => file.endsWith('.sql')).sort();
  let names: string[] = [];
  try {
    // Application roles are deliberately denied direct access to the migration
    // receipt table. The migration-owned, read-only function exposes only the
    // receipt names needed by the operational screen while preserving that
    // boundary.
    names = (await db.prepare('SELECT name FROM erpx_migration_receipts() ORDER BY name').all<{ name: string }>()).results.map(row => row.name);
  } catch (error) {
    if (!['42P01', '42883'].includes((error as { code?: string }).code ?? '')) throw error;
    // A missing function does not prove that no migrations were applied.
    // Older Control tracks have receipts without the narrow read-only reader.
    // Inspect catalogue presence without falling back to privileged table reads.
    const receiptTable = await db.prepare(catalogObjectExistsSql('to_regclass', 'public.erpx_migrations')).first<boolean>('present');
    if (receiptTable) throw Object.assign(new Error('MIGRATION_RECEIPTS_UNAVAILABLE'), {
      code: 'MIGRATION_RECEIPTS_UNAVAILABLE', cause: error,
    });
  }
  const appliedNames = new Set(names);
  return {
    track,
    applied: names.length,
    pending: files.filter(file => !appliedNames.has(file)),
    latestApplied: names.at(-1) ?? null,
  };
}

async function refreshCoverage(client: PoolClient, track: string) {
  if (track === 'holding-write' && (await client.query(catalogObjectExistsSql('to_regprocedure', 'ensure_work_cache_coverage()'))).rows[0].present)
    await client.query('SELECT ensure_work_cache_coverage()');
}

/** Schema clones still need the migration engine's repair for newly created tables. */
export async function refreshMigrationCoverage(db: PostgresDatabase, track: string) {
  if (!['control','control-read','holding-write','holding-read'].includes(track)) throw new Error('INVALID_MIGRATION_TRACK');
  const client = await db.pool.connect();
  try {
    await client.query(MIGRATION_LOCK_SQL);
    await refreshCoverage(client, track);
  } finally { await client.query(MIGRATION_UNLOCK_SQL).catch(() => {}); client.release(); }
}

export async function migrateDatabase(db: PostgresDatabase, track: string, root = runtimeResourcePath('migrations/postgres')) {
  if (!['control','control-read','holding-write','holding-read'].includes(track)) throw new Error('INVALID_MIGRATION_TRACK');
  const client = await db.pool.connect();
  try {
    await client.query(MIGRATION_LOCK_SQL);
    await client.query('CREATE TABLE IF NOT EXISTS erpx_migrations(name TEXT PRIMARY KEY,checksum TEXT NOT NULL,applied_at TIMESTAMPTZ NOT NULL DEFAULT now())');
    for (const name of (await readdir(join(root, track))).filter(f => f.endsWith('.sql')).sort()) {
      const sql = await readFile(join(root, track, name), 'utf8'), checksum = createHash('sha256').update(sql).digest('hex');
      const previous = await client.query('SELECT checksum FROM erpx_migrations WHERE name=$1', [name]);
      if (previous.rows.length) {
        // Two shipped 0041 variants exist in development. Preserve the original receipt;
        // 0042 repairs entitlement provenance additively and 0043 normalizes approval kinds.
        // This exception is pinned to both exact hashes, never to a filename alone.
        const repairedLeavePredecessor = track === 'holding-write' && name === '0041_leave_self_service.sql'
          && previous.rows[0].checksum === 'fb926c55f087b296c11f0f3356a32c2473bd3c56ea13834f25f186564da9038a'
          && checksum === '0b964e468801e0d410cbcce5e1b8f5d3c4cc85d095d51ec38feaa0d6844e007e';
        // A development-only 0293 receipt included one additional trailing
        // newline. Pin both exact hashes so the canonical SQL can proceed
        // without rewriting an applied migration receipt.
        const canonicalizedExecutiveEvidence = track === 'holding-write' && name === '0293_executive_evidence.sql'
          && previous.rows[0].checksum === '250b2bbc409499b500d4dfc3b667c3e9bd849a15d04749cf3047c804563790e9'
          && checksum === 'd713f4a01ef10cee409bc5a6cc40471154691a4d98903539cc698bf0bfd3964b';
        if (previous.rows[0].checksum !== checksum && !repairedLeavePredecessor && !canonicalizedExecutiveEvidence) throw new Error(`MIGRATION_CHANGED:${track}:${name}`);
        continue;
      }
      await client.query('BEGIN');
      try { await client.query(sql); await client.query(migrationReceiptSql, [name, checksum]); await client.query('COMMIT'); }
      catch (error) { await client.query('ROLLBACK'); throw error; }
    }
    await refreshCoverage(client, track);
  } finally { await client.query(MIGRATION_UNLOCK_SQL).catch(() => {}); client.release(); }
}
