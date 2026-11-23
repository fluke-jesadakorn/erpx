import 'server-only';
import { createHash } from 'node:crypto';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { SqlDatabase, SqlResult } from '@erpx/server-runtime/public/runtime/types';

/**
 * Strict owner transport for the mounted Customer Authority `command-plan`
 * read route on the customer-access router. The owner plans the exact fixed
 * statement for a registered read kind; this bridge emits no SQL, accepts no
 * SQL from a caller, and executes the returned statement on the caller's
 * already-open unit. Read kinds carry no receipt/audit/outbox ledger. Missing
 * configuration or any configured failure fails closed with no TS fallback.
 */
type Holding = Readonly<{ origin: string; holdingSlug: string; companyId: string }>;
type Settings = Readonly<{ privateBaseUrl: string; timeoutMs: number; holdings: readonly Holding[] }>;

export const CA_RD_SALES_REVIEW_COUNTS = 'customer_authority_rd_sales_review_counts';
export const CA_RD_IDENTITY_CHANGE_HISTORY = 'customer_authority_rd_identity_change_history';
export const CA_RD_DUPLICATE_REVIEW_HISTORY = 'customer_authority_rd_duplicate_review_history';
export const CA_RD_ACCESS_EVENTS = 'customer_authority_rd_access_events';
export const CA_RD_ACCESS_READS = 'customer_authority_rd_access_reads';
export const CA_RD_ACCESS_IDENTITIES = 'customer_authority_rd_access_identities';
export const CA_RD_DUPLICATE_REVIEWS_PENDING = 'customer_authority_rd_duplicate_reviews_pending';
export const CA_RD_IDENTITY_CHANGE_EVENTS_MONTH = 'customer_authority_rd_identity_change_events_month';
export const CA_RD_IDENTITY_CHANGE_EVENTS_ACTION = 'customer_authority_rd_identity_change_events_action';

/** An exact owner-planned statement: native SQL and its positional values. */
export type OwnerReadStatement = Readonly<{ sql: string; values: readonly unknown[] }>;

const unavailable = (): never => {
  throw new ApiError(503, 'CUSTOMER_AUTHORITY_NATIVE_READ_UNAVAILABLE', 'The native Customer Authority read service is unavailable.');
};
const record = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v);
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_CUSTOMER_AUTHORITY_READ_CONFIG;
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
        typeof item.companyId !== 'string') return unavailable();
    let origin: URL;
    try { origin = new URL(item.origin); } catch { return unavailable(); }
    if (origin.protocol !== 'https:' || origin.username || origin.password || item.origin !== origin.origin) return unavailable();
    return { origin: item.origin, holdingSlug: item.holdingSlug, companyId: item.companyId };
  });
  return { privateBaseUrl: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** Plan one owner read without executing it. The returned statement is the
 * exact native literal and its positional values; the caller prepares and
 * executes it on its own already-open unit. Consumers that also need the SQL
 * text (for example executive source drill-down) use this instead of keeping
 * a TypeScript mirror. */
export async function nativeOwnerReadStatement(
  kind: string,
  values: readonly unknown[],
  holdingSlug: string,
): Promise<OwnerReadStatement> {
  const config = configuration();
  const holding = config.holdings.find(h => h.holdingSlug === holdingSlug);
  if (!holding) return unavailable();
  const key = 'ca-read-' + createHash('sha256').update(kind).update('\u0000').update(JSON.stringify(values)).digest('hex').slice(0, 32);
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/customer-access/command-plan`, config.privateBaseUrl);
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'POST',
      headers: {
        host: new URL(holding.origin).host,
        origin: holding.origin,
        'content-type': 'application/json',
        accept: 'application/json',
        'cache-control': 'no-cache',
        'idempotency-key': key,
        'x-erpx-context-company': holding.companyId,
      },
      body: JSON.stringify({ kind, values, now: Date.now() }),
      cache: 'no-store',
      redirect: 'error',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch { return unavailable(); }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    const payload = await response.json().catch((): unknown => null);
    const err = record(payload) ? (payload as { error?: { code?: string; message?: string } }).error : undefined;
    if (response.status >= 400 && response.status < 500) {
      throw new ApiError(response.status, err?.code ?? 'CUSTOMER_AUTHORITY_NATIVE_READ_FAILED', err?.message ?? 'The native Customer Authority read was rejected.');
    }
    return unavailable();
  }
  const payload = await response.json().catch((): unknown => null);
  const statements = record(payload) ? (payload as { data?: { statements?: unknown } }).data?.statements : undefined;
  if (!Array.isArray(statements) || statements.length !== 1) return unavailable();
  const statement = statements[0];
  if (!record(statement) || typeof statement.sql !== 'string' || !Array.isArray(statement.values)) return unavailable();
  return { sql: statement.sql, values: statement.values };
}

/** Plan one owner read and execute it on the caller's open unit. */
export async function nativeOwnerReadAll<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
  holdingSlug: string,
): Promise<SqlResult<T>> {
  const statement = await nativeOwnerReadStatement(kind, values, holdingSlug);
  return db.prepare(statement.sql).bind(...statement.values).all<T>();
}
