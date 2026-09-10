import { createHash } from 'node:crypto';
import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import type { PostgresDatabase } from './postgres';

export async function migrateDatabase(db: PostgresDatabase, track: string, root = join(process.cwd(), 'migrations/postgres')) {
  if (!['control','control-read','holding-write','holding-read'].includes(track)) throw new Error('INVALID_MIGRATION_TRACK');
  const client = await db.pool.connect();
  try {
    await client.query('SELECT pg_advisory_lock(714928001)');
    await client.query('CREATE TABLE IF NOT EXISTS erpx_migrations(name TEXT PRIMARY KEY,checksum TEXT NOT NULL,applied_at TIMESTAMPTZ NOT NULL DEFAULT now())');
    for (const name of (await readdir(join(root, track))).filter(f => f.endsWith('.sql')).sort()) {
      const sql = await readFile(join(root, track, name), 'utf8'), checksum = createHash('sha256').update(sql).digest('hex');
      const previous = await client.query('SELECT checksum FROM erpx_migrations WHERE name=$1', [name]);
      if (previous.rows.length) {
        if (previous.rows[0].checksum !== checksum) throw new Error(`MIGRATION_CHANGED:${track}:${name}`);
        continue;
      }
      await client.query('BEGIN');
      try { await client.query(sql); await client.query('INSERT INTO erpx_migrations(name,checksum) VALUES($1,$2)', [name,checksum]); await client.query('COMMIT'); }
      catch (error) { await client.query('ROLLBACK'); throw error; }
    }
  } finally { await client.query('SELECT pg_advisory_unlock(714928001)').catch(() => {}); client.release(); }
}
