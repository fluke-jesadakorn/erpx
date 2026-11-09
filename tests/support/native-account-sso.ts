import { spawn, type ChildProcess } from 'node:child_process';
import { createServer, request as httpRequest, type Server } from 'node:http';
import { chmodSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { Pool } from 'pg';

/**
 * Integration-only native Account SSO fixture.
 *
 * The shared development native Account API is bound to the leased clone
 * Control database, so it cannot authenticate users created in a suite's
 * disposable databases. This fixture starts the actual Rust `private-api
 * --test-config` against the exact `erpx_test_<10 hex>` databases and exposes a
 * loopback proxy. The production transport still performs every host, origin,
 * cookie and redirect check; only the database binding differs.
 */

const ACCOUNT_HOST = 'account-dev-erpx.jesadakorn.com';
const HOLDING_HOSTS: Record<string, string> = {
  h001: 'acme.jesadakorn.com',
  h002: 'northstar.jesadakorn.com',
  h003: 'erpx-internal.jesadakorn.com',
};
const PRODUCT_HOSTS = [
  ['admin', 'admin-dev-erpx.jesadakorn.com'],
  ['apps', 'apps-dev-erpx.jesadakorn.com'],
  ['assets', 'assets-dev-erpx.jesadakorn.com'],
  ['booking', 'booking-dev-erpx.jesadakorn.com'],
  ['console', 'console-dev-erpx.jesadakorn.com'],
  ['finance', 'finance-dev-erpx.jesadakorn.com'],
  ['forms', 'forms-dev-erpx.jesadakorn.com'],
  ['learn', 'learn-dev-erpx.jesadakorn.com'],
  ['legal-portal', 'legal-portal-dev-erpx.jesadakorn.com'],
  ['people', 'people-dev-erpx.jesadakorn.com'],
  ['pms', 'pms-dev-erpx.jesadakorn.com'],
  ['portal', 'portal-dev-erpx.jesadakorn.com'],
  ['pos', 'pos-dev-erpx.jesadakorn.com'],
  ['sales', 'sales-dev-erpx.jesadakorn.com'],
  ['shop', 'shop-dev-erpx.jesadakorn.com'],
  ['supply', 'supply-dev-erpx.jesadakorn.com'],
  ['support', 'support-dev-erpx.jesadakorn.com'],
] as const;

type Manifest = {
  databases: Record<string, string>;
  slots: { id: string; write: string; read: string; files: string }[];
};
type Started = { baseUrl: string; stop: () => Promise<void> };

let starting: Promise<Started> | undefined;
let proxy: Server | undefined;

function manifest(): Manifest {
  const path = process.env.ERPX_DATABASE_MANIFEST;
  if (!path) throw new Error('NATIVE_ACCOUNT_TEST_MANIFEST_REQUIRED');
  return JSON.parse(readFileSync(path, 'utf8')) as Manifest;
}

function freePort(): Promise<number> {
  return new Promise((resolvePort, reject) => {
    const listener = createServer();
    listener.once('error', reject);
    listener.listen(0, '127.0.0.1', () => {
      const address = listener.address();
      const port = typeof address === 'object' && address ? address.port : 0;
      listener.close(error => (error ? reject(error) : resolvePort(port)));
    });
  });
}

async function waitForHealth(baseUrl: string, child: ChildProcess, stderr: () => string) {
  const deadline = Date.now() + 20_000;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) {
      throw new Error(`NATIVE_ACCOUNT_TEST_SERVER_EXITED:${stderr().slice(0, 800)}`);
    }
    try {
      const response = await fetch(`${baseUrl}/api/health`, {
        headers: { host: new URL(baseUrl).host },
        redirect: 'manual',
      });
      if (response.status === 200) return;
    } catch {
      // The listener is still starting.
    }
    await new Promise(resolveWait => setTimeout(resolveWait, 100));
  }
  throw new Error(`NATIVE_ACCOUNT_TEST_SERVER_UNREADY:${stderr().slice(0, 800)}`);
}

async function seedBindings(controlUrl: string, holdings: Manifest['slots']) {
  const pool = new Pool({ connectionString: controlUrl, max: 1 });
  const deadline = Date.now() + 20_000;
  try {
    for (;;) {
      try {
        for (const slot of holdings) {
          const id = slot.id;
          await pool.query(
            `INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status)
             VALUES($1,$2,$3,$4,$5,$6,'active')
             ON CONFLICT (holding_id) DO UPDATE SET slug=excluded.slug,display_name=excluded.display_name,
               write_binding=excluded.write_binding,read_binding=excluded.read_binding,
               files_binding=excluded.files_binding,status='active'`,
            [id, id === 'h003' ? 'erpx' : id === 'h002' ? 'northstar' : 'acme', `Native test ${id}`, slot.write, slot.read, slot.files],
          );
        }
        return;
      } catch (error) {
        if (Date.now() >= deadline) throw error;
        await new Promise(resolveWait => setTimeout(resolveWait, 100));
      }
    }
  } finally {
    await pool.end();
  }
}

async function start(): Promise<Started> {
  const current = manifest();
  const holdings = current.slots.filter(slot => HOLDING_HOSTS[slot.id]);
  if (!holdings.length) throw new Error('NATIVE_ACCOUNT_TEST_HOLDING_REQUIRED');
  const controlUrl = current.databases.CONTROL_DB;
  if (!controlUrl) throw new Error('NATIVE_ACCOUNT_TEST_CONTROL_REQUIRED');
  await seedBindings(controlUrl, holdings);
  const port = await freePort();
  const temporaryRoot = process.env.ERPX_DATA_DIR ?? process.cwd();
  const path = join(temporaryRoot, `native-account-sso-${process.pid}-${port}.json`);
  const firstHolding = holdings[0].id;
  const configuration = {
    version: 1,
    mode: 'test',
    environment: 'development',
    transport: 'direct-loopback',
    listen: `127.0.0.1:${port}`,
    accountHost: ACCOUNT_HOST,
    requireEmailVerification: false,
    passwordWorkers: 2,
    maximumRequests: 64,
    requestTimeoutMs: 10_000,
    control: { url: controlUrl },
    holdings: holdings.map(slot => ({
      holdingId: slot.id,
      slug: slot.id === 'h003' ? 'erpx' : slot.id === 'h002' ? 'northstar' : 'acme',
      host: HOLDING_HOSTS[slot.id],
      writeBinding: slot.write,
      readBinding: slot.read,
      filesBinding: slot.files,
      database: { url: current.databases[slot.write] },
      ...(current.databases[slot.read] ? { readDatabase: { url: current.databases[slot.read] } } : {}),
    })),
    productOrigins: PRODUCT_HOSTS.map(([key, host]) => ({ host, key, holdingId: firstHolding })),
    accountBootstrap: true,
  };
  writeFileSync(path, JSON.stringify(configuration, null, 2) + '\n', { mode: 0o600 });
  chmodSync(path, 0o600);
  const binary = process.env.ERPX_NATIVE_ACCOUNT_BIN ?? resolve(process.cwd(), 'target/debug/erpx-server');
  let stderr = '';
  const child = spawn(binary, ['private-api', '--test-config', path], {
    env: { ...process.env, ERPX_PROCESS_ROLE: 'app' },
    stdio: ['ignore', 'ignore', 'pipe'],
  });
  child.stderr?.on('data', chunk => {
    stderr += String(chunk);
    if (stderr.length > 16_384) stderr = stderr.slice(-16_384);
    if (process.env.ERPX_NATIVE_ACCOUNT_TEST_DEBUG === '1') console.error('NATIVE_ACCOUNT_TEST_STDERR', String(chunk).slice(0, 400));
  });
  const baseUrl = `http://127.0.0.1:${port}`;
  try {
    await waitForHealth(baseUrl, child, () => stderr);
  } catch (error) {
    child.kill('SIGTERM');
    throw error;
  }
  return {
    baseUrl,
    stop: async () => {
      child.kill('SIGTERM');
      await new Promise<void>(resolveStop => {
        if (child.exitCode !== null) return resolveStop();
        const timer = setTimeout(() => {
          child.kill('SIGKILL');
          resolveStop();
        }, 5_000);
        child.once('exit', () => {
          clearTimeout(timer);
          resolveStop();
        });
      });
    },
  };
}

async function ensureServer(): Promise<Started> {
  if (!starting) {
    starting = start().catch(error => {
      starting = undefined;
      throw error;
    });
  }
  return starting;
}

/** Native Argon2id hash for fixtures that sign in through the test API. */
export async function nativeTestPasswordHash(password: string): Promise<string> {
  const raw = process.env.ERPX_NATIVE_ACCOUNT_SESSION_CONFIG;
  if (!raw) throw new Error('NATIVE_ACCOUNT_TEST_PROXY_REQUIRED');
  const configuration = JSON.parse(raw) as { privateBaseUrl: string };
  const target = new URL('/api/v1/test/account-password-hash', configuration.privateBaseUrl);
  const result = await new Promise<{ status: number; body: string }>((resolveHash, reject) => {
    const outgoing = httpRequest(
      target,
      {
        method: 'POST',
        headers: {
          host: ACCOUNT_HOST,
          origin: `https://${ACCOUNT_HOST}`,
          'content-type': 'application/json',
        },
      },
      response => {
        let body = '';
        response.setEncoding('utf8');
        response.on('data', chunk => (body += chunk));
        response.on('end', () => resolveHash({ status: response.statusCode ?? 0, body }));
      },
    );
    outgoing.once('error', reject);
    outgoing.end(JSON.stringify({ password }));
  });
  let body: { hash?: unknown } | null = null;
  try {
    body = JSON.parse(result.body) as { hash?: unknown };
  } catch {
    body = null;
  }
  if (result.status !== 200 || typeof body?.hash !== 'string' || !body.hash.startsWith('$argon2id$v=19$')) {
    throw new Error(`NATIVE_ACCOUNT_TEST_PASSWORD_HASH_FAILED:${result.status}`);
  }
  return body.hash;
}

/** Starts the lazy proxy and publishes the exact transport configuration. */
export async function startNativeAccountSsoProxy(): Promise<void> {
  if (proxy) return;
  const port = await freePort();
  proxy = createServer((incoming, outgoing) => {
    void (async () => {
      try {
        const native = await ensureServer();
        const target = new URL(incoming.url ?? '/', native.baseUrl);
        const headers = { ...incoming.headers };
        const upstream = httpRequest(
          target,
          { method: incoming.method, headers },
          response => {
            outgoing.writeHead(response.statusCode ?? 502, response.headers);
            response.pipe(outgoing);
          },
        );
        upstream.once('error', () => {
          if (!outgoing.headersSent) outgoing.writeHead(503, { 'content-type': 'application/json' });
          outgoing.end(JSON.stringify({ error: { code: 'NATIVE_ACCOUNT_TEST_UNAVAILABLE', message: 'The native test Account API is unavailable.' } }));
        });
        incoming.pipe(upstream);
      } catch (error) {
        if (process.env.ERPX_NATIVE_ACCOUNT_TEST_DEBUG === '1') console.error('NATIVE_ACCOUNT_TEST_PROXY_ERROR', error instanceof Error ? error.message : String(error));
        if (!outgoing.headersSent) outgoing.writeHead(503, { 'content-type': 'application/json' });
        outgoing.end(JSON.stringify({ error: { code: 'NATIVE_ACCOUNT_TEST_UNAVAILABLE', message: error instanceof Error ? error.message : 'unavailable' } }));
      }
    })();
  });
  await new Promise<void>((resolveListen, reject) => {
    proxy!.once('error', reject);
    proxy!.listen(port, '127.0.0.1', () => resolveListen());
  });
  const proxyBaseUrl = `http://127.0.0.1:${port}`;
  const holdings = manifest().slots.filter(slot => HOLDING_HOSTS[slot.id]);
  const holdingOrigins = [
    ...holdings.map(slot => `https://${HOLDING_HOSTS[slot.id]}`),
    ...PRODUCT_HOSTS.map(([, host]) => `https://${host}`),
  ];
  process.env.ERPX_NATIVE_ACCOUNT_SESSION_CONFIG = JSON.stringify({
    privateBaseUrl: proxyBaseUrl,
    accountOrigin: `https://${ACCOUNT_HOST}`,
    holdingOrigins,
    timeoutMs: 10_000,
  });
}

/** Stops the proxy and the native test API. Safe to call repeatedly. */
export async function stopNativeAccountSsoProxy(): Promise<void> {
  if (proxy) {
    const current = proxy;
    proxy = undefined;
    await new Promise<void>(resolveClose => current.close(() => resolveClose()));
  }
  if (starting) {
    try {
      const current = await starting.catch(() => undefined);
      await current?.stop();
    } finally {
      starting = undefined;
    }
  }
}
