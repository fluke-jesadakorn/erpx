import { describe, expect, test } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { migrationControlReadSql } from '../../../packages/server-runtime/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-runtime/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Byte oracles captured at HEAD before the r19 fold: the two migration control
// reads in migrations.ts. The helper passes the same columns, closed relation
// literal and suffix, so rendered SQL and bind placeholders are unchanged.
const reads = [
  ['receipt names', 'name', 'erpx_migration_receipts()', ' ORDER BY name',
    'SELECT name FROM erpx_migration_receipts() ORDER BY name',
    '39ec7b83a3a427da244b501662b58ef5eb2aa0f1460597cf1a1b851e910e5149', 0],
  ['receipt checksum', 'checksum', 'erpx_migrations', ' WHERE name=$1',
    'SELECT checksum FROM erpx_migrations WHERE name=$1',
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

describe('runtime read-tail r19 migration control read head family', () => {
  test.each(reads)('%s is byte-identical to the pre-dedupe rendering', (_label, columns, relation, suffix, before, digest, binds) => {
    const sql = migrationControlReadSql(columns, relation, suffix);
    expect(sql).toBe(before);
    expect(sha(sql)).toBe(digest);
    expect((sql.match(/\$\d/g) ?? []).length).toBe(binds);
    readOnly(sql);
  });

  test('both migration reads are composed by the named adapter, never inlined', () => {
    const migrations = src('runtime/migrations.ts');
    expect(migrations).toContain("import { catalogObjectExistsSql, migrationControlReadSql } from '../read-tail-sql';");
    for (const read of reads) expect(migrations, read[4]).not.toContain(read[4]);
    expect(migrations).toContain("migrationControlReadSql('name', 'erpx_migration_receipts()', ' ORDER BY name')");
    expect(migrations).toContain("migrationControlReadSql('checksum', 'erpx_migrations', ' WHERE name=$1')");
    expect(migrations.match(/migrationControlReadSql\(/g)).toHaveLength(2);
    // Binds stay at the call site and keep their exact argument order.
    expect(migrations).toContain(".all<{ name: string }>()");
    expect(migrations).toContain("client.query(migrationControlReadSql('checksum', 'erpx_migrations', ' WHERE name=$1'), [name])");
  });

  test('the head lives once and no other owner file calls it', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('`${selectHeadSql}${columns} FROM ${relation}${suffix}`')).toHaveLength(2);
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
          && readFileSync(path, 'utf8').includes('migrationControlReadSql')) foreign.push(path);
      }
    };
    collect(srcRoot);
    expect(foreign).toEqual([]);
  });

  test('locks, guards, jobs and prior helpers are untouched', () => {
    const migrations = src('runtime/migrations.ts');
    expect(migrations).toContain("const MIGRATION_LOCK_SQL = 'SELECT pg_advisory_lock(714928001)'");
    expect(migrations).toContain("const MIGRATION_UNLOCK_SQL = 'SELECT pg_advisory_unlock(714928001)'");
    expect(migrations).toContain("catalogObjectExistsSql('to_regclass', 'public.erpx_migrations')");
    expect(migrations).toContain("catalogObjectExistsSql('to_regprocedure', 'ensure_work_cache_coverage()')");
    const postgres = src('runtime/postgres.ts');
    expect(postgres).toContain("'SELECT pg_advisory_lock(hashtextextended($1,0))'");
    expect(postgres).not.toContain('migrationControlReadSql');
    expect(src('executive/reports.ts')).toContain('const REPORT_LOCK_SQL = "SELECT id FROM executive_reports WHERE id=? FOR UPDATE"');
    expect(src('runtime/jobs.ts')).not.toContain('migrationControlReadSql');
    const shared = src('read-tail-sql.ts');
    for (const template of [
      '`SELECT ${columns} FROM holding_bindings${suffix}`',
      '`SELECT ${columns} FROM executive_generation_jobs${suffix}`',
      '`SELECT ${columns} FROM runtime_health${suffix}`',
      '`SELECT ${columns} FROM executive_external_tokens${suffix}`',
      '`SELECT ${columns} FROM executive_report_shares${suffix}`',
      '`SELECT ${columns} FROM executive_report_revisions WHERE report_id=?${suffix}`',
      '`SELECT ${columns} FROM executive_reports${suffix}`',
      "`${selectHeadSql}${fn}('${name}') IS NOT NULL present`",
    ]) expect(shared.split(template)).toHaveLength(2);
  });

  test('retirement:ts-statements records BEFORE=29 strictly lower to 27', () => {
    expect(kpi()).toBe(26);
    expect(kpi()).toBeLessThan(29);
  });
});
