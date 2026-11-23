import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import type { SqlResult, SqlStatement } from '@erpx/server-runtime/public/runtime/types';

/** Thin fail-closed transport to the single native Commerce owner axum router.
 * `values` is a positional data array, never SQL text. */
type NativeConfig = Readonly<{ base: string; host: string; holdingSlug: string; origin: string; timeoutMs: number }>;
function config(): NativeConfig {
  const raw = process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG;
  if (raw === undefined) throw new NativeCommerceOwnerWriteError('COMMERCE_OWNER_WRITE_UNAVAILABLE', 503, 'The native Commerce service is not configured.');
  let value: { privateBaseUrl?: unknown; host?: unknown; holdingSlug?: unknown; origin?: unknown; timeoutMs?: unknown };
  try { value = JSON.parse(raw) as typeof value; } catch { throw new NativeCommerceOwnerWriteError('COMMERCE_OWNER_WRITE_CONFIG_INVALID', 503, 'The native Commerce transport is misconfigured.'); }
  let base: URL, origin: URL;
  try { base = new URL(String(value.privateBaseUrl)); origin = new URL(String(value.origin)); }
  catch { throw new NativeCommerceOwnerWriteError('COMMERCE_OWNER_WRITE_CONFIG_INVALID', 503, 'The native Commerce transport is misconfigured.'); }
  if (base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || origin.protocol !== 'https:' ||
      typeof value.host !== 'string' || value.host !== origin.host ||
      typeof value.holdingSlug !== 'string' || !/^[a-z0-9_-]{1,128}$/.test(value.holdingSlug) ||
      !Number.isInteger(value.timeoutMs) || (value.timeoutMs as number) < 250 || (value.timeoutMs as number) > 30000)
    throw new NativeCommerceOwnerWriteError('COMMERCE_OWNER_WRITE_CONFIG_INVALID', 503, 'The native Commerce transport is misconfigured.');
  return { base: base.origin, host: value.host, holdingSlug: value.holdingSlug, origin: origin.origin, timeoutMs: value.timeoutMs as number };
}
export class NativeCommerceOwnerWriteError extends Error {
  readonly code: string; readonly status: number;
  constructor(code: string, status: number, message: string) { super(message); this.name = 'NativeCommerceOwnerWriteError'; this.code = code; this.status = status; }
}
const isRecord = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v);
export type CommerceOwnerWriteKind =
  | 'settings.initialize' | 'billing-test.document' | 'billing-test.entry' | 'billing-test.line'
  | 'supplier.fulfillment-version' | 'supplier.order-version' | 'supplier.fulfillment-links'
  | 'mfg-existing-posting-confirmation' | 'goods-carrying' | 'order.posted-revision'
  | 'product.create' | 'product.update';

/** Post with an explicit Host header; fetch forbids overriding Host. */
function post(
  url: URL,
  headers: Record<string, string>,
  body: string,
  timeoutMs: number,
): Promise<{ status: number; value: unknown }> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import('node:http').ProxyEnv,
    });
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
          const text = Buffer.concat(chunks).toString('utf8');
          let value: unknown = null;
          try { value = text ? JSON.parse(text) : null; } catch { value = null; }
          resolve({ status: incoming.statusCode ?? 502, value });
        });
        incoming.on('error', error => { agent.destroy(); reject(error); });
      },
    );
    outgoing.on('error', error => { agent.destroy(); reject(error); });
    outgoing.end(body);
  });
}

export async function nativeCommerceOwnerWrite(
  kind: CommerceOwnerWriteKind, values: readonly unknown[], recordId: string,
  actorId: string, idempotencyKey: string, companyId?: string,
): Promise<{ id: string; kind: string }> {
  const cfg = config();
  const body = JSON.stringify({ kind, values, recordId, companyId: companyId ?? recordId });
  let response: { status: number; value: unknown };
  try {
    response = await post(
      new URL(`/api/v1/holdings/${encodeURIComponent(cfg.holdingSlug)}/commerce/owner-write`, cfg.base),
      {
        host: cfg.host, origin: cfg.origin, accept: 'application/json', 'content-type': 'application/json',
        'idempotency-key': idempotencyKey, 'x-erpx-context-user': actorId,
      },
      body,
      cfg.timeoutMs,
    );
  } catch { throw new NativeCommerceOwnerWriteError('COMMERCE_OWNER_WRITE_UNAVAILABLE', 503, 'The native Commerce service is unavailable.'); }
  const value: unknown = response.value;
  if (response.status !== 200) {
    const error = isRecord(value) && isRecord(value.error) ? value.error : {};
    throw new NativeCommerceOwnerWriteError(typeof error.code === 'string' ? error.code : 'COMMERCE_OWNER_WRITE_FAILED', response.status, typeof error.message === 'string' ? error.message : 'The Commerce write could not be completed.');
  }
  if (!isRecord(value) || !isRecord(value.data) || typeof value.data.id !== 'string' || typeof value.data.kind !== 'string')
    throw new NativeCommerceOwnerWriteError('COMMERCE_OWNER_WRITE_RESPONSE_INVALID', 503, 'The native Commerce acknowledgement was invalid.');
  return { id: value.data.id, kind: value.data.kind };
}


/**
 * A caller-transaction-compatible statement whose `run()` performs the write
 * through the native axum command route. It carries NO SQL text (no JSON SQL
 * mirror and no TS fallback): the owner crate owns every write statement.
 * `database.batch([...])` calls run(), so staged cross-owner writes keep their
 * synchronous statement contract while the actual write executes in Rust.
 */
export function nativeOwnerStatement(
  kind: CommerceOwnerWriteKind,
  values: readonly unknown[],
  recordId: string,
  actorId: string,
  idempotencyKey: string,
  companyId?: string,
): SqlStatement {
  const statement: SqlStatement = {
    bind() { return statement; },
    async run<T>(): Promise<SqlResult<T>> {
      await nativeCommerceOwnerWrite(kind, values, recordId, actorId, idempotencyKey, companyId);
      return { results: [], success: true, meta: { changes: 1, duration: 0 } };
    },
    async first<T>(): Promise<T | null> { throw new NativeCommerceOwnerWriteError('COMMERCE_OWNER_WRITE_READ_UNSUPPORTED', 503, 'This native statement is write-only.'); },
    async all<T>(): Promise<SqlResult<T>> { throw new NativeCommerceOwnerWriteError('COMMERCE_OWNER_WRITE_READ_UNSUPPORTED', 503, 'This native statement is write-only.'); },
    async raw<T>(): Promise<T[]> { throw new NativeCommerceOwnerWriteError('COMMERCE_OWNER_WRITE_READ_UNSUPPORTED', 503, 'This native statement is write-only.'); },
  };
  return statement;
}
