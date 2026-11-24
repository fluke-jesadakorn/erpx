import 'server-only';
import { createHash } from 'node:crypto';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { SqlDatabase, SqlResult } from '@erpx/server-runtime/public/runtime/types';

/**
 * Strict owner transport for the native Manufacturing `read-plan` route.
 *
 * The axum owner resolves a closed RD_* kind and bounded caller scope against
 * its frozen read registry and returns one exact static statement. This bridge
 * emits no query text, accepts none from a caller, and runs the returned
 * statement on the caller's already-open unit so the read observes uncommitted
 * effects in the caller's serializable transaction. Missing configuration or
 * any configured failure fails closed with no TypeScript fallback.
 */
type Holding = Readonly<{ origin: string; holdingSlug: string; holdingId?: string }>;
type Settings = Readonly<{ base: string; timeoutMs: number; holdings: readonly Holding[] }>;

export const MANUFACTURING_RD_SPECIFICATION = 'specification';
export const MANUFACTURING_RD_SPECIFICATION_VERSION = 'specification-version';
export const MANUFACTURING_RD_SPECIFICATION_LINES = 'specification-lines';
export const MANUFACTURING_RD_ORDERS = 'orders';
export const MANUFACTURING_RD_ORDER_DETAIL = 'order-detail';
export const MANUFACTURING_RD_MATERIAL_RESERVATIONS = 'material-reservations';
export const MANUFACTURING_RD_ORDER_EVENTS = 'order-events';
export const MANUFACTURING_RD_ORDER_POSTINGS = 'order-postings';
export const MANUFACTURING_RD_ORDER_OPERATIONS = 'order-operations';
export const MANUFACTURING_RD_LOT_GENEALOGY = 'lot-genealogy';
export const MANUFACTURING_RD_PRODUCT_GENEALOGY = 'product-genealogy';
export const MANUFACTURING_RD_ACCOUNTING_PROFILES = 'accounting-profiles';

/** Wire order mirrors the Rust `BindField` registry. */
export type NativeManufacturingReadScope = Readonly<{
  id?: string;
  companyId?: string;
  /** Omitted binds SQL NULL and keeps the unscoped branch predicate. */
  branchId?: string;
  specificationId?: string;
  orderId?: string;
  version?: number;
}>;

export type NativeManufacturingPlan = Readonly<{
  kind: string;
  caller: string;
  sql: string;
  values: readonly (string | null)[];
}>;

const KINDS = new Set<string>([
  MANUFACTURING_RD_SPECIFICATION,
  MANUFACTURING_RD_SPECIFICATION_VERSION,
  MANUFACTURING_RD_SPECIFICATION_LINES,
  MANUFACTURING_RD_ORDERS,
  MANUFACTURING_RD_ORDER_DETAIL,
  MANUFACTURING_RD_MATERIAL_RESERVATIONS,
  MANUFACTURING_RD_ORDER_EVENTS,
  MANUFACTURING_RD_ORDER_POSTINGS,
  MANUFACTURING_RD_ORDER_OPERATIONS,
  MANUFACTURING_RD_LOT_GENEALOGY,
  MANUFACTURING_RD_PRODUCT_GENEALOGY,
  MANUFACTURING_RD_ACCOUNTING_PROFILES,
]);

const unavailable = (): never => {
  throw new ApiError(503, 'MANUFACTURING_NATIVE_READ_UNAVAILABLE', 'The native Manufacturing read service is unavailable.');
};
const invalid = (): never => {
  throw new ApiError(503, 'MANUFACTURING_NATIVE_READ_CONFIG_INVALID', 'The native Manufacturing read transport is misconfigured.');
};
const record = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v);
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_MANUFACTURING_READ_CONFIG ?? process.env.ERPX_NATIVE_MANUFACTURING_CONFIG;
  if (raw === undefined) return unavailable();
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return invalid(); }
  if (!record(value) || typeof value.privateBaseUrl !== 'string' || !Array.isArray(value.holdings)) return invalid();
  if (typeof value.timeoutMs !== 'number' || !Number.isSafeInteger(value.timeoutMs) || value.timeoutMs < 250 || value.timeoutMs > 30_000) return invalid();
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { return invalid(); }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port ||
    reservedPorts.has(port) || (port >= 3200 && port <= 3299) || base.username || base.password ||
    value.privateBaseUrl !== base.origin
  ) return invalid();
  const holdings = value.holdings.map((item: unknown): Holding => {
    if (!record(item) || typeof item.origin !== 'string' || typeof item.holdingSlug !== 'string') return invalid();
    if (item.holdingId !== undefined && typeof item.holdingId !== 'string') return invalid();
    let origin: URL;
    try { origin = new URL(item.origin); } catch { return invalid(); }
    if (origin.protocol !== 'https:' || item.origin !== origin.origin || origin.username || origin.password ||
        !/^[a-z0-9_-]{1,128}$/.test(item.holdingSlug)) return invalid();
    return { origin: item.origin, holdingSlug: item.holdingSlug, holdingId: item.holdingId as string | undefined };
  });
  if (holdings.length < 1 || holdings.length > 64) return invalid();
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** Ask the owner to plan one exact registered read on the selected Holding. */
export async function nativeManufacturingPlan(
  kind: string,
  scope: NativeManufacturingReadScope = {},
): Promise<NativeManufacturingPlan> {
  if (!KINDS.has(kind)) {
    throw new ApiError(503, 'MANUFACTURING_READ_PLAN_INVALID', 'Review the Manufacturing owner read plan.');
  }
  if (scope.version !== undefined && !Number.isSafeInteger(scope.version)) {
    throw new ApiError(503, 'MANUFACTURING_READ_PLAN_INVALID', 'Review the Manufacturing owner read plan.');
  }
  for (const value of [scope.id, scope.companyId, scope.branchId, scope.specificationId, scope.orderId]) {
    if (value !== undefined && (value.length === 0 || value.length > 200)) {
      throw new ApiError(503, 'MANUFACTURING_READ_PLAN_INVALID', 'Review the Manufacturing owner read plan.');
    }
  }
  const config = configuration();
  const holding = config.holdings[0]!;
  const key = 'mfg-read-' + createHash('sha256').update(kind).update('\u0000').update(JSON.stringify(scope)).digest('hex').slice(0, 32);
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/manufacturing/read-plan`, config.base);
  let response: Response;
  try {
    const headers = new Headers({
      host: new URL(holding.origin).host,
      origin: holding.origin,
      'content-type': 'application/json',
      accept: 'application/json',
      'cache-control': 'no-cache',
      'idempotency-key': key,
    });
    if (scope.companyId) headers.set('x-erpx-context-company', scope.companyId);
    response = await fetch(url, {
      method: 'POST',
      headers,
      body: JSON.stringify({ kind, scope }),
      cache: 'no-store',
      redirect: 'error',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch { return unavailable(); }
  if (response.status !== 200) {
    const payload: unknown = await response.json().catch((): unknown => null);
    const err = record(payload) && record(payload.error) ? payload.error : undefined;
    if (response.status >= 400 && response.status < 500) {
      throw new ApiError(
        response.status,
        typeof err?.code === 'string' ? err.code : 'MANUFACTURING_NATIVE_READ_FAILED',
        typeof err?.message === 'string' ? err.message : 'The native Manufacturing read was rejected.',
      );
    }
    return unavailable();
  }
  const payload: unknown = await response.json().catch((): unknown => null);
  const data = record(payload) && record(payload.data) ? payload.data : undefined;
  if (!data || typeof data.kind !== 'string' || typeof data.caller !== 'string') return unavailable();
  const statement = record(data.statement) ? data.statement : undefined;
  if (!statement || typeof statement.sql !== 'string') return unavailable();
  const sql = statement.sql;
  // Guard the response without embedding owner query text in this transport.
  if (!/^\s*[Ss][Ee][Ll][Ee][Cc][Tt]\s/.test(sql) || sql.length > 16 * 1024) return unavailable();
  const values = statement.values;
  if (!Array.isArray(values) || values.length !== sql.split('?').length - 1) return unavailable();
  if (!values.every((v) => v === null || typeof v === 'string')) return unavailable();
  return {
    kind: data.kind,
    caller: data.caller,
    sql,
    values: values as readonly (string | null)[],
  };
}

/** Plan on the owner, then execute on the caller's open unit. */
export async function nativeManufacturingReadAll<T>(
  kind: string,
  scope: NativeManufacturingReadScope,
  db: SqlDatabase,
): Promise<SqlResult<T>> {
  const planned = await nativeManufacturingPlan(kind, scope);
  const statement = db.prepare(planned.sql).bind(...planned.values);
  const results = await db.batch<T>([statement]);
  return results[0] ?? { results: [], success: true, meta: { changes: 0, duration: 0 } };
}

export async function nativeManufacturingReadFirst<T>(
  kind: string,
  scope: NativeManufacturingReadScope,
  db: SqlDatabase,
): Promise<T | null> {
  const result = await nativeManufacturingReadAll<T>(kind, scope, db);
  return result.results[0] ?? null;
}
