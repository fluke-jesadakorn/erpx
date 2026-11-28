import { afterAll, beforeAll, beforeEach, describe, expect, test } from 'vitest';
import { createServer } from 'node:http';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import {
  controlPlaneReadConfigured,
  parseControlPlaneConfig,
  planRuntimeControlPlane,
  RD_MIGRATION_RECEIPTS,
  RD_MIGRATION_CHECKSUM,
} from '../../../platform/runtime/server/src/adapters/native-runtime-control-plane';
import {
  executiveReportRevisionsSql,
  executiveReportSharesSql,
  executiveReportsSql,
  holdingBindingsSql,
  reportLockSql,
  reportVersionGuardSql,
} from '../../../platform/runtime/server/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../platform/runtime/server/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const rust = (name: string) => readFileSync(resolve(srcRoot, name), 'utf8');
const tails = () =>
  JSON.parse(readFileSync(resolve(adapterDir, 'read-tail-sources.json'), 'utf8')) as Record<string, string>;
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
    const contract = tails();
    for (const [key, table] of [
      ['holdingBindings', 'holding_bindings'],
      ['executiveGenerationJobs', 'executive_generation_jobs'],
      ['runtimeHealth', 'runtime_health'],
      ['executiveExternalTokens', 'executive_external_tokens'],
      ['executiveReportShares', 'executive_report_shares'],
      ['executiveReportRevisions', 'executive_report_revisions'],
      ['executiveReports', 'executive_reports'],
    ] as const) {
      expect(contract[key]).toContain('SELECT {columns} FROM ');
      expect(contract[key]).toContain(table);
    }
    expect(src('read-tail-sql.ts')).not.toMatch(/SELECT |INSERT |DELETE |UPDATE /);
  });

  test('retirement:ts-statements records BEFORE=30 strictly lower to 4', () => {
    expect(kpi()).toBe(4);
    expect(kpi()).toBeLessThan(30);
  });
});

const ENV = 'ERPX_NATIVE_RUNTIME_CONTROL_PLANE_CONFIG';
let cfg: string;
let response: { body: unknown; status: number };
let observed: { url: string; host: string | undefined; body: string };
const server = createServer(async (request, result) => {
  const chunks: Buffer[] = [];
  for await (const chunk of request) chunks.push(Buffer.from(chunk));
  observed = {
    url: request.url ?? '',
    host: request.headers.host,
    body: Buffer.concat(chunks).toString('utf8'),
  };
  result.writeHead(response.status, { 'content-type': 'application/json' });
  result.end(JSON.stringify(response.body));
});
beforeAll(async () => {
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('Expected a loopback listener');
  cfg = JSON.stringify({
    privateBaseUrl: `http://127.0.0.1:${address.port}`,
    timeoutMs: 1000,
    holdings: [{ origin: 'https://acme.jesadakorn.com', holdingSlug: 'acme', holdingId: 'h001' }],
  });
});
beforeEach(() => { response = { body: {}, status: 503 }; });
afterAll(() => new Promise<void>((resolve, reject) => {
  server.close(error => error ? reject(error) : resolve());
}));
const env = (raw: string | undefined = cfg): Record<string, string | undefined> => ({ [ENV]: raw });
const reply = (body: unknown, status = 200) => { response = { body, status }; };

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
    ]) expect(() => parseControlPlaneConfig(raw)).toThrow("The native Runtime control-plane service is unavailable.");
  });

  test('plans pass through owner SQL and binds byte-for-byte', async () => {
    const sql = 'SELECT checksum FROM erpx_migrations WHERE name=$1';
    reply({ data: { sql, values: ['a.sql'] } });
    expect(await planRuntimeControlPlane(RD_MIGRATION_CHECKSUM, ['a.sql'], env())).toEqual({
      sql,
      values: ['a.sql'],
    });
    expect(observed.url).toBe('/api/v1/holdings/acme/runtime/control-plane-read-port');
    expect(observed.host).toBe('acme.jesadakorn.com');
    expect(observed.body).toBe(JSON.stringify({ kind: RD_MIGRATION_CHECKSUM, values: ['a.sql'] }));
  });

  test('rejects unknown kinds, arity mismatch and malformed replies', async () => {
    await expect(planRuntimeControlPlane('SELECT 1', [], env())).rejects.toMatchObject({ status: 503, code: "RUNTIME_CONTROL_PLANE_READ_INVALID" });
    reply({ data: { sql: 'SELECT 1', values: [] } });
    await expect(planRuntimeControlPlane(RD_MIGRATION_CHECKSUM, ['a.sql'], env())).rejects.toMatchObject({ status: 503, code: "NATIVE_RUNTIME_CONTROL_PLANE_RESPONSE_INVALID" });
    reply({ data: { sql: 'UPDATE x SET y=1', values: [] } });
    await expect(planRuntimeControlPlane(RD_MIGRATION_RECEIPTS, [], env())).rejects.toMatchObject({ status: 503, code: "NATIVE_RUNTIME_CONTROL_PLANE_RESPONSE_INVALID" });
    reply({}, 503);
    await expect(planRuntimeControlPlane(RD_MIGRATION_RECEIPTS, [], env())).rejects.toMatchObject({ status: 503, code: "RUNTIME_CONTROL_PLANE_READ_FAILED" });
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
    reply({ data: { sql: utcSql, values: [] } });
    const plan = await planRuntimeControlPlane('runtime_rd_utc_date', [], env());
    expect(plan).toEqual({ sql: utcSql, values: [] });
    expect(observed.body).toBe(JSON.stringify({ kind: 'runtime_rd_utc_date', values: [] }));
  });
});


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
    reply({ data: { sql: healthSql, values: [] } });
    const plan = await planRuntimeControlPlane('runtime_rd_runtime_health', [], env());
    expect(plan).toEqual({ sql: healthSql, values: [] });
    expect(observed.body).toBe(
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
    reply({ data: { sql: lockSql, values: ['attachment-object:c:k'] } });
    expect(await planRuntimeControlPlane('runtime_rd_object_lock', ['attachment-object:c:k'], env())).toEqual({
      sql: lockSql,
      values: ['attachment-object:c:k'],
    });
    expect(observed.body).toBe(
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
    reply({ data: { sql: pairLockSql, values: ['transaction', 'aggregate-1'] } });
    expect(await planRuntimeControlPlane('runtime_rd_pair_lock', ['transaction', 'aggregate-1'], env())).toEqual({
      sql: pairLockSql,
      values: ['transaction', 'aggregate-1'],
    });
    expect(observed.body).toBe(
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
    reply({ data: { sql: lockSql, values: [] } });
    expect(await planRuntimeControlPlane('runtime_rd_migration_lock', [], env())).toEqual({
      sql: lockSql,
      values: [],
    });
    expect(observed.body).toBe(
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
    reply({ data: { sql: restoreSql, values: ['5s'] } });
    expect(await planRuntimeControlPlane('runtime_rd_lock_timeout_restore', ['5s'], env())).toEqual({
      sql: restoreSql,
      values: ['5s'],
    });
    expect(observed.body).toBe(
      JSON.stringify({ kind: 'runtime_rd_lock_timeout_restore', values: ['5s'] }),
    );
    expect(src('runtime/postgres.ts')).toContain('planRuntimeControlPlane(RD_RUNTIME_HEALTH)');
  });
});

describe('runtime read-tail builders and report guards own a native include_str! contract', () => {
  const shapes = [
    ['holdingBindings', 'SELECT {columns} FROM holding_bindings{suffix}', 46,
      'd4da402d52732e8f9cd8e3543f184d434af504e4d6c77f701d80f6426a2212f6'],
    ['executiveGenerationJobs', 'SELECT {columns} FROM executive_generation_jobs{suffix}', 55,
      'e020849b356c816f1a8d7491299686617ac3ec1fa8d9174ef9ac4ac421d079f2'],
    ['runtimeHealth', 'SELECT {columns} FROM runtime_health{suffix}', 44,
      'eb435fb5a73703cdb575e7fe29099ce6c481b9911e5e1b3cac9531d798599dcd'],
    ['executiveExternalTokens', 'SELECT {columns} FROM executive_external_tokens{suffix}', 55,
      'c727e6dbf89015e7e1620c10a2995804757c7d0f4eb0e4049390810fb0369d4c'],
    ['executiveReportShares', 'SELECT {columns} FROM executive_report_shares{suffix}', 53,
      '9f35b62c461ad16a8d831e8206fa10bb13c45904c8af698794810f9044ace1be'],
    ['executiveReportRevisions', 'SELECT {columns} FROM executive_report_revisions WHERE report_id=?{suffix}', 74,
      '63498e31e451b1227efcba1aeb279b75b5cde85d2412c999fb0bdfccf23bc0db'],
    ['executiveReports', 'SELECT {columns} FROM executive_reports{suffix}', 47,
      'bb2c4aaff57372836aba4c59297562bdec016709447ba16349b0f1df7b95ab45'],
  ] as const;

  test.each(shapes)('%s template is byte-identical in the native contract', (key, template, size, digest) => {
    const contract = tails();
    expect(contract[key]).toBe(template);
    expect(sha(contract[key]!)).toBe(digest);
    expect(contract[key]!.length).toBe(size);
    expect(contract[key]!.match(/\{columns\}/g)).toHaveLength(1);
    expect(contract[key]!.match(/\{suffix\}/g)).toHaveLength(1);
    expect(rust('adapters/read_tail.rs')).toContain('include_str!("read-tail-sources.json")');
    expect(rust('adapters/read_tail.rs')).toContain(`[serde(rename = "${key}")]`);
  });

  test('both report guards stay byte-exact with one and three binds', () => {
    const contract = tails();
    expect(contract.reportLock).toBe('SELECT id FROM executive_reports WHERE id=? FOR UPDATE');
    expect(sha(contract.reportLock!)).toBe('6bc60d98469950d7d7c62501f49dd1636875a600dbfd77e56d229ede29e35df7');
    expect(contract.reportLock!.length).toBe(54);
    expect(contract.reportLock!.match(/\?/g)).toHaveLength(1);
    expect(contract.reportLock!).toMatch(/FOR UPDATE$/);
    expect(contract.reportVersionGuard).toBe('EXISTS(SELECT 1 FROM executive_reports WHERE id=? AND owner_id=? AND version=?)');
    expect(sha(contract.reportVersionGuard!)).toBe('4c7f66216d0c97be6848e31c1b5a4cfd921e6886f00557346c346e076257a6d3');
    expect(contract.reportVersionGuard!.length).toBe(79);
    expect(contract.reportVersionGuard!.match(/\?/g)).toHaveLength(3);
    expect(contract.reportVersionGuard!).toMatch(/^EXISTS\(/);
  });

  test('the helper renders every closed family byte-identically and keeps $ binds', () => {
    expect(holdingBindingsSql('1', ' WHERE holding_id=$1')).toBe('SELECT 1 FROM holding_bindings WHERE holding_id=$1');
    expect(executiveReportsSql('*', ' WHERE id=?')).toBe('SELECT * FROM executive_reports WHERE id=?');
    expect(executiveReportSharesSql('id,version', ' WHERE report_id=?')).toBe(
      'SELECT id,version FROM executive_report_shares WHERE report_id=?',
    );
    expect(executiveReportRevisionsSql('version', ' ORDER BY version')).toBe(
      'SELECT version FROM executive_report_revisions WHERE report_id=? ORDER BY version',
    );
    expect(reportLockSql).toBe('SELECT id FROM executive_reports WHERE id=? FOR UPDATE');
    expect(reportVersionGuardSql).toBe('EXISTS(SELECT 1 FROM executive_reports WHERE id=? AND owner_id=? AND version=?)');
  });

  test('the TypeScript helper imports the contract and authors no statement text', () => {
    const helper = src('read-tail-sql.ts');
    for (const token of ['SELECT', 'INSERT', 'DELETE', 'UPDATE']) expect(helper).not.toContain(token);
    expect(helper).toContain('read-tail-sources.json');
    expect(helper).toContain('{columns}');
    expect(helper).toContain('{suffix}');
    const reports = src('executive/reports.ts');
    expect(reports).toContain('reportLockSql');
    expect(reports).toContain('reportVersionGuardSql');
    expect(reports).not.toContain('SELECT id FROM executive_reports WHERE id=? FOR UPDATE');
    expect(reports).not.toContain('EXISTS(SELECT 1 FROM executive_reports WHERE id=? AND owner_id=? AND version=?)');
  });

  test('the native module and public facade expose all nine frozen shapes', () => {
    const native = rust('adapters/read_tail.rs');
    for (const field of shapes.map(([key]) => `[serde(rename = "${key}")]`)) expect(native).toContain(field);
    const port = rust('public/read_tail.rs');
    for (const symbol of [
      'holding_bindings',
      'executive_generation_jobs',
      'runtime_health',
      'executive_external_tokens',
      'executive_report_shares',
      'executive_report_revisions',
      'executive_reports',
      'report_lock',
      'report_version_guard',
    ]) expect(port).toContain(symbol);
  });

  test('the cross-owner dynamic metric-source builder is recorded as excluded', () => {
    const metric = src('executive/metric-sources.ts');
    expect(selectCount(metric)).toBe(4);
    expect(metric).toContain('export function metricSourceQuery');
    expect(metric).toContain('export async function readMetricSources');
  });

  test('retirement:ts-statements records BEFORE=13 strictly lower to 4', () => {
    expect(kpi()).toBe(4);
    expect(kpi()).toBeLessThan(13);
  });
});
