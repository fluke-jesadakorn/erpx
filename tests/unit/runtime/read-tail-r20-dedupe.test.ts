import { describe, expect, test } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';

const srcRoot = resolve(__dirname, '../../../packages/server-runtime/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const rust = (name: string) => readFileSync(resolve(srcRoot, name), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');
const binds = (sql: string) => (sql.match(/\$\d|\?/g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Byte oracles captured from HEAD d107e637 (BEFORE=28). The r20 bare head and
// its two folded builders are deleted: every statement is now owned by the
// native control-plane port and planned over the axum read-port ingress.
type Oracle = readonly [sql: string, digest: string, length: number, bindCount: number];
const folded: Record<string, Oracle> = {
  receiptTable: ['SELECT to_regclass(\'public.erpx_migrations\') IS NOT NULL present',
    '99325b8fb076cf1e15812dd9fb5e59e1a098fc2ea9a535be895340d53627aa6d', 64, 0],
  coverageProcedure: ['SELECT to_regprocedure(\'ensure_work_cache_coverage()\') IS NOT NULL present',
    '67f61d9f5431fb9214ad80eaf84c8b21fb1309bc3d87c8f0ce9bedcccaeeb967', 74, 0],
  receiptNames: ['SELECT name FROM erpx_migration_receipts() ORDER BY name',
    '39ec7b83a3a427da244b501662b58ef5eb2aa0f1460597cf1a1b851e910e5149', 56, 0],
  receiptChecksum: ['SELECT checksum FROM erpx_migrations WHERE name=$1',
    '7f5c53fd80be6e47ddaa81e3590eb6458363ee6231abdc46da6dd5c8f29cfecc', 50, 1],
};
const check = (sql: string, [before, digest, length, bindCount]: Oracle) => {
  expect(sql).toBe(before);
  expect(sha(sql)).toBe(digest);
  expect(sql.length).toBe(length);
  expect(binds(sql)).toBe(bindCount);
  readOnly(sql);
};

const kpi = () => {
  let total = 0;
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
    }
  };
  walk(srcRoot);
  return total;
};

describe('runtime r20 bare-head family retired into the native port', () => {
  test('every oracle stays byte-identical in the native Rust port', () => {
    const port = rust('adapters/postgres/control_plane.rs');
    for (const oracle of Object.values(folded)) check(oracle[0], oracle);
    for (const oracle of Object.values(folded)) expect(port).toContain(oracle[0]);
  });

  test('the bare head and both folded builders are deleted', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared).not.toMatch(/selectHeadSql|catalogObjectExistsSql|migrationControlReadSql/);
    for (const template of [
      '`SELECT ${columns} FROM holding_bindings${suffix}`',
      '`SELECT ${columns} FROM executive_generation_jobs${suffix}`',
      '`SELECT ${columns} FROM runtime_health${suffix}`',
      '`SELECT ${columns} FROM executive_external_tokens${suffix}`',
      '`SELECT ${columns} FROM executive_report_shares${suffix}`',
      '`SELECT ${columns} FROM executive_report_revisions WHERE report_id=?${suffix}`',
      '`SELECT ${columns} FROM executive_reports${suffix}`',
    ]) expect(shared.split(template)).toHaveLength(2);
  });

  test('migrations.ts plans every retired statement and inlines none', () => {
    const migrations = src('runtime/migrations.ts');
    for (const oracle of Object.values(folded)) expect(migrations, oracle[0]).not.toContain(oracle[0]);
    for (const kind of [
      'RD_RECEIPT_TABLE_PRESENT',
      'RD_COVERAGE_PROCEDURE_PRESENT',
      'RD_MIGRATION_RECEIPTS',
      'RD_MIGRATION_CHECKSUM',
      'RD_COVERAGE_REFRESH',
    ]) expect(migrations).toContain(`planRuntimeControlPlane(${kind}`);
    expect(migrations).toContain('.first<boolean>(\'present\')');
    expect(migrations).toContain('.rows[0].present');
    expect(migrations).toContain('checksumPlan.sql, [...checksumPlan.values]');
  });

  test('no other owner file calls the deleted helpers', () => {
    const foreign: string[] = [];
    const collect = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) collect(path);
        else if (entry.name.endsWith('.ts')
          && /catalogObjectExistsSql|migrationControlReadSql|selectHeadSql/.test(readFileSync(path, 'utf8'))) foreign.push(path);
      }
    };
    collect(srcRoot);
    expect(foreign).toEqual([]);
  });

  test('locks, jobs, native kinds and cross-owner reads are untouched', () => {
    const migrations = src('runtime/migrations.ts');
    expect(migrations).toContain("const MIGRATION_LOCK_SQL = 'SELECT pg_advisory_lock(714928001)'");
    expect(migrations).toContain("const MIGRATION_UNLOCK_SQL = 'SELECT pg_advisory_unlock(714928001)'");
    const postgres = src('runtime/postgres.ts');
    expect(postgres).toContain("'SELECT pg_advisory_lock(hashtextextended($1,0))'");
    expect(postgres).not.toMatch(/selectHeadSql|catalogObjectExistsSql|migrationControlReadSql/);
    const reports = src('executive/reports.ts');
    expect(reports).toContain('const REPORT_LOCK_SQL = "SELECT id FROM executive_reports WHERE id=? FOR UPDATE"');
    expect(reports).not.toMatch(/selectHeadSql|catalogObjectExistsSql|migrationControlReadSql/);
    for (const sql of Object.values(folded).map(oracle => oracle[0])) {
      expect(sql).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|pg_advisory|@erpx\/server-|FROM\s+executive_reports/);
    }
  });

  test('retirement:ts-statements records BEFORE=28 strictly lower to 23', () => {
    expect(kpi()).toBe(23);
    expect(kpi()).toBeLessThan(28);
  });
});
