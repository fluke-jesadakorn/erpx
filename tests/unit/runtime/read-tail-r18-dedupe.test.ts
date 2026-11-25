import { afterEach, describe, expect, test, vi } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { ApiError } from '../../../packages/server-runtime/src/public/http';
import {
  controlPlaneReadConfigured,
  parseControlPlaneConfig,
  planRuntimeControlPlane,
  RD_MIGRATION_RECEIPTS,
  RD_MIGRATION_CHECKSUM,
} from '../../../packages/server-runtime/src/adapters/native-runtime-control-plane';

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

// Byte oracles captured before the r18 fold. The catalog-presence family now
// lives in the native control-plane port; the TypeScript builder is deleted.
const probes = [
  ['receipt table', 'SELECT to_regclass(\'public.erpx_migrations\') IS NOT NULL present',
    '99325b8fb076cf1e15812dd9fb5e59e1a098fc2ea9a535be895340d53627aa6d'],
  ['coverage procedure', 'SELECT to_regprocedure(\'ensure_work_cache_coverage()\') IS NOT NULL present',
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

describe('runtime r18 catalog-presence family cut over to the native port', () => {
  test.each(probes)('%s probe is byte-identical in the native Rust port', (_label, before, digest) => {
    const port = rust('adapters/postgres/control_plane.rs');
    expect(port).toContain(before);
    expect(sha(before)).toBe(digest);
    expect(before.match(/\?|\$\d/g)).toBeNull();
    readOnly(before);
  });

  test('migrations.ts plans both probes and inlines neither', () => {
    const migrations = src('runtime/migrations.ts');
    expect(migrations).toContain('planRuntimeControlPlane(RD_RECEIPT_TABLE_PRESENT)');
    expect(migrations).toContain('planRuntimeControlPlane(RD_COVERAGE_PROCEDURE_PRESENT)');
    for (const probe of probes) expect(migrations, probe[1]).not.toContain(probe[1]);
    expect(migrations).toContain(".first<boolean>('present')");
    expect(migrations).toContain('.rows[0].present');
  });

  test('the TypeScript builder is deleted and no foreign caller remains', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared).not.toContain('catalogObjectExistsSql');
    expect(shared).not.toContain('selectHeadSql');
    const foreign: string[] = [];
    const collect = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) collect(path);
        else if (entry.name.endsWith('.ts') && readFileSync(path, 'utf8').includes('catalogObjectExistsSql')) foreign.push(path);
      }
    };
    collect(srcRoot);
    expect(foreign).toEqual([]);
  });

  test('session locks and unrelated read families are untouched', () => {
    const migrations = src('runtime/migrations.ts');
    expect(migrations).toContain("const MIGRATION_LOCK_SQL = 'SELECT pg_advisory_lock(714928001)'");
    expect(migrations).toContain("const MIGRATION_UNLOCK_SQL = 'SELECT pg_advisory_unlock(714928001)'");
    const postgres = src('runtime/postgres.ts');
    expect(postgres).toContain("'SELECT pg_advisory_lock(hashtextextended($1,0))'");
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

  test('retirement:ts-statements records BEFORE=30 strictly lower to 23', () => {
    expect(kpi()).toBe(23);
    expect(kpi()).toBeLessThan(30);
  });
});

const ENV = 'ERPX_NATIVE_RUNTIME_CONTROL_PLANE_CONFIG';
const cfg = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:4200',
  timeoutMs: 1000,
  holdings: [{ origin: 'https://acme.jesadakorn.com', holdingSlug: 'acme', holdingId: 'h001' }],
});
const env = (raw: string | undefined = cfg): Record<string, string | undefined> => ({ [ENV]: raw });
const reply = (body: unknown, status = 200) =>
  vi.fn(async () => ({ status, json: async () => body, body: { cancel: async () => {} } }));

afterEach(() => vi.unstubAllGlobals());

describe('native runtime control-plane read client', () => {
  test('emits no SQL and plans only registered kinds', () => {
    const client = src('native-runtime-control-plane.ts');
    expect(client).not.toMatch(/SELECT\s/i);
    const ingress = rust('adapters/control_plane_read_port.rs');
    for (const kind of [
      'runtime_rd_migration_receipts',
      'runtime_rd_migration_checksum',
      'runtime_rd_receipt_table_present',
      'runtime_rd_coverage_procedure_present',
      'runtime_rd_coverage_refresh',
      'runtime_rd_runtime_health',
    ]) {
      expect(client).toContain(kind);
      expect(ingress).toContain(kind);
    }
  });

  test('configuration fails closed', () => {
    expect(controlPlaneReadConfigured({ [ENV]: undefined })).toBe(false);
    expect(controlPlaneReadConfigured(env(cfg))).toBe(true);
    for (const raw of [
      undefined,
      '',
      'not-json',
      JSON.stringify({ ...JSON.parse(cfg), privateBaseUrl: 'http://127.0.0.1:3100' }),
      JSON.stringify({ ...JSON.parse(cfg), timeoutMs: 100 }),
      JSON.stringify({ ...JSON.parse(cfg), holdings: [] }),
    ]) expect(() => parseControlPlaneConfig(raw)).toThrow(ApiError);
  });

  test('plans pass through owner SQL and binds byte-for-byte', async () => {
    const sql = 'SELECT checksum FROM erpx_migrations WHERE name=$1';
    const fetchMock = reply({ data: { sql, values: ['a.sql'] } });
    vi.stubGlobal('fetch', fetchMock);
    expect(await planRuntimeControlPlane(RD_MIGRATION_CHECKSUM, ['a.sql'], env())).toEqual({
      sql,
      values: ['a.sql'],
    });
    const [url, init] = fetchMock.mock.calls[0] as unknown as [URL, RequestInit];
    expect(String(url)).toBe(
      'http://127.0.0.1:4200/api/v1/holdings/acme/runtime/control-plane-read-port',
    );
    expect((init.headers as Record<string, string>).host).toBe('acme.jesadakorn.com');
    expect(init.body).toBe(JSON.stringify({ kind: RD_MIGRATION_CHECKSUM, values: ['a.sql'] }));
  });

  test('rejects unknown kinds, arity mismatch and malformed replies', async () => {
    await expect(planRuntimeControlPlane('SELECT 1', [], env())).rejects.toThrow(ApiError);
    vi.stubGlobal('fetch', reply({ data: { sql: 'SELECT 1', values: [] } }));
    await expect(planRuntimeControlPlane(RD_MIGRATION_CHECKSUM, ['a.sql'], env())).rejects.toThrow(ApiError);
    vi.stubGlobal('fetch', reply({ data: { sql: 'UPDATE x SET y=1', values: [] } }));
    await expect(planRuntimeControlPlane(RD_MIGRATION_RECEIPTS, [], env())).rejects.toThrow(ApiError);
    vi.stubGlobal('fetch', reply({}, 503));
    await expect(planRuntimeControlPlane(RD_MIGRATION_RECEIPTS, [], env())).rejects.toThrow(ApiError);
  });
});

describe('runtime UTC-date read family cut over to the native port', () => {
  const utcSql = "SELECT to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') utc_date";

  test('the byte oracle is owned verbatim by the Rust port', () => {
    const ingress = rust('adapters/control_plane_read_port.rs');
    expect(ingress).toContain(utcSql);
    expect(sha(utcSql)).toBe('c1b53ea237f832b5519ec95877078c89286fa698da4e312c06fb59f0522f02de');
    expect(utcSql.length).toBe(74);
    expect(utcSql).not.toMatch(/\$\d|\?/);
    expect(utcSql).toMatch(/^SELECT /);
  });

  test('crm-evidence.ts plans the read and keeps no literal', () => {
    const crm = src('executive/crm-evidence.ts');
    expect(crm).not.toContain(utcSql);
    expect(crm).toContain('planRuntimeControlPlane(RD_UTC_DATE)');
    expect(crm).toContain('database.prepare(plan.sql).bind(...plan.values).first<{');
    expect(crm).toContain('utc_date: string;');
    const client = src('native-runtime-control-plane.ts');
    const ingress = rust('adapters/control_plane_read_port.rs');
    expect(client).toContain('runtime_rd_utc_date');
    expect(ingress).toContain('runtime_rd_utc_date');
  });

  test('the planned UTC read executes on the caller unit without binds', async () => {
    const fetchMock = reply({ data: { sql: utcSql, values: [] } });
    vi.stubGlobal('fetch', fetchMock);
    const plan = await planRuntimeControlPlane('runtime_rd_utc_date', [], env());
    expect(plan).toEqual({ sql: utcSql, values: [] });
    expect(initBody(fetchMock)).toBe(JSON.stringify({ kind: 'runtime_rd_utc_date', values: [] }));
  });
});

function initBody(mock: ReturnType<typeof reply>): unknown {
  return (mock.mock.calls[0] as unknown as [URL, RequestInit])[1].body;
}
