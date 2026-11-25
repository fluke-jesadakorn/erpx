import { describe, expect, test } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';

const srcRoot = resolve(__dirname, '../../../packages/server-runtime/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const rust = (name: string) => readFileSync(resolve(srcRoot, name), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Byte oracles captured before the r19 fold. The migration control-read family
// now lives in the native control-plane port; the TypeScript builder is deleted.
const reads = [
  ['receipt names', 'SELECT name FROM erpx_migration_receipts() ORDER BY name',
    '39ec7b83a3a427da244b501662b58ef5eb2aa0f1460597cf1a1b851e910e5149', 0],
  ['receipt checksum', 'SELECT checksum FROM erpx_migrations WHERE name=$1',
    '7f5c53fd80be6e47ddaa81e3590eb6458363ee6231abdc46da6dd5c8f29cfecc', 1],
] as const;

const kpi = () => {
  let total = 0;
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
    }
  };
  walk(srcRoot);
  return total;
};

describe('runtime r19 migration control reads cut over to the native port', () => {
  test.each(reads)('%s is byte-identical in the native Rust port', (_label, before, digest, binds) => {
    const port = rust('adapters/postgres/control_plane.rs');
    expect(port).toContain(before);
    expect(sha(before)).toBe(digest);
    expect((before.match(/\$\d/g) ?? []).length).toBe(binds);
    readOnly(before);
  });

  test('migrations.ts plans both reads and inlines neither', () => {
    const migrations = src('runtime/migrations.ts');
    expect(migrations).toContain('planRuntimeControlPlane(RD_MIGRATION_RECEIPTS)');
    expect(migrations).toContain('planRuntimeControlPlane(RD_MIGRATION_CHECKSUM, [name])');
    for (const read of reads) expect(migrations, read[1]).not.toContain(read[1]);
    expect(migrations).toContain(".all<{ name: string }>()");
    expect(migrations).toContain('checksumPlan.sql, [...checksumPlan.values]');
  });

  test('the TypeScript builder is deleted and no foreign caller remains', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared).not.toContain('migrationControlReadSql');
    const foreign: string[] = [];
    const collect = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) collect(path);
        else if (entry.name.endsWith('.ts') && readFileSync(path, 'utf8').includes('migrationControlReadSql')) foreign.push(path);
      }
    };
    collect(srcRoot);
    expect(foreign).toEqual([]);
  });

  test('locks, guards, jobs and unrelated helpers are untouched', () => {
    const migrations = src('runtime/migrations.ts');
    expect(migrations).toContain("const MIGRATION_LOCK_SQL = 'SELECT pg_advisory_lock(714928001)'");
    expect(migrations).toContain("const MIGRATION_UNLOCK_SQL = 'SELECT pg_advisory_unlock(714928001)'");
    const postgres = src('runtime/postgres.ts');
    expect(postgres).toContain("'SELECT pg_advisory_lock(hashtextextended($1,0))'");
    expect(postgres).not.toContain('migrationControlReadSql');
    expect(src('executive/reports.ts')).toContain('const REPORT_LOCK_SQL = "SELECT id FROM executive_reports WHERE id=? FOR UPDATE"');
    expect(src('runtime/jobs.ts')).not.toContain('migrationControlReadSql');
  });

  test('retirement:ts-statements records BEFORE=29 strictly lower to 22', () => {
    expect(kpi()).toBe(22);
    expect(kpi()).toBeLessThan(29);
  });
});
