import 'server-only';
import { createHash } from 'node:crypto';
import { Agent, request as httpRequest } from 'node:http';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { SqlDatabase, SqlResult } from '@erpx/server-runtime/public/runtime/types';

/**
 * Strict owner transport for the mounted native Sales `command-plan` read
 * route. The axum owner plans the exact statement for a registered read kind;
 * this bridge emits no SQL, accepts no SQL from a caller, and executes the
 * returned statement on the caller's already-open unit so the read observes
 * uncommitted effects in the caller's serializable transaction. Read kinds
 * carry no receipt/audit/outbox ledger. Missing configuration or any configured
 * failure fails closed with no TypeScript fallback.
 */
type Holding = Readonly<{ origin: string; holdingSlug: string; holdingId: string }>;
type Settings = Readonly<{ privateBaseUrl: string; timeoutMs: number; holdings: readonly Holding[] }>;

/** Read-kind ids shared with the Rust read registry in `native_sales_read_sql.rs`. */
export const SALES_RD_PRICE_LIST_ACTIVE = 'sales_rd_price_list_active';
export const SALES_RD_PRICE_LIST_ITEMS_ACTIVE = 'sales_rd_price_list_items_active';
export const SALES_RD_NOTIFICATION_ATTEMPT = 'sales_rd_notification_attempt';
export const SALES_RD_NOTIFICATION_RECONCILIATION_INSTALLED = 'sales_rd_notification_reconciliation_installed';
export const SALES_RD_FOLLOWUP_RECIPIENTS = 'sales_rd_followup_recipients';
export const SALES_RD_PRICING_LIST_BY_ID = 'sales_rd_pricing_list_by_id';
export const SALES_RD_PRICING_LIST_ACTIVE_BY_ID = 'sales_rd_pricing_list_active_by_id';
export const SALES_RD_PRICING_LIST_ACTIVE = 'sales_rd_pricing_list_active';
export const SALES_RD_PRICING_ITEMS_BY_LIST = 'sales_rd_pricing_items_by_list';
export const SALES_RD_FOLLOWUP_SOURCE_FOLLOWUP = 'sales_rd_followup_source_followup';
export const SALES_RD_FOLLOWUP_SOURCE_ORDER = 'sales_rd_followup_source_order';
export const SALES_RD_FOLLOWUP_METRIC_PROJECTION = 'sales_rd_followup_metric_projection';
export const SALES_RD_FOLLOWUP_METRIC_SOURCE = 'sales_rd_followup_metric_source';
export const SALES_RD_NOTIFICATION_SOURCE_DEV = 'sales_rd_notification_source_dev';
export const SALES_RD_NOTIFICATION_SOURCE_PROD = 'sales_rd_notification_source_prod';
export const SALES_RD_CALENDAR_JSONB_MEMBER = 'sales_rd_calendar_jsonb_member';
export const SALES_RD_CALENDAR_OWNER_MEMBER = 'sales_rd_calendar_owner_member';
export const SALES_RD_FOLLOWUP_ORDERS = 'sales_rd_followup_orders';

/** Owner-planned statement. The caller may run it on its own open unit or
 * compose it into a larger read; no SQL is accepted from the caller. */
export type NativeSalesPlan = { sql: string; values: unknown[] };

const unavailable = (): never => {
  throw new ApiError(503, 'SALES_NATIVE_READ_UNAVAILABLE', 'The native Sales read service is unavailable.');
};
const record = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v);
const identifier = (v: unknown): v is string => typeof v === 'string' && v.length > 0 && new TextEncoder().encode(v).length <= 200;
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_SALES_DOCUMENTS_CONFIG;
  if (!raw) return unavailable();
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return unavailable(); }
  if (!record(value) || typeof value.privateBaseUrl !== 'string' || !Array.isArray(value.holdings)) return unavailable();
  if (typeof value.timeoutMs !== 'number' || !Number.isSafeInteger(value.timeoutMs) || value.timeoutMs < 250 || value.timeoutMs > 30_000) return unavailable();
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { return unavailable(); }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port ||
    reservedPorts.has(port) || (port >= 3200 && port <= 3299) || base.username || base.password ||
    value.privateBaseUrl !== base.origin
  ) return unavailable();
  const holdings = value.holdings.map((item: unknown) => {
    if (!record(item) || typeof item.origin !== 'string' || typeof item.holdingSlug !== 'string' ||
        typeof item.holdingId !== 'string' || !identifier(item.holdingId)) return unavailable();
    let origin: URL;
    try { origin = new URL(item.origin); } catch { return unavailable(); }
    if (origin.protocol !== 'https:' || item.origin !== origin.origin || origin.username || origin.password ||
        !/^[a-z0-9_-]+$/.test(item.holdingSlug)) return unavailable();
    return { origin: item.origin, holdingSlug: item.holdingSlug, holdingId: item.holdingId };
  });
  if (holdings.length < 1 || holdings.length > 64) return unavailable();
  return { privateBaseUrl: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** POST with an explicit Host header; fetch forbids overriding Host. */
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

/** Ask the mounted axum owner to plan one registered read. The returned
 * statement is owned by Rust; the caller supplies only the kind and its binds. */
async function plan(kind: string, values: readonly unknown[]): Promise<NativeSalesPlan> {
  const config = configuration();
  const holding = config.holdings[0]!;
  const key = 'sal-read-' + createHash('sha256').update(kind).update('\u0000').update(JSON.stringify(values)).digest('hex').slice(0, 32);
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/sales/command-plan`, config.privateBaseUrl);
  let response: { status: number; value: unknown };
  try {
    response = await post(url, {
      host: new URL(holding.origin).host,
      origin: holding.origin,
      accept: 'application/json',
      'content-type': 'application/json',
      'cache-control': 'no-cache',
      'idempotency-key': key,
    }, JSON.stringify({ kind, values, now: Date.now() }), config.timeoutMs);
  } catch { return unavailable(); }
  const payload: unknown = response.value;
  if (response.status !== 200) {
    const err = record(payload) && record(payload.error) ? payload.error : undefined;
    if (response.status >= 400 && response.status < 500) {
      throw new ApiError(
        response.status,
        typeof err?.code === 'string' ? err.code : 'SALES_NATIVE_READ_FAILED',
        typeof err?.message === 'string' ? err.message : 'The native Sales read was rejected.',
      );
    }
    return unavailable();
  }
  const statements = record(payload) && record(payload.data) ? payload.data.statements : undefined;
  if (!Array.isArray(statements) || statements.length !== 1) return unavailable();
  const statement = statements[0];
  if (!record(statement) || typeof statement.sql !== 'string' || !Array.isArray(statement.values)) return unavailable();
  return { sql: statement.sql, values: statement.values };
}

/** Planner for owners that compose a larger read on their own open unit. */
export async function nativeSalesPlan(
  kind: string,
  values: readonly unknown[],
): Promise<NativeSalesPlan> {
  return plan(kind, values);
}

/** Plan an owner read in Rust and run it on the caller's open unit. Pure read:
 * the kind selects the SQL; only routing-scoped values cross the wire. */
export async function nativeSalesReadAll<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
): Promise<SqlResult<T>> {
  const planned = await plan(kind, values);
  const results = await db.batch<T>([db.prepare(planned.sql).bind(...planned.values)]);
  return results[0] ?? { results: [], success: true, meta: { changes: 0, duration: 0 } };
}

/** Run a single-row owner read and return its first row (or null). */
export async function nativeSalesReadFirst<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
): Promise<T | null> {
  const result = await nativeSalesReadAll<T>(kind, values, db);
  return result.results[0] ?? null;
}
