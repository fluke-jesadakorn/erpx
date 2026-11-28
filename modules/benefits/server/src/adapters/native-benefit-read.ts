import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';

/** Native owner read plans for Benefits attachments. The mounted
 * `benefits/command-plan` route resolves the allow-listed kind; this bridge
 * emits no SQL, accepts no SQL from a caller and executes the returned
 * statement on the caller's already-open unit. */
export const NATIVE_BENEFITS_OWNER_READ_CONFIG = 'ERPX_NATIVE_BENEFITS_OWNER_READ_CONFIG';
export const RD_ATTACHMENT_BENEFIT_DEPENDENTS = 'benefits_rd_attachment_benefit_dependents';
export const RD_ATTACHMENT_BENEFIT_ELECTIONS = 'benefits_rd_attachment_benefit_elections';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

export type BenefitOwnerReadConfig = Readonly<{
  base: string;
  host: string;
  holdingSlug: string;
  origin: string;
  timeoutMs: number;
}>;

type Transport = (
  url: URL,
  headers: Record<string, string>,
  body: string,
  timeoutMs: number,
) => Promise<{ status: number; value: unknown }>;

const unavailable = (code: string, message: string): never => {
  throw new ApiError(503, code, message);
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max: number): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;

export function benefitOwnerReadConfiguration(raw: string | undefined): BenefitOwnerReadConfig {
  if (!raw)
    return unavailable('BENEFIT_OWNER_READ_UNAVAILABLE', 'The native Benefits read service is not configured.');
  let value: Record<string, unknown>;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!record(parsed)) throw new Error('not an object');
    value = parsed;
  } catch {
    return unavailable('BENEFIT_OWNER_READ_CONFIG_INVALID', 'The native Benefits read transport is misconfigured.');
  }
  if (
    !text(value.privateBaseUrl, 512) ||
    !text(value.host, 512) ||
    !text(value.holdingSlug, 128) ||
    !/^[a-z0-9_-]{1,128}$/.test(value.holdingSlug) ||
    !text(value.origin, 512) ||
    typeof value.timeoutMs !== 'number' ||
    !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000
  )
    return unavailable('BENEFIT_OWNER_READ_CONFIG_INVALID', 'The native Benefits read transport is misconfigured.');
  let base: URL;
  let origin: URL;
  try {
    base = new URL(value.privateBaseUrl);
    origin = new URL(value.origin);
  } catch {
    return unavailable('BENEFIT_OWNER_READ_CONFIG_INVALID', 'The native Benefits read transport is misconfigured.');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    base.origin !== value.privateBaseUrl ||
    origin.protocol !== 'https:' ||
    origin.origin !== value.origin ||
    origin.host !== value.host ||
    origin.username ||
    origin.password
  )
    return unavailable('BENEFIT_OWNER_READ_CONFIG_INVALID', 'The native Benefits read transport is misconfigured.');
  return {
    base: base.origin,
    host: origin.host,
    holdingSlug: value.holdingSlug,
    origin: origin.origin,
    timeoutMs: value.timeoutMs,
  };
}

function httpPost(
  url: URL,
  headers: Record<string, string>,
  body: string,
  timeoutMs: number,
): Promise<{ status: number; value: unknown }> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({ keepAlive: false, maxSockets: 1, maxTotalSockets: 1, proxyEnv: {} as import('node:http').ProxyEnv });
    const outgoing = httpRequest(
      url,
      {
        method: 'POST',
        agent,
        headers: { ...headers, 'content-length': String(Buffer.byteLength(body)) },
        signal: AbortSignal.timeout(timeoutMs),
        maxHeaderSize: 8192,
      },
      incoming => {
        const chunks: Buffer[] = [];
        incoming.on('data', chunk => chunks.push(Buffer.from(chunk)));
        incoming.on('end', () => {
          agent.destroy();
          const payload = Buffer.concat(chunks).toString('utf8');
          let value: unknown = null;
          try { value = payload ? JSON.parse(payload) : null; } catch { value = null; }
          resolve({ status: incoming.statusCode ?? 502, value });
        });
        incoming.on('error', error => { agent.destroy(); reject(error); });
      },
    );
    outgoing.on('error', error => { agent.destroy(); reject(error); });
    outgoing.end(body);
  });
}

type Planned = Readonly<{ sql: string; values: unknown[] }>;

/** Only the typed kind and positional values cross the wire. The reply must
 * carry exactly one owner statement whose bind count matches the payload. */
export function readBenefitReadReply(value: unknown, expected: number): Planned {
  if (!record(value) || !record(value.data) || !Array.isArray(value.data.statements) || value.data.statements.length !== 1)
    return unavailable('BENEFIT_OWNER_READ_RESPONSE_INVALID', 'The native Benefits read reply was invalid.');
  const entry = value.data.statements[0];
  if (
    !record(entry) ||
    typeof entry.sql !== 'string' ||
    entry.sql.length > 64_000 ||
    !Array.isArray(entry.values) ||
    entry.values.length !== expected ||
    !entry.values.every(item => item === null || typeof item === 'string' || Number.isSafeInteger(item))
  )
    return unavailable('BENEFIT_OWNER_READ_RESPONSE_INVALID', 'The native Benefits read reply was invalid.');
  return { sql: entry.sql, values: entry.values };
}

export function createBenefitOwnerReadClient(cfg: BenefitOwnerReadConfig, transport: Transport) {
  return async function plan(kind: string, values: readonly unknown[]): Promise<Planned> {
    if (!/^[a-z0-9_]{1,120}$/.test(kind) || values.length > 64)
      return unavailable('BENEFIT_OWNER_READ_INVALID', 'Use one bounded Benefits owner read.');
    let response: { status: number; value: unknown };
    try {
      response = await transport(
        new URL(`/api/v1/holdings/${encodeURIComponent(cfg.holdingSlug)}/benefits/command-plan`, cfg.base),
        {
          host: cfg.host,
          origin: cfg.origin,
          accept: 'application/json',
          'content-type': 'application/json',
          'cache-control': 'no-cache',
        },
        JSON.stringify({ kind, values }),
        cfg.timeoutMs,
      );
    } catch {
      return unavailable('BENEFIT_OWNER_READ_UNAVAILABLE', 'The native Benefits read service is unavailable.');
    }
    if (response.status !== 200)
      return unavailable('BENEFIT_OWNER_READ_UNAVAILABLE', 'The native Benefits read service is unavailable.');
    return readBenefitReadReply(response.value, values.length);
  };
}

/** Planned owner read returning a single row (retired `.first()` parity). */
export async function nativeBenefitReadFirst<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
): Promise<T | null> {
  const cfg = benefitOwnerReadConfiguration(process.env[NATIVE_BENEFITS_OWNER_READ_CONFIG]);
  const statement = await createBenefitOwnerReadClient(cfg, httpPost)(kind, values);
  return db.prepare(statement.sql).bind(...statement.values).first<T>();
}
