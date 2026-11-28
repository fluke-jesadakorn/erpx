import 'server-only';
import { createHash } from 'node:crypto';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { SqlDatabase, SqlResult } from '@erpx/platform-runtime-server/public/runtime/types';

/**
 * Strict owner transport for the native Assets `read-plan` route.
 *
 * The axum owner resolves a closed owner read kind and bounded caller scope
 * against its frozen read registry and returns one exact static statement. This
 * bridge emits no query text, accepts none from a caller, and runs the returned
 * statement on the caller's already-open unit so the read observes uncommitted
 * effects in the caller's serializable transaction. Missing configuration or
 * any configured failure fails closed with no TypeScript fallback.
 */
type Holding = Readonly<{ origin: string; holdingSlug: string; holdingId?: string }>;
type Settings = Readonly<{ base: string; timeoutMs: number; holdings: readonly Holding[] }>;

export const ASSETS_RD_FS_ASSET_SOURCE_ROWS = 'field-service-asset-source-rows';
export const ASSETS_RD_FS_ASSET_ATTACHMENT_IMAGE = 'field-service-asset-attachment-image';
export const ASSETS_RD_PKG_ASSET_SOURCES = 'package-asset-sources';
export const ASSETS_RD_PKG_LINKS = 'package-finance-links';
export const ASSETS_RD_OBLIGATIONS_BASE = 'package-obligations-base';
export const ASSETS_RD_OBLIGATIONS_PREVENTIVE = 'package-obligations-preventive';
export const ASSETS_RD_OBLIGATIONS_PARTS = 'package-obligations-parts';
export const ASSETS_RD_PACKAGE_CLOCK = 'package-clock-now-ms';

/** Wire order mirrors the owner `BindField` registry. */
export type NativeAssetsReadScope = Readonly<{
  assetId?: string;
  companyId?: string;
  /** `packageId === 'assets'`; bound as a PG boolean. */
  isBase?: boolean;
}>;

export type NativeAssetsPlan = Readonly<{
  kind: string;
  caller: string;
  sql: string;
  values: readonly (string | null)[];
}>;

const KINDS = new Set<string>([
  ASSETS_RD_FS_ASSET_SOURCE_ROWS,
  ASSETS_RD_FS_ASSET_ATTACHMENT_IMAGE,
  ASSETS_RD_PKG_ASSET_SOURCES,
  ASSETS_RD_PKG_LINKS,
  ASSETS_RD_OBLIGATIONS_BASE,
  ASSETS_RD_OBLIGATIONS_PREVENTIVE,
  ASSETS_RD_OBLIGATIONS_PARTS,
  ASSETS_RD_PACKAGE_CLOCK,
]);

const unavailable = (): never => {
  throw new ApiError(503, 'ASSETS_NATIVE_READ_UNAVAILABLE', 'The native Assets read service is unavailable.');
};
const invalid = (): never => {
  throw new ApiError(503, 'ASSETS_NATIVE_READ_CONFIG_INVALID', 'The native Assets read transport is misconfigured.');
};
const record = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v);
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

/** Selector presence only; an absent configuration never probes the service. */
export function nativeAssetsReadConfigured(): boolean {
  return process.env.ERPX_NATIVE_ASSETS_READ_CONFIG !== undefined
    || process.env.ERPX_NATIVE_ASSETS_CONFIG !== undefined;
}

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_ASSETS_READ_CONFIG ?? process.env.ERPX_NATIVE_ASSETS_CONFIG;
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
export async function nativeAssetsPlan(
  kind: string,
  scope: NativeAssetsReadScope = {},
): Promise<NativeAssetsPlan> {
  if (!KINDS.has(kind)) {
    throw new ApiError(503, 'ASSETS_READ_PLAN_INVALID', 'Review the Assets owner read plan.');
  }
  if (scope.isBase !== undefined && typeof scope.isBase !== 'boolean') {
    throw new ApiError(503, 'ASSETS_READ_PLAN_INVALID', 'Review the Assets owner read plan.');
  }
  for (const value of [scope.assetId, scope.companyId]) {
    if (value !== undefined && (value.length === 0 || value.length > 200)) {
      throw new ApiError(503, 'ASSETS_READ_PLAN_INVALID', 'Review the Assets owner read plan.');
    }
  }
  const config = configuration();
  const holding = config.holdings[0]!;
  const key = 'ast-read-' + createHash('sha256').update(kind).update('\u0000').update(JSON.stringify(scope)).digest('hex').slice(0, 32);
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/assets/read-plan`, config.base);
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
        typeof err?.code === 'string' ? err.code : 'ASSETS_NATIVE_READ_FAILED',
        typeof err?.message === 'string' ? err.message : 'The native Assets read was rejected.',
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
  // Assets obligation plans begin with WITH (CTE), other plans with a projection.
  if (!/^\s*(?:[Ss][Ee][Ll][Ee][Cc][Tt]|[Ww][Ii][Tt][Hh])\s/.test(sql) || sql.length > 64 * 1024) return unavailable();
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

/**
 * Plan the cross-owner package clock scalar on the owner. The caller embeds the
 * returned statement as a materialized CTE body; no statement text is built or
 * retained in this package, and any owner/transport failure fails closed.
 */
export async function nativeAssetsClockScalar(): Promise<string> {
  const planned = await nativeAssetsPlan(ASSETS_RD_PACKAGE_CLOCK);
  if (planned.values.length !== 0) return unavailable();
  const sql = planned.sql;
  // Structural guard only: never embed owner query text in the transport.
  if (sql.length > 256 || sql.includes(';') || sql.includes('?') ||
      !sql.endsWith(' now_ms') || !sql.includes('clock_timestamp()') ||
      !/^[A-Za-z]+\s/.test(sql)) return unavailable();
  return sql;
}

/** Plan on the owner, then execute on the caller's open unit. */
export async function nativeAssetsReadAll<T>(
  kind: string,
  scope: NativeAssetsReadScope,
  db: SqlDatabase,
): Promise<SqlResult<T>> {
  const planned = await nativeAssetsPlan(kind, scope);
  const statement = db.prepare(planned.sql).bind(...planned.values);
  const results = await db.batch<T>([statement]);
  return results[0] ?? { results: [], success: true, meta: { changes: 0, duration: 0 } };
}

export async function nativeAssetsReadFirst<T>(
  kind: string,
  scope: NativeAssetsReadScope,
  db: SqlDatabase,
): Promise<T | null> {
  const result = await nativeAssetsReadAll<T>(kind, scope, db);
  return result.results[0] ?? null;
}
