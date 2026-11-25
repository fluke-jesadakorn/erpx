import { describe, expect, test } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { catalogObjectExistsSql } from '../../../packages/server-runtime/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-runtime/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Byte oracles captured at HEAD before the r18 fold, rendered by the two inline
// migration catalog probes. The helper now passes the same fixed function and
// trusted literal name; rendered SQL and binds (both statements are bind-free)
// must not change.
const probes = [
  ['receipt table', 'to_regclass', 'public.erpx_migrations',
    "SELECT to_regclass('public.erpx_migrations') IS NOT NULL present",
    '99325b8fb076cf1e15812dd9fb5e59e1a098fc2ea9a535be895340d53627aa6d'],
  ['coverage procedure', 'to_regprocedure', 'ensure_work_cache_coverage()',
    "SELECT to_regprocedure('ensure_work_cache_coverage()') IS NOT NULL present",
    '67f61d9f5431fb9214ad80eaf84c8b21fb1309bc3d87c8f0ce9bedcccaeeb967'],
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

describe('runtime read-tail r18 catalog-object presence head family', () => {
  test.each(probes)('%s probe is byte-identical to the pre-dedupe rendering', (_label, fn, name, before, digest) => {
    const sql = catalogObjectExistsSql(fn, name);
    expect(sql).toBe(before);
    expect(sha(sql)).toBe(digest);
    expect(sql.match(/\?|\$\d/g)).toBeNull();
    readOnly(sql);
  });

  test('both migration probes are composed by the named adapter, never inlined', () => {
    const migrations = src('runtime/migrations.ts');
    expect(migrations).toContain("import { catalogObjectExistsSql, migrationControlReadSql } from '../read-tail-sql';");
    for (const probe of probes) expect(migrations, probe[3]).not.toContain(probe[3]);
    expect(migrations).toContain("catalogObjectExistsSql('to_regclass', 'public.erpx_migrations')");
    expect(migrations).toContain("catalogObjectExistsSql('to_regprocedure', 'ensure_work_cache_coverage()')");
    expect(migrations.match(/catalogObjectExistsSql\(/g)).toHaveLength(2);
    // Bind-free call sites keep their exact result projection.
    expect(migrations).toContain(".first<boolean>('present')");
    expect(migrations).toContain('.rows[0].present');
  });

  test('the head lives once and no other owner file calls it', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.match(/\$\{selectHeadSql\}\$\{fn\}\('\$\{name\}'\) IS NOT NULL present/g)).toHaveLength(1);
    // No foreign call site: the only helper calls are the two in migrations.ts.
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
          && readFileSync(path, 'utf8').includes('catalogObjectExistsSql')) foreign.push(path);
      }
    };
    collect(srcRoot);
    expect(foreign).toEqual([]);
  });

  test('control-plane locks and prior helpers are untouched', () => {
    const migrations = src('runtime/migrations.ts');
    expect(migrations).toContain("const MIGRATION_LOCK_SQL = 'SELECT pg_advisory_lock(714928001)'");
    expect(migrations).toContain("const MIGRATION_UNLOCK_SQL = 'SELECT pg_advisory_unlock(714928001)'");
    expect(migrations).toContain("migrationControlReadSql('name', 'erpx_migration_receipts()', ' ORDER BY name')");
    expect(migrations).toContain("migrationControlReadSql('checksum', 'erpx_migrations', ' WHERE name=$1')");
    const postgres = src('runtime/postgres.ts');
    expect(postgres).toContain("'SELECT pg_advisory_lock(hashtextextended($1,0))'");
    expect(postgres).not.toContain('catalogObjectExistsSql');
    expect(src('executive/reports.ts')).toContain('const REPORT_LOCK_SQL = "SELECT id FROM executive_reports WHERE id=? FOR UPDATE"');
    const shared = src('read-tail-sql.ts');
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

  test('retirement:ts-statements records BEFORE=30 strictly lower to 27', () => {
    expect(kpi()).toBe(26);
    expect(kpi()).toBeLessThan(30);
  });
});
