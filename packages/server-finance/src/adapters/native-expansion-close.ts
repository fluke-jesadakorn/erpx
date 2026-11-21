import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

/**
 * Strict server-side client for the native Finance expansion-close pending
 * source-task read. This module carries no SQL and offers no TypeScript
 * fallback: the authoritative `EXPANSION_CLOSE_SQL` lives in the Rust owner.
 * A missing transport or a malformed reply fails closed. The `db` argument is
 * retained only for signature compatibility with the retired TS bridge.
 */
export type ExpansionClosePendingRow = { kind: string; id: string };

type Settings = Readonly<{ privateBaseUrl: string; timeoutMs: number }>;

const CONFIG_KEY = 'ERPX_NATIVE_FINANCE_EXPANSION_CLOSE_CONFIG';
const BODY_LIMIT = 2 * 1024 * 1024;
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

const fail = (status = 503): never => {
  throw new ApiError(
    status,
    'NATIVE_FINANCE_EXPANSION_CLOSE_UNAVAILABLE',
    'The native Finance expansion-close read is unavailable.',
  );
};

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

function config(): Settings {
  const raw = process.env[CONFIG_KEY];
  if (raw === undefined) return fail();
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail();
  }
  if (!record(value) || typeof value.privateBaseUrl !== 'string') return fail();
  const timeoutMs = value.timeoutMs;
  if (
    typeof timeoutMs !== 'number' ||
    !Number.isSafeInteger(timeoutMs) ||
    timeoutMs < 250 ||
    timeoutMs > 30000
  )
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
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    value.privateBaseUrl !== base.origin
  )
    return fail();
  return { privateBaseUrl: value.privateBaseUrl, timeoutMs };
}

const validIdentifier = (value: string) =>
  value.length > 0 &&
  value.length <= 200 &&
  !/[\u0000-\u001f/?#\\]/.test(value);

const validBoundary = (value: string) =>
  /^\d{4}-\d{2}-\d{2}$/.test(value);

const validRow = (value: unknown): value is ExpansionClosePendingRow =>
  record(value) &&
  typeof value.kind === 'string' &&
  value.kind.length > 0 &&
  value.kind.length <= 200 &&
  typeof value.id === 'string' &&
  value.id.length > 0 &&
  value.id.length <= 200;

async function readBody(body: ReadableStream<Uint8Array>): Promise<string> {
  const reader = body.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.byteLength;
      if (bytes > BODY_LIMIT) {
        await reader.cancel();
        return fail(502);
      }
      chunks.push(next.value);
    }
  } finally {
    reader.releaseLock();
  }
  const data = new Uint8Array(bytes);
  let offset = 0;
  for (const chunk of chunks) {
    data.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return new TextDecoder('utf-8', { fatal: true }).decode(data);
}

/** Ask the native Finance owner for the authoritative pending source-task list. */
export async function financeExpansionClosePending(
  _db: SqlDatabase,
  companyId: string,
  start: string,
  end: string,
): Promise<ExpansionClosePendingRow[]> {
  const settings = config();
  if (
    !validIdentifier(companyId) ||
    !validBoundary(start) ||
    !validBoundary(end) ||
    start > end
  )
    return fail(422);
  const url = new URL(
    `/api/v1/companies/${encodeURIComponent(companyId)}/finance/expansion-close/pending` +
      `?start=${encodeURIComponent(start)}&end=${encodeURIComponent(end)}`,
    settings.privateBaseUrl,
  );
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'GET',
      headers: {
        accept: 'application/json',
        'x-erpx-context-company': companyId,
      },
      cache: 'no-store',
      redirect: 'error',
      signal: AbortSignal.timeout(settings.timeoutMs),
    });
  } catch {
    return fail(503);
  }
  if (response.redirected || response.status !== 200 || !response.body) {
    void response.body?.cancel().catch(() => undefined);
    return fail(response.status);
  }
  if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body.cancel().catch(() => undefined);
    return fail(502);
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(await readBody(response.body));
  } catch {
    return fail(502);
  }
  if (
    !record(parsed) ||
    !Array.isArray(parsed.data) ||
    !parsed.data.every(validRow)
  )
    return fail(502);
  return parsed.data as ExpansionClosePendingRow[];
}
