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
    expect(migrations).toContain("planRuntimeControlPlane(RD_MIGRATION_LOCK)");
    expect(migrations).toContain("planRuntimeControlPlane(RD_MIGRATION_UNLOCK)");
    const postgres = src('runtime/postgres.ts');
    expect(postgres).toContain("planRuntimeControlPlane(RD_OBJECT_LOCK,[lock])");
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

  test('retirement:ts-statements records BEFORE=30 strictly lower to 13', () => {
    expect(kpi()).toBe(13);
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

describe('runtime session-health read family cut over to the native port', () => {
  const healthSql = "SELECT current_setting('lock_timeout') AS value";

  test('the byte oracle is owned verbatim by the Rust control-plane port', () => {
    const port = rust('adapters/postgres/control_plane.rs');
    const ingress = rust('adapters/control_plane_read_port.rs');
    expect(port).toContain(healthSql);
    expect(ingress).toContain('RD_RUNTIME_HEALTH');
    expect(sha(healthSql)).toBe('689f028e520f23f09000751de1b0f09a1ff13e458ae129bfc8c6712d76bea3d2');
    expect(healthSql.length).toBe(47);
    expect(healthSql).not.toMatch(/\$\d|\?/);
  });

  test('postgres.ts plans the read on the caller client and keeps no literal', () => {
    const postgres = src('runtime/postgres.ts');
    expect(postgres).not.toContain(healthSql);
    expect(postgres).toContain('planRuntimeControlPlane(RD_RUNTIME_HEALTH)');
    expect(postgres).toContain('client.query<{value:string}>(healthPlan.sql,[...healthPlan.values])');
    expect(postgres).toContain("if(!priorTimeout)throw new Error('OBJECT_LOCK_TIMEOUT_UNAVAILABLE')");
    expect(src('native-runtime-control-plane.ts')).toContain('runtime_rd_runtime_health');
  });

  test('the planned health read keeps its projection and bind count', async () => {
    const fetchMock = reply({ data: { sql: healthSql, values: [] } });
    vi.stubGlobal('fetch', fetchMock);
    const plan = await planRuntimeControlPlane('runtime_rd_runtime_health', [], env());
    expect(plan).toEqual({ sql: healthSql, values: [] });
    expect(initBody(fetchMock)).toBe(
      JSON.stringify({ kind: 'runtime_rd_runtime_health', values: [] }),
    );
  });
});

describe('runtime object advisory-lock family cut over to the native planner', () => {
  const lockSql = 'SELECT pg_advisory_lock(hashtextextended($1,0))';
  const unlockSql = 'SELECT pg_advisory_unlock(hashtextextended($1,0)) AS released';
  const xactSql = 'SELECT pg_advisory_xact_lock(hashtextextended($1,0))';
  const oracles = [
    ['runtime_rd_object_lock', lockSql, 47, '2178dcd09cbf4b262537e2a65e44072d2489dc5ad6ab6e0550834573a06f8af2'],
    ['runtime_rd_object_unlock', unlockSql, 61, '0fe1c33ce60de290dafc243a4438bf222c0f00c91c24967112d47cad77157a9b'],
    ['runtime_rd_object_xact_lock', xactSql, 52, '1a6751ba70ff65d15e989d0a11b3d8d51cabe9cfb325027136238a3b25d84f8e'],
  ] as const;

  test.each(oracles)('%s is byte-identical in the Rust port', (kind, sql, size, digest) => {
    const port = rust('adapters/postgres/control_plane.rs');
    const ingress = rust('adapters/control_plane_read_port.rs');
    expect(port).toContain(sql);
    expect(ingress).toContain(kind);
    expect(sha(sql)).toBe(digest);
    expect(sql.length).toBe(size);
    expect((sql.match(/\$\d/g) ?? []).length).toBe(1);
  });

  test('postgres.ts plans every object lock and inlines none', () => {
    const postgres = src('runtime/postgres.ts');
    for (const sql of [lockSql, unlockSql, xactSql]) expect(postgres).not.toContain(sql);
    for (const kind of ['RD_OBJECT_LOCK', 'RD_OBJECT_UNLOCK', 'RD_OBJECT_XACT_LOCK'])
      expect(postgres).toContain(`planRuntimeControlPlane(${kind}`);
    expect(postgres).toContain('client.query(lockPlan.sql,[...lockPlan.values])');
    expect(postgres).toContain('client.query<{released:boolean}>(unlockPlan.sql,[...unlockPlan.values])');
    expect(postgres).toContain('this.pool.query(plan.sql,[...plan.values])');
    const client = src('native-runtime-control-plane.ts');
    for (const kind of ['runtime_rd_object_lock', 'runtime_rd_object_unlock', 'runtime_rd_object_xact_lock'])
      expect(client).toContain(kind);
  });

  test('the planned lock keeps the exact key bind and reply shape', async () => {
    const fetchMock = reply({ data: { sql: lockSql, values: ['attachment-object:c:k'] } });
    vi.stubGlobal('fetch', fetchMock);
    expect(await planRuntimeControlPlane('runtime_rd_object_lock', ['attachment-object:c:k'], env())).toEqual({
      sql: lockSql,
      values: ['attachment-object:c:k'],
    });
    expect(initBody(fetchMock)).toBe(
      JSON.stringify({ kind: 'runtime_rd_object_lock', values: ['attachment-object:c:k'] }),
    );
  });
});

describe('runtime pair-key transaction lock family cut over to the native planner', () => {
  const pairLockSql = 'SELECT pg_advisory_lock(hashtext($1),hashtext($2))';
  const pairUnlockSql = 'SELECT pg_advisory_unlock(hashtext($1),hashtext($2)) released';
  const oracles = [
    ['runtime_rd_pair_lock', pairLockSql, 50, '2dbd2f0b999bdbcb04183c72ac5a56e464e87e58766a92440429048278430489'],
    ['runtime_rd_pair_unlock', pairUnlockSql, 61, '12a7fb9aea50f41132ebf24fd0a0a52cdb8725dd104c066b21086d0ecba86183'],
  ] as const;

  test.each(oracles)('%s is byte-identical in the Rust port', (kind, sql, size, digest) => {
    const port = rust('adapters/postgres/control_plane.rs');
    const ingress = rust('adapters/control_plane_read_port.rs');
    expect(port).toContain(sql);
    expect(ingress).toContain(kind);
    expect(sha(sql)).toBe(digest);
    expect(sql.length).toBe(size);
    expect((sql.match(/\$\d/g) ?? []).length).toBe(2);
  });

  test('postgres.ts plans both pair statements and inlines neither', () => {
    const postgres = src('runtime/postgres.ts');
    for (const sql of [pairLockSql, pairUnlockSql]) expect(postgres).not.toContain(sql);
    expect(postgres).toContain('planRuntimeControlPlane(RD_PAIR_LOCK,[lock.namespace,lock.key])');
    expect(postgres).toContain('planRuntimeControlPlane(RD_PAIR_UNLOCK,[lock!.namespace,lock!.key])');
    expect(postgres).toContain('client.query(plan.sql,[...plan.values]);locked=true');
    expect(postgres).toContain('client.query(plan.sql,[...plan.values]);');
    const client = src('native-runtime-control-plane.ts');
    for (const kind of ['runtime_rd_pair_lock', 'runtime_rd_pair_unlock']) expect(client).toContain(kind);
  });

  test('a planned pair lock keeps namespace/key bind order', async () => {
    const fetchMock = reply({ data: { sql: pairLockSql, values: ['transaction', 'aggregate-1'] } });
    vi.stubGlobal('fetch', fetchMock);
    expect(await planRuntimeControlPlane('runtime_rd_pair_lock', ['transaction', 'aggregate-1'], env())).toEqual({
      sql: pairLockSql,
      values: ['transaction', 'aggregate-1'],
    });
    expect(initBody(fetchMock)).toBe(
      JSON.stringify({ kind: 'runtime_rd_pair_lock', values: ['transaction', 'aggregate-1'] }),
    );
  });
});

describe('runtime migration advisory-lock family cut over to the native planner', () => {
  const lockSql = 'SELECT pg_advisory_lock(714928001)';
  const unlockSql = 'SELECT pg_advisory_unlock(714928001)';
  const oracles = [
    ['runtime_rd_migration_lock', lockSql, 34, '60698f8193884d5ba81ccadadf6db08300c3cfcb37c93f3f2d6e55e9288103a3'],
    ['runtime_rd_migration_unlock', unlockSql, 36, 'efbd4e23974b6945dc8e6102bc215bac8b9b572baaaa7af238c47be9553799c1'],
  ] as const;

  test.each(oracles)('%s is byte-identical in the Rust port', (kind, sql, size, digest) => {
    const port = rust('adapters/postgres/control_plane.rs');
    const ingress = rust('adapters/control_plane_read_port.rs');
    expect(port).toContain(sql);
    expect(ingress).toContain(kind);
    expect(sha(sql)).toBe(digest);
    expect(sql.length).toBe(size);
    expect(sql).not.toMatch(/\$\d|\?/);
  });

  test('migrations.ts plans both locks on the caller connection and inlines neither', () => {
    const migrations = src('runtime/migrations.ts');
    for (const sql of [lockSql, unlockSql]) expect(migrations).not.toContain(sql);
    expect(migrations).toContain('async function migrationLock(client: PoolClient)');
    expect(migrations).toContain('planRuntimeControlPlane(RD_MIGRATION_LOCK)');
    expect(migrations).toContain('planRuntimeControlPlane(RD_MIGRATION_UNLOCK)');
    expect(migrations).toContain('await migrationLock(client);');
    expect(migrations).toContain('await migrationUnlock(client).catch(() => {}); client.release();');
    const client = src('native-runtime-control-plane.ts');
    for (const kind of ['runtime_rd_migration_lock', 'runtime_rd_migration_unlock']) expect(client).toContain(kind);
  });

  test('the planned migration lock is bind-free', async () => {
    const fetchMock = reply({ data: { sql: lockSql, values: [] } });
    vi.stubGlobal('fetch', fetchMock);
    expect(await planRuntimeControlPlane('runtime_rd_migration_lock', [], env())).toEqual({
      sql: lockSql,
      values: [],
    });
    expect(initBody(fetchMock)).toBe(
      JSON.stringify({ kind: 'runtime_rd_migration_lock', values: [] }),
    );
  });
});

describe('runtime lock-timeout session family cut over to the native planner', () => {
  const setSql = "SELECT set_config('lock_timeout','10s',false)";
  const restoreSql = "SELECT set_config('lock_timeout',$1,false)";
  const oracles = [
    ['runtime_rd_lock_timeout_set', setSql, 45, 'b9e9a6159c864803e689ef1c4c8ac7a92f84446c584321e8c642c439a7aca227', 0],
    ['runtime_rd_lock_timeout_restore', restoreSql, 42, '9d794c5838b4078ba06bd7092af9bb7803acce2e2d06a112f1cc0540e700c4a7', 1],
  ] as const;

  test.each(oracles)('%s is byte-identical in the Rust port', (kind, sql, size, digest, binds) => {
    const port = rust('adapters/postgres/control_plane.rs');
    const ingress = rust('adapters/control_plane_read_port.rs');
    expect(port).toContain(sql);
    expect(ingress).toContain(kind);
    expect(sha(sql)).toBe(digest);
    expect(sql.length).toBe(size);
    expect((sql.match(/\$\d/g) ?? []).length).toBe(binds);
  });

  test('postgres.ts plans both session settings on the pinned client and inlines neither', () => {
    const postgres = src('runtime/postgres.ts');
    for (const sql of [setSql, restoreSql]) expect(postgres).not.toContain(sql);
    expect(postgres).toContain('planRuntimeControlPlane(RD_LOCK_TIMEOUT_SET)');
    expect(postgres).toContain('planRuntimeControlPlane(RD_LOCK_TIMEOUT_RESTORE,[priorTimeout])');
    expect(postgres).toContain('client.query(timeoutPlan.sql,[...timeoutPlan.values])');
    expect(postgres).toContain('client.query(restorePlan.sql,[...restorePlan.values])');
    expect(postgres).toContain('}catch{discard=true;}');
    const client = src('native-runtime-control-plane.ts');
    for (const kind of ['runtime_rd_lock_timeout_set', 'runtime_rd_lock_timeout_restore'])
      expect(client).toContain(kind);
  });

  test('restore keeps the prior timeout bind and the health projection is unchanged', async () => {
    const fetchMock = reply({ data: { sql: restoreSql, values: ['5s'] } });
    vi.stubGlobal('fetch', fetchMock);
    expect(await planRuntimeControlPlane('runtime_rd_lock_timeout_restore', ['5s'], env())).toEqual({
      sql: restoreSql,
      values: ['5s'],
    });
    expect(initBody(fetchMock)).toBe(
      JSON.stringify({ kind: 'runtime_rd_lock_timeout_restore', values: ['5s'] }),
    );
    expect(src('runtime/postgres.ts')).toContain('planRuntimeControlPlane(RD_RUNTIME_HEALTH)');
  });
});
