/** Vertical boot probe: leased PostgreSQL + ephemeral private-api composition for one suite.
 *
 * Spike for lead run-20261123-d. Given a suite id (currently only
 * `inventory.transit.integration`):
 *  1. creates leased PostgreSQL databases exactly like
 *     tests/support/rust-postgres.rs (admin ERPX_TEST_ADMIN_URL, database
 *     erpx_dev_rust_<hex>, REVOKE CONNECT FROM PUBLIC + lease COMMENT, local
 *     127.0.0.1:5432/postgres erpx_test_admin assertions, ownership/lease
 *     verification before drop),
 *  2. migrates them (Control track for the leased control DB, HoldingWrite
 *     track for the leased holding DBs) and inserts a minimal probe row
 *     (one organization_nodes company row `probe-co`, mirroring the seed
 *     pattern in the rust-supply-package-obligations native test),
 *  3. writes an ephemeral development config (mirroring the
 *     .scratch/dev/v4.json/v5.json shape) on a free loopback port >= 3310
 *     and boots ./target/debug/erpx-server (rebuilt with
 *     --features private-development-api only when older than
 *     platform/composition/src/collaboration.rs) as
 *     `private-api --development-config <tmp>`, waits for /api/health 200
 *     (60s), then probes POST
 *     /api/v1/holdings/acme/collaboration/notification-statement expecting
 *     UNAUTHENTICATED and holdings/nope expecting NOT_FOUND,
 *  4. prints the exact ERPX_NATIVE_* env matrix a lane would need for this
 *     instance in a form test.ts could inject, then tears down: kills the
 *     instance, verifies the port is free and no erpx-server child remains,
 *     and drops ONLY the leased DBs after re-verifying the exact lease
 *     comment (guard pattern copied from rust-postgres.rs).
 *
 * Usage: bun scripts/testing/boot-suite-composition.ts
 *          --suite=inventory.transit.integration [--keep|--serve]
 * --keep skips teardown for debugging; the default tears everything down.
 * --serve keeps the instance up for a lane runner: after printing the env
 * matrix it waits for stdin EOF, then tears everything down the normal way.
 * test.ts spawns --serve, injects the matrix, runs the lane, closes stdin.
 */
import { spawn, type ChildProcess } from 'node:child_process';
import { randomBytes, createHash } from 'node:crypto';
import { chmodSync, existsSync, mkdirSync, readdirSync, readFileSync, statSync, unlinkSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Client } from 'pg';

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const root = resolve(scriptDirectory, '../..');

const SUPPORTED_SUITES = ['inventory.transit.integration'] as const;
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const FIRST_PROBE_PORT = 3310;
const HEALTH_TIMEOUT_MS = 60_000;
const MIGRATION_LOCK_KEY = 714928001;
const BINARY = join(root, 'target/debug/erpx-server');
const BUILD_SENTINEL = join(root, 'platform/composition/src/collaboration.rs');
const ACCOUNT_HOST = 'account-dev-erpx.jesadakorn.com';
const HOLDING_HOSTS: Record<string, { slug: string; host: string }> = {
  h001: { slug: 'acme', host: 'acme.jesadakorn.com' },
  h002: { slug: 'northstar', host: 'northstar.jesadakorn.com' },
};
const PROBE_COMPANY = 'probe-co';

function fail(code: string, detail?: string): never {
  throw new Error(detail ? `${code}:${detail}` : code);
}

function parseArguments(argv: readonly string[]): { suite: string; keep: boolean; serve: boolean } {
  const args = argv[0] === '--' ? argv.slice(1) : [...argv];
  let suite: string | undefined;
  let keep = false;
  let serve = false;
  for (const argument of args) {
    if (argument === '--keep') keep = true;
    else if (argument === '--serve') serve = true;
    else if (argument.startsWith('--suite=')) suite = argument.slice('--suite='.length);
    else fail('BOOT_SUITE_USAGE_REQUIRED', 'expected --suite=<id> [--keep|--serve]');
  }
  if (!suite) fail('BOOT_SUITE_USAGE_REQUIRED', 'expected --suite=<id> [--keep|--serve]');
  if (!(SUPPORTED_SUITES as readonly string[]).includes(suite)) fail('BOOT_SUITE_PROBE_UNSUPPORTED', suite);
  if (keep && serve) fail('BOOT_SUITE_USAGE_REQUIRED', '--keep and --serve are exclusive');
  return { suite, keep, serve };
}

/** Admin URL resolution mirrors test.ts databaseCredentials(): shell wins, then ignored local env files. */
function adminUrl(): string {
  if (process.env.ERPX_TEST_ADMIN_URL) return process.env.ERPX_TEST_ADMIN_URL;
  for (const file of ['.env.test.local', '.env.test', '.env.local']) {
    const path = join(root, file);
    if (!existsSync(path)) continue;
    const value = readFileSync(path, 'utf8').match(/^\s*(?:export\s+)?ERPX_TEST_ADMIN_URL\s*=\s*(.*?)\s*$/m)?.[1];
    if (value) return value.replace(/^(["'])(.*)\1$/, '$2');
  }
  fail('ERPX_TEST_ADMIN_URL_REQUIRED');
}

/** Endpoint guard mirrors the asserts in tests/support/rust-postgres.rs. */
function assertAdminEndpoint(url: string): { password: string } {
  const endpoint = new URL(url);
  if (endpoint.hostname !== '127.0.0.1') fail('BOOT_SUITE_ADMIN_HOST_INVALID');
  if (endpoint.port !== '5432') fail('BOOT_SUITE_ADMIN_PORT_INVALID');
  if (endpoint.pathname !== '/postgres') fail('BOOT_SUITE_ADMIN_DATABASE_INVALID');
  if (decodeURIComponent(endpoint.username) !== 'erpx_test_admin') fail('BOOT_SUITE_ADMIN_ROLE_INVALID');
  if (!endpoint.password) fail('BOOT_SUITE_ADMIN_PASSWORD_REQUIRED');
  if (endpoint.search || endpoint.hash) fail('BOOT_SUITE_ADMIN_URL_INVALID');
  return { password: endpoint.password };
}

function leasedUrl(admin: string, token: string): string {
  const endpoint = new URL(admin);
  endpoint.pathname = `/erpx_dev_rust_${token}`;
  endpoint.search = '';
  endpoint.hash = '';
  return endpoint.toString();
}

type Lease = { token: string; name: string; lease: string; url: string };

function newLease(admin: string): Lease {
  const token = randomBytes(16).toString('hex');
  return { token, name: `erpx_dev_rust_${token}`, lease: `erpx-rust-test:${token}`, url: leasedUrl(admin, token) };
}

async function createLeasedDatabase(admin: Client, lease: Lease): Promise<void> {
  // Mirrors tests/support/rust-postgres.rs: fixed prefix + generated lowercase hex only.
  await admin.query(`CREATE DATABASE "${lease.name}" TEMPLATE template0`);
  await admin.query(`REVOKE CONNECT ON DATABASE "${lease.name}" FROM PUBLIC`);
  await admin.query(`COMMENT ON DATABASE "${lease.name}" IS '${lease.lease}'`);
  console.log(`BOOT_SUITE_DB_CREATED:${lease.name}`);
}

/** Drop guard mirrors tests/support/rust-postgres.rs verbatim in intent:
 *  only a database whose name has our prefix, whose owner is erpx_test_admin
 *  and whose comment is exactly our lease may be dropped; a 55006 race retries. */
async function dropLeasedDatabase(admin: Client, lease: Lease): Promise<void> {
  const ownership = await admin.query(
    "SELECT pg_get_userbyid(datdba)::text owner,shobj_description(oid,'pg_database') lease FROM pg_database WHERE datname=$1",
    [lease.name],
  );
  if (ownership.rowCount !== 1) fail('BOOT_SUITE_LEASE_NOT_FOUND', lease.name);
  if (!lease.name.startsWith('erpx_dev_rust_')) fail('BOOT_SUITE_LEASE_PREFIX_MISMATCH', lease.name);
  if (ownership.rows[0].owner !== 'erpx_test_admin') fail('BOOT_SUITE_LEASE_OWNER_MISMATCH', lease.name);
  if (ownership.rows[0].lease !== lease.lease) fail('BOOT_SUITE_LEASE_COMMENT_MISMATCH', lease.name);
  for (let attempt = 0; attempt < 30; attempt++) {
    try {
      await admin.query(`DROP DATABASE "${lease.name}"`);
      console.log(`BOOT_SUITE_DB_DROPPED:${lease.name}`);
      return;
    } catch (error) {
      const code = (error as { code?: string }).code;
      if (code === '55006' && attempt < 29) {
        await new Promise(resolveWait => setTimeout(resolveWait, 100));
        continue;
      }
      fail('BOOT_SUITE_DROP_FAILED', `${lease.name}:${code ?? 'unknown'}`);
    }
  }
}

function migrationFiles(track: 'control' | 'holding-write'): { name: string; sql: string; checksum: string }[] {
  const directory = join(root, 'migrations/postgres', track);
  const files = readdirSync(directory, { withFileTypes: true })
    .filter(entry => entry.isFile() && entry.name.endsWith('.sql'))
    .map(entry => entry.name)
    .sort()
    .map(name => {
      const sql = readFileSync(join(directory, name), 'utf8');
      return { name, sql, checksum: createHash('sha256').update(Buffer.from(sql, 'utf8')).digest('hex') };
    });
  if (!files.length) fail('BOOT_SUITE_MIGRATIONS_MISSING', track);
  return files;
}

/** Applies reviewed migration bytes exactly like migrate_database in
 *  platform/runtime/server/src/adapters/postgres/migrations.rs: one advisory
 *  lock, erpx_migrations receipts, whole-file single transactions, then the
 *  HoldingWrite work-cache coverage call. */
async function migrateDatabase(url: string, track: 'control' | 'holding-write'): Promise<void> {
  const files = migrationFiles(track);
  const client = new Client({ connectionString: url });
  await client.connect();
  try {
    await client.query('SELECT pg_advisory_lock($1::bigint)', [MIGRATION_LOCK_KEY]);
    let applied = 0, skipped = 0;
    try {
      await client.query('CREATE TABLE IF NOT EXISTS erpx_migrations(name TEXT PRIMARY KEY,checksum TEXT NOT NULL,applied_at TIMESTAMPTZ NOT NULL DEFAULT now())');
      for (const file of files) {
        const previous = await client.query('SELECT checksum FROM erpx_migrations WHERE name=$1', [file.name]);
        if (previous.rowCount) {
          if (previous.rows[0].checksum !== file.checksum) fail('BOOT_SUITE_MIGRATION_CHANGED', `${track}:${file.name}`);
          skipped++;
          continue;
        }
        await client.query('BEGIN');
        try {
          await client.query(file.sql);
          await client.query('INSERT INTO erpx_migrations(name,checksum) VALUES($1,$2)', [file.name, file.checksum]);
          await client.query('COMMIT');
          applied++;
        } catch (error) {
          await client.query('ROLLBACK');
          throw error;
        }
      }
      if (track === 'holding-write') {
        const present = await client.query("SELECT to_regprocedure('ensure_work_cache_coverage()') IS NOT NULL AS present");
        if (present.rows[0].present) await client.query('SELECT ensure_work_cache_coverage()');
      }
    } finally {
      await client.query('SELECT pg_advisory_unlock($1::bigint)', [MIGRATION_LOCK_KEY]);
    }
    console.log(`BOOT_SUITE_MIGRATED:${track}:applied=${applied}:skipped=${skipped}:files=${files.length}`);
  } finally {
    await client.end();
  }
}

async function query(url: string, sql: string, parameters: unknown[] = []): Promise<{ rows: Record<string, unknown>[] }> {
  const client = new Client({ connectionString: url });
  await client.connect();
  try {
    return await client.query(sql, parameters as never[]);
  } finally {
    await client.end();
  }
}

async function seedControlBindings(controlUrl: string, holdings: { id: string; write: string; read: string; files: string }[]): Promise<void> {
  const deadline = Date.now() + 20_000;
  for (;;) {
    try {
      for (const holding of holdings) {
        const { slug } = HOLDING_HOSTS[holding.id];
        await query(
          controlUrl,
          `INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status)
           VALUES($1,$2,$3,$4,$5,$6,'active')
           ON CONFLICT (holding_id) DO UPDATE SET slug=excluded.slug,display_name=excluded.display_name,
             write_binding=excluded.write_binding,read_binding=excluded.read_binding,
             files_binding=excluded.files_binding,status='active'`,
          [holding.id, slug, `Boot probe ${holding.id}`, holding.write, holding.read, holding.files],
        );
      }
      console.log('BOOT_SUITE_CONTROL_SEEDED:holding_bindings=h001,h002');
      return;
    } catch (error) {
      if (Date.now() >= deadline) throw error;
      await new Promise(resolveWait => setTimeout(resolveWait, 100));
    }
  }
}

async function pickPort(): Promise<number> {
  for (let port = FIRST_PROBE_PORT; port < FIRST_PROBE_PORT + 100; port++) {
    if (RESERVED_PORTS.has(port) || (port >= 3200 && port <= 3299)) continue;
    try {
      const probe = createServer();
      await new Promise<void>((resolveProbe, rejectProbe) => {
        probe.once('error', rejectProbe);
        probe.listen(port, '127.0.0.1', () => resolveProbe());
      });
      await new Promise<void>(resolveClose => probe.close(() => resolveClose()));
      console.log(`BOOT_SUITE_PORT_FREE:${port}`);
      return port;
    } catch {
      // Occupied; keep scanning upward.
    }
  }
  fail('BOOT_SUITE_PORT_UNAVAILABLE', String(FIRST_PROBE_PORT));
}

function ephemeralConfiguration(port: number, control: Lease, holdings: Record<string, Lease>): Record<string, unknown> {
  const holdingEntries = Object.entries(holdings).map(([id, lease]) => ({
    holdingId: id,
    slug: HOLDING_HOSTS[id].slug,
    host: HOLDING_HOSTS[id].host,
    writeBinding: `${id.toUpperCase()}_DB`,
    readBinding: `${id.toUpperCase()}_READ_DB`,
    filesBinding: `${id.toUpperCase()}_FILES`,
    database: { url: lease.url, leaseToken: lease.token },
  }));
  return {
    version: 1,
    mode: 'development',
    transport: 'direct-loopback',
    listen: `127.0.0.1:${port}`,
    accountHost: ACCOUNT_HOST,
    requireEmailVerification: true,
    passwordWorkers: 2,
    maximumRequests: 16,
    requestTimeoutMs: 10000,
    accountBootstrap: true,
    control: { url: control.url, leaseToken: control.token },
    holdings: holdingEntries,
    financeJournalReads: [{ holdingId: 'h001', companyId: PROBE_COMPANY }],
    operationsCompanies: [{ holdingId: 'h001', companyId: PROBE_COMPANY }],
  };
}

async function ensureBinary(): Promise<void> {
  const fresh = existsSync(BINARY) && statSync(BINARY).mtimeMs >= statSync(BUILD_SENTINEL).mtimeMs;
  if (fresh) {
    console.log('BOOT_SUITE_BINARY_REUSED:target/debug/erpx-server');
    return;
  }
  // At most one cargo build per run; the heavy-build queue stays untouched otherwise.
  const log = join(root, '.scratch/dev', `boot-suite-composition.build.${process.pid}.log`);
  console.log(`BOOT_SUITE_BUILD_START:cargo build -p erpx-platform-composition --features private-development-api --bin erpx-server (log ${log})`);
  const child = spawn('cargo', ['build', '-p', 'erpx-platform-composition', '--features', 'private-development-api', '--bin', 'erpx-server'], {
    cwd: root,
    env: process.env,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let output = '';
  child.stdout?.on('data', chunk => { output += String(chunk); });
  child.stderr?.on('data', chunk => { output += String(chunk); });
  const exit = await new Promise<number>((resolveExit, rejectExit) => {
    child.once('error', rejectExit);
    child.once('exit', code => resolveExit(code ?? 1));
  });
  writeFileSync(log, output.slice(-1_000_000), { mode: 0o600 });
  if (exit !== 0) fail('BOOT_SUITE_BUILD_FAILED', `exit ${exit}; see ${log}`);
  console.log('BOOT_SUITE_BUILD_DONE:target/debug/erpx-server');
}

function startInstance(configurationPath: string, logPath: string): ChildProcess {
  const child = spawn(BINARY, ['private-api', '--development-config', configurationPath], {
    cwd: root,
    env: { ...process.env, ERPX_PROCESS_ROLE: 'app' },
    stdio: ['ignore', 'ignore', 'pipe'],
  });
  const append = (chunk: Buffer | string) => {
    try {
      writeFileSync(logPath, readFileSync(logPath, 'utf8').slice(-1_000_000) + String(chunk), { mode: 0o600 });
    } catch {
      // Best-effort log capture only.
    }
  };
  child.stderr?.on('data', append);
  return child;
}

async function waitForHealth(port: number, child: ChildProcess, logPath: string): Promise<void> {
  const baseUrl = `http://127.0.0.1:${port}`;
  const deadline = Date.now() + HEALTH_TIMEOUT_MS;
  for (;;) {
    if (child.exitCode !== null) {
      const tail = existsSync(logPath) ? readFileSync(logPath, 'utf8').slice(-2000) : '';
      fail('BOOT_SUITE_SERVER_EXITED', tail.replace(/\n/g, ' | ').slice(0, 800));
    }
    try {
      const response = await fetch(`${baseUrl}/api/health`, {
        headers: { host: `127.0.0.1:${port}` },
        redirect: 'manual',
        signal: AbortSignal.timeout(5000),
      });
      if (response.status === 200) {
        console.log(`BOOT_SUITE_HEALTH_200:${baseUrl}/api/health`);
        await response.body?.cancel().catch(() => undefined);
        return;
      }
      await response.body?.cancel().catch(() => undefined);
    } catch {
      // The listener is still starting.
    }
    if (Date.now() >= deadline) {
      const tail = existsSync(logPath) ? readFileSync(logPath, 'utf8').slice(-2000) : '';
      fail('BOOT_SUITE_SERVER_UNREADY', tail.replace(/\n/g, ' | ').slice(0, 800));
    }
    await new Promise(resolveWait => setTimeout(resolveWait, 100));
  }
}

async function postProbe(port: number, path: string, key: string): Promise<{ status: number; code: string; body: string }> {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    method: 'POST',
    headers: {
      host: 'acme.jesadakorn.com',
      origin: 'https://acme.jesadakorn.com',
      'content-type': 'application/json',
      accept: 'application/json',
      'idempotency-key': key,
    },
    body: JSON.stringify({
      companyId: PROBE_COMPANY,
      userId: 'probe-user',
      recordType: 'inventory_transit',
      recordId: 'transit-1',
      recordVersion: 1,
      message: 'boot probe',
      now: Date.now(),
      idempotencyKey: key,
    }),
    redirect: 'manual',
    signal: AbortSignal.timeout(15000),
  });
  const text = await response.text();
  let code = '';
  try {
    code = (JSON.parse(text) as { error?: { code?: string } }).error?.code ?? '';
  } catch {
    code = '';
  }
  return { status: response.status, code, body: text.slice(0, 400) };
}

function printEnvMatrix(port: number): void {
  const baseUrl = `http://127.0.0.1:${port}`;
  const acme = { origin: 'https://acme.jesadakorn.com', holdingSlug: 'acme', holdingId: 'h001' };
  const company = { ...acme, companyId: PROBE_COMPANY };
  const matrix: Record<string, unknown> = {
    ERPX_NATIVE_INVENTORY_CONFIG: { privateBaseUrl: baseUrl, timeoutMs: 10000, holdings: [company] },
    ERPX_NATIVE_INVENTORY_COMMAND_CONFIG: { privateBaseUrl: baseUrl, timeoutMs: 10000, holdings: [acme] },
    ERPX_NATIVE_INVENTORY_COUNT_CONFIG: { privateBaseUrl: baseUrl, timeoutMs: 10000, holdings: [acme] },
    ERPX_NATIVE_INVENTORY_STOCK_CONFIG: { privateBaseUrl: baseUrl, timeoutMs: 10000, holdings: [acme] },
    ERPX_NATIVE_INVENTORY_AVAILABILITY_CONFIG: { privateBaseUrl: baseUrl, timeoutMs: 10000, holdings: [acme] },
    ERPX_NATIVE_COLLABORATION_CONFIG: { privateBaseUrl: baseUrl, timeoutMs: 10000 },
    ERPX_NATIVE_COMMERCE_WAVE2_CONFIG: { privateBaseUrl: baseUrl, host: 'acme.jesadakorn.com', holdingSlug: 'acme', origin: 'https://acme.jesadakorn.com', timeoutMs: 10000 },
    ERPX_NATIVE_COMMERCIAL_WAVE2_CONFIG: { privateBaseUrl: baseUrl, host: 'acme.jesadakorn.com', holdingSlug: 'acme', origin: 'https://acme.jesadakorn.com', timeoutMs: 10000 },
    ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG: { privateBaseUrl: baseUrl, host: 'acme.jesadakorn.com', holdingSlug: 'acme', origin: 'https://acme.jesadakorn.com', timeoutMs: 10000 },
    ERPX_NATIVE_FINANCE_OWNER_WRITE_CONFIG: { privateBaseUrl: baseUrl, timeoutMs: 10000, holdings: [{ ...acme }] },
    ERPX_NATIVE_WORKFLOW_WAVE3_CONFIG: { privateBaseUrl: baseUrl, timeoutMs: 10000 },
  };
  console.log('BOOT_SUITE_ENV_MATRIX_BEGIN:test.ts injects these shell exports for the lane');
  for (const [name, value] of Object.entries(matrix)) {
    console.log(`export ${name}='${JSON.stringify(value)}'`);
  }
  console.log('BOOT_SUITE_ENV_MATRIX_END');
}

async function stopInstance(child: ChildProcess): Promise<void> {
  if (child.exitCode !== null) return;
  child.kill('SIGTERM');
  await new Promise<void>(resolveStop => {
    if (child.exitCode !== null) return resolveStop();
    const timer = setTimeout(() => {
      child.kill('SIGKILL');
      resolveStop();
    }, 5000);
    child.once('exit', () => {
      clearTimeout(timer);
      resolveStop();
    });
  });
  console.log(`BOOT_SUITE_INSTANCE_STOPPED:pid=${child.pid}:exit=${child.exitCode}`);
}

function processGone(pid: number | undefined): boolean {
  if (pid === undefined) return true;
  try {
    process.kill(pid, 0);
    return false;
  } catch {
    return true;
  }
}

async function verifyInstanceTeardown(port: number, pid: number | undefined): Promise<void> {
  if (!processGone(pid)) fail('BOOT_SUITE_ORPHAN_PROCESS', String(pid));
  console.log(`BOOT_SUITE_NO_ORPHAN:pid=${pid}`);
  try {
    const probe = createServer();
    await new Promise<void>((resolveProbe, rejectProbe) => {
      probe.once('error', rejectProbe);
      probe.listen(port, '127.0.0.1', () => resolveProbe());
    });
    await new Promise<void>(resolveClose => probe.close(() => resolveClose()));
    console.log(`BOOT_SUITE_PORT_FREE_AFTER:${port}`);
  } catch {
    fail('BOOT_SUITE_PORT_STILL_BOUND', String(port));
  }
}

async function verifyDbsGone(admin: string, names: string[]): Promise<void> {
  const remaining = await query(admin, 'SELECT datname FROM pg_database WHERE datname = ANY($1)', [names]);
  if (remaining.rows.length) fail('BOOT_SUITE_DB_REMAINS', remaining.rows.map(row => String(row.datname)).join(','));
  console.log(`BOOT_SUITE_DBS_GONE:${names.join(',')}`);
}

async function run(): Promise<void> {
  const { suite, keep, serve } = parseArguments(process.argv.slice(2));
  console.log(`BOOT_SUITE_START:${suite}:keep=${keep ? '1' : '0'}`);
  const admin = adminUrl();
  const { password: _password } = assertAdminEndpoint(admin);
  void _password;
  const control = newLease(admin);
  const holdings: Record<string, Lease> = { h001: newLease(admin), h002: newLease(admin) };
  const all = [control, holdings.h001, holdings.h002];
  const adminClient = new Client({ connectionString: admin });
  await adminClient.connect();
  let child: ChildProcess | undefined;
  let configurationPath = '';
  let succeeded = false;
  try {
    const role = await adminClient.query(
      'SELECT current_user::text AS role,rolsuper,rolcreatedb,rolcreaterole FROM pg_roles WHERE rolname=current_user',
    );
    if (role.rows[0].role !== 'erpx_test_admin' || role.rows[0].rolsuper || !role.rows[0].rolcreatedb || role.rows[0].rolcreaterole) {
      fail('BOOT_SUITE_ADMIN_ROLE_UNEXPECTED');
    }
    for (const lease of all) await createLeasedDatabase(adminClient, lease);
    await migrateDatabase(control.url, 'control');
    await migrateDatabase(holdings.h001.url, 'holding-write');
    await migrateDatabase(holdings.h002.url, 'holding-write');
    await seedControlBindings(control.url, [
      { id: 'h001', write: 'H001_DB', read: 'H001_READ_DB', files: 'H001_FILES' },
      { id: 'h002', write: 'H002_DB', read: 'H002_READ_DB', files: 'H002_FILES' },
    ]);
    // Minimal probe row, mirroring the organization_nodes seed pattern in the
    // rust-supply-package-obligations native test.
    await query(
      holdings.h001.url,
      "INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES('probe-co','company','probe-co','Probe','probe-co') ON CONFLICT DO NOTHING",
    );
    console.log('BOOT_SUITE_PROBE_ROW:organization_nodes:probe-co');
    const port = await pickPort();
    const scratch = join(root, '.scratch/dev');
    mkdirSync(scratch, { recursive: true, mode: 0o700 });
    configurationPath = join(scratch, `boot-suite-${suite}.${process.pid}.json`);
    writeFileSync(configurationPath, JSON.stringify(ephemeralConfiguration(port, control, holdings), null, 2) + '\n', { mode: 0o600 });
    chmodSync(configurationPath, 0o600);
    console.log(`BOOT_SUITE_CONFIG:${configurationPath}`);
    await ensureBinary();
    const logPath = join(scratch, `boot-suite-${suite}.${process.pid}.log`);
    child = startInstance(configurationPath, logPath);
    console.log(`BOOT_SUITE_INSTANCE_STARTED:pid=${child.pid}`);
    await waitForHealth(port, child, logPath);
    const key = randomBytes(16).toString('hex');
    const resolved = await postProbe(port, '/api/v1/holdings/acme/collaboration/notification-statement', key);
    console.log(`BOOT_SUITE_PROBE_ACME:status=${resolved.status}:code=${resolved.code}`);
    if (resolved.code !== 'UNAUTHENTICATED') fail('BOOT_SUITE_PROBE_ACME_UNEXPECTED', `${resolved.status}:${resolved.code}`);
    const missing = await postProbe(port, '/api/v1/holdings/nope/collaboration/notification-statement', randomBytes(16).toString('hex'));
    console.log(`BOOT_SUITE_PROBE_NOPE:status=${missing.status}:code=${missing.code}`);
    if (missing.status !== 404 || missing.code !== 'NOT_FOUND') fail('BOOT_SUITE_PROBE_NOPE_UNEXPECTED', `${missing.status}:${missing.code}`);
    printEnvMatrix(port);
    if (serve) {
      console.log(`BOOT_SUITE_SERVE_READY:port=${port}`);
      await new Promise<void>(resolveServe => {
        process.stdin.resume();
        process.stdin.on('end', () => resolveServe());
        process.stdin.on('close', () => resolveServe());
      });
    }
    if (keep) {
      succeeded = true;
      console.log(`BOOT_SUITE_KEEP:instance=http://127.0.0.1:${port}:pid=${child?.pid}:config=${configurationPath}:log=${logPath}`);
      console.log(`BOOT_SUITE_KEEP_CLEANUP:kill ${child?.pid} && bun scripts/testing/boot-suite-composition.ts --suite=${suite}`);
      return;
    }
    const pid = child.pid;
    await stopInstance(child);
    child = undefined;
    await verifyInstanceTeardown(port, pid);
    succeeded = true;
  } finally {
    if (!(keep && succeeded)) {
      if (child && child.exitCode === null) await stopInstance(child);
      child = undefined;
      for (const lease of all) {
        try {
          await dropLeasedDatabase(adminClient, lease);
        } catch (error) {
          console.error(`BOOT_SUITE_DROP_ERROR:${lease.name}:${error instanceof Error ? error.message : error}`);
          throw error;
        }
      }
      await verifyDbsGone(admin, all.map(lease => lease.name));
      if (configurationPath && existsSync(configurationPath)) unlinkSync(configurationPath);
    }
    await adminClient.end();
  }
  console.log(`BOOT_SUITE_DONE:${suite}`);
}

if (import.meta.main) {
  try {
    await run();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
