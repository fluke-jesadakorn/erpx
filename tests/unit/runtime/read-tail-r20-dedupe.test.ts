import { describe, expect, test } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import {
  selectHeadSql,
  catalogObjectExistsSql,
  migrationControlReadSql,
} from '../../../packages/server-runtime/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-runtime/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');
const binds = (sql: string) => (sql.match(/\$\d|\?/g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Byte oracles captured from HEAD d107e637 (BEFORE=28) before the r20 bare-head
// fold. Only the shared `SELECT ` prefix moved into selectHeadSql; every folded
// renderer must emit the identical SQL, length, hash and bind count.
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

describe('runtime read-tail r20 bare-head fold', () => {
  test('every folded renderer stays byte-identical to the pre-fold HEAD', () => {
    expect(selectHeadSql).toBe('SELECT ');
    check(catalogObjectExistsSql('to_regclass', 'public.erpx_migrations'), folded.receiptTable!);
    check(catalogObjectExistsSql('to_regprocedure', 'ensure_work_cache_coverage()'), folded.coverageProcedure!);
    check(migrationControlReadSql('name', 'erpx_migration_receipts()', ' ORDER BY name'), folded.receiptNames!);
    check(migrationControlReadSql('checksum', 'erpx_migrations', ' WHERE name=$1'), folded.receiptChecksum!);
  });

  test('the bare head lives once and the two folded heads delegate to it', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.match(/export const selectHeadSql =/g) ?? []).toHaveLength(1);
    expect(shared.match(/'SELECT '/g) ?? []).toHaveLength(1);
    expect(shared.match(/\$\{selectHeadSql\}\$\{fn\}\('\$\{name\}'\) IS NOT NULL present/g) ?? []).toHaveLength(1);
    expect(shared.match(/\$\{selectHeadSql\}\$\{columns\} FROM \$\{relation\}\$\{suffix\}/g) ?? []).toHaveLength(1);
    expect(shared.match(/\$\{selectHeadSql\}/g) ?? []).toHaveLength(2);
    // The r1-r17 relation helpers are deliberately not re-folded.
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

  test('both folded families are composed by migrations.ts, never inlined', () => {
    const migrations = src('runtime/migrations.ts');
    expect(migrations).toContain("import { catalogObjectExistsSql, migrationControlReadSql } from '../read-tail-sql';");
    for (const oracle of Object.values(folded)) expect(migrations, oracle[0]).not.toContain(oracle[0]);
    expect(migrations.match(/catalogObjectExistsSql\(/g) ?? []).toHaveLength(2);
    expect(migrations.match(/migrationControlReadSql\(/g) ?? []).toHaveLength(2);
    // Binds and projections stay at the call site in their exact order.
    expect(migrations).toContain("catalogObjectExistsSql('to_regclass', 'public.erpx_migrations')");
    expect(migrations).toContain("catalogObjectExistsSql('to_regprocedure', 'ensure_work_cache_coverage()')");
    expect(migrations).toContain("migrationControlReadSql('name', 'erpx_migration_receipts()', ' ORDER BY name')");
    expect(migrations).toContain("migrationControlReadSql('checksum', 'erpx_migrations', ' WHERE name=$1')");
    expect(migrations).toContain('.first<boolean>(\'present\')');
    expect(migrations).toContain('.rows[0].present');
    expect(migrations).toContain("client.query(migrationControlReadSql('checksum', 'erpx_migrations', ' WHERE name=$1'), [name])");
  });

  test('no other owner file calls the folded helpers', () => {
    const ownerFiles = new Set([
      resolve(adapterDir, 'read-tail-sql.ts'),
      resolve(adapterDir, 'runtime/migrations.ts'),
    ]);
    const foreign: string[] = [];
    const collect = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) collect(path);
        else if (entry.name.endsWith('.ts') && !ownerFiles.has(path)
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
    expect(migrations).toContain("await client.query('SELECT ensure_work_cache_coverage()')");
    const postgres = src('runtime/postgres.ts');
    expect(postgres).toContain("'SELECT pg_advisory_lock(hashtextextended($1,0))'");
    expect(postgres).not.toMatch(/selectHeadSql|catalogObjectExistsSql|migrationControlReadSql/);
    const reports = src('executive/reports.ts');
    expect(reports).toContain('const REPORT_LOCK_SQL = "SELECT id FROM executive_reports WHERE id=? FOR UPDATE"');
    expect(reports).not.toMatch(/selectHeadSql|catalogObjectExistsSql|migrationControlReadSql/);
    expect(src('runtime/jobs.ts')).not.toMatch(/selectHeadSql|catalogObjectExistsSql|migrationControlReadSql/);
    for (const sql of Object.values(folded).map(oracle => oracle[0])) {
      expect(sql).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|pg_advisory|@erpx\/server-|FROM\s+executive_reports/);
    }
  });

  test('retirement:ts-statements records BEFORE=28 strictly lower to 27', () => {
    expect(kpi()).toBe(27);
    expect(kpi()).toBeLessThan(28);
  });
});
