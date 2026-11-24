import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { Readable } from 'node:stream';
import type { SqlStatement } from '@erpx/server-runtime/public/runtime/types';

/** The single one-row probe base shared by inert owner-write placeholders and
 * owner-local verification probes. Kept here once so owner code never repeats
 * the same fixed statement; each caller supplies its own suffix. */
export function selectOneSql(suffix = '') { return `SELECT 1${suffix}`; }

/** The single inert placeholder statement SQL returned to existing caller
 * batches after a native write. Kept here once so owner writers never repeat
 * the same fixed statement in every command step. */
export const NATIVE_OWNER_WRITE_NOOP_SQL = selectOneSql();

/**
 * The single write transport for all remaining Platform Administration owner writes.
 * It sends a typed allow-listed `kind` and positional values to the native
 * axum command router, which EXECUTES the owner SQL and the receipt/audit/
 * outbox ledger in one Rust transaction. No SQL, no JSON SQL mirror and no
 * fallback exist here: a missing transport or invalid reply fails closed.
 */
export type OwnerWriteConfig = Readonly<{
  privateBaseUrl: string;
  holdingSlug: string;
  holdingId: string;
  origin: string;
  timeoutMs: number;
}>;

export type OwnerWriteOptions = Readonly<{
  /** Control-side route (default: caller-bound Holding route). */
  control?: boolean;
  companyId?: string;
  now?: number;
}>;

export class NativeOwnerWriteError extends Error {
  readonly status?: number;
  constructor(code: string, status?: number) {
    super('The native Platform Administration owner write could not be completed.');
    this.name = 'NativeOwnerWriteError';
    this.status = status;
  }
}

const fail = (status?: number): never => {
  throw new NativeOwnerWriteError('NATIVE_PLATFORM_OWNER_WRITE_UNAVAILABLE', status);
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: string | undefined): OwnerWriteConfig {
  if (raw === undefined) return fail();
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail();
  }
  if (!record(value) || typeof value.privateBaseUrl !== 'string') return fail();
  const timeoutMs = value.timeoutMs;
  if (typeof timeoutMs !== 'number' || !Number.isSafeInteger(timeoutMs) || timeoutMs < 250 || timeoutMs > 30000)
    return fail();
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return fail();
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    reservedPorts.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username || base.password || value.privateBaseUrl !== base.origin
  )
    return fail();
  if (
    typeof value.holdingSlug !== 'string' || !/^[a-z0-9_-]+$/.test(value.holdingSlug) ||
    typeof value.holdingId !== 'string' || value.holdingId.length === 0 ||
    typeof value.origin !== 'string' || value.origin !== new URL(value.origin).origin ||
    !value.origin.startsWith('https://')
  )
    return fail();
  return {
    privateBaseUrl: value.privateBaseUrl,
    holdingSlug: value.holdingSlug,
    holdingId: value.holdingId,
    origin: value.origin,
    timeoutMs,
  };
}

function directHttp(
  url: URL,
  headers: Record<string, string>,
  body: string,
  signal: AbortSignal,
): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({ keepAlive: false, maxSockets: 1, maxTotalSockets: 1, proxyEnv: {} as import('node:http').ProxyEnv });
    const outgoing = httpRequest(
      url,
      { method: 'POST', agent, headers: { ...headers, 'content-length': String(new TextEncoder().encode(body).length) }, signal, maxHeaderSize: 8192 },
      incoming => {
        incoming.once('close', () => agent.destroy());
        try {
          const status = incoming.statusCode ?? 502;
          if (status !== 200) {
            incoming.destroy();
            resolve(new Response(null, { status: status >= 200 && status <= 599 ? status : 502 }));
            return;
          }
          const values = new Headers();
          for (let i = 0; i < incoming.rawHeaders.length; i += 2) values.append(incoming.rawHeaders[i]!, incoming.rawHeaders[i + 1]!);
          resolve(new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, { status, headers: values }));
        } catch (error) {
          incoming.destroy();
          agent.destroy();
          reject(error);
        }
      },
    );
    outgoing.once('error', error => { agent.destroy(); reject(error); });
    outgoing.write(body);
    outgoing.end();
  });
}

export function nativeOwnerWriteAvailable(): boolean {
  return process.env.ERPX_NATIVE_PLATFORM_OWNER_WRITE_CONFIG !== undefined;
}

/** Execute one owner write in Rust. `key` is the Idempotency-Key; `now` is the
 * caller's committed timestamp. Returns a no-op statement so existing caller
 * batches keep their shape without carrying any write SQL. */
export async function nativeOwnerWrite(
  kind: string,
  values: readonly unknown[],
  key: string,
  options: OwnerWriteOptions,
  noop: () => SqlStatement,
): Promise<SqlStatement> {
  const config = configuration(process.env.ERPX_NATIVE_PLATFORM_OWNER_WRITE_CONFIG);
  if (
    !/^[a-z0-9_]{1,120}$/.test(kind) || values.length < 1 || values.length > 64 ||
    !/^[A-Za-z0-9:_.-]{8,180}$/.test(key) ||
    (options.companyId !== undefined && options.companyId.length > 200) ||
    (options.now !== undefined && !Number.isSafeInteger(options.now))
  )
    return fail();
  const suffix = options.control ? 'owner-write-control' : 'owner-write';
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(config.holdingSlug)}/platform/${suffix}`,
    config.privateBaseUrl,
  );
  const body = JSON.stringify({
    kind,
    values,
    companyId: options.companyId ?? '',
    now: options.now ?? Date.now(),
  });
  if (new TextEncoder().encode(body).length > 128 * 1024) return fail();
  const headers: Record<string, string> = {
    host: new URL(config.origin).host,
    origin: config.origin,
    'idempotency-key': key,
    accept: 'application/json',
    'content-type': 'application/json',
    'cache-control': 'no-cache',
  };
  if (options.companyId) headers['x-erpx-context-company'] = options.companyId;
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  try {
    const response = await directHttp(url, headers, body, controller.signal);
    if (response.redirected || response.status !== 200 || !response.body) return fail(response.status);
    const text = await response.text();
    if (text.length > 16 * 1024) return fail();
    const payload = JSON.parse(text) as { data?: unknown };
    if (!record(payload) || !record(payload.data) || payload.data.ok !== true || payload.data.kind !== kind)
      return fail();
    return noop();
  } catch (error) {
    if (error instanceof NativeOwnerWriteError) throw error;
    return fail();
  } finally {
    clearTimeout(timer);
  }
}
