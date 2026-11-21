import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';

/**
 * Strict client for the single native Organization owner-write axum router.
 * The SQL is owned by the Rust owner crate; this module sends only a typed
 * allow-listed kind, positional values and a command key. There is no TS SQL
 * copy, no fallback writer and no partial projection: an absent transport or
 * an invalid reply fails closed.
 */
type Settings = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: ReadonlyArray<Readonly<{ origin: string; holdingSlug: string; holdingId: string }>>;
}>;

type Reply = Readonly<{
  data?: Readonly<{ kind?: string; rows?: number; sql?: string; values?: unknown[] }>;
  error?: Readonly<{ code?: string; message?: string }>;
}>;

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
  if (!raw) throw new ApiError(503, 'NATIVE_ORGANIZATION_UNAVAILABLE', 'The native Organization service is unavailable.');
  let value: unknown;
  try { value = JSON.parse(raw); }
  catch { throw new ApiError(503, 'NATIVE_ORGANIZATION_CONFIGURATION_INVALID', 'Organization transport is misconfigured.'); }
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new ApiError(503, 'NATIVE_ORGANIZATION_CONFIGURATION_INVALID', 'Organization transport is misconfigured.');
  const settings = value as Partial<Settings>;
  if (
    typeof settings.privateBaseUrl !== 'string' ||
    typeof settings.timeoutMs !== 'number' || !Number.isSafeInteger(settings.timeoutMs) ||
    settings.timeoutMs < 250 || settings.timeoutMs > 30000 ||
    !Array.isArray(settings.holdings) || settings.holdings.length < 1 || settings.holdings.length > 64
  )
    throw new ApiError(503, 'NATIVE_ORGANIZATION_CONFIGURATION_INVALID', 'Organization transport is misconfigured.');
  return settings as Settings;
}

function identifier(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= 200;
}

async function post(path: string, headers: Record<string, string>, body: unknown, timeoutMs: number): Promise<Reply> {
  let response: Response;
  try {
    response = await fetch(path, {
      method: 'POST',
      headers,
      body: JSON.stringify(body),
      cache: 'no-store',
      signal: AbortSignal.timeout(timeoutMs),
    });
  } catch {
    throw new ApiError(503, 'NATIVE_ORGANIZATION_UNAVAILABLE', 'The native Organization service is unavailable.');
  }
  const payload = await response.json().catch(() => null) as Reply | null;
  if (!response.ok || !payload?.data)
    throw new ApiError(
      response.status,
      payload?.error?.code ?? 'NATIVE_ORGANIZATION_FAILED',
      payload?.error?.message ?? 'The native Organization request failed.',
    );
  return payload;
}

function command(kind: string, values: unknown[], key: string, now: number, mode?: 'statement') {
  if (!identifier(kind) || values.length < 1 || values.length > 64 || !identifier(key))
    throw new ApiError(422, 'ORGANIZATION_OWNER_WRITE_INVALID', 'Review the organization write request.');
  return mode === 'statement' ? { kind, values, now, key: undefined, mode } : { kind, values, now };
}

/** Company-keyed system write. Returns the authoritative owner SQL statement
 * in the caller's own transaction; used only where a cross-owner command must
 * keep the write in its existing unit. */
export async function nativeOrgStatement(
  db: SqlDatabase,
  companyId: string,
  kind: string,
  values: unknown[],
  key: string,
  now = Date.now(),
): Promise<SqlStatement> {
  if (!identifier(companyId))
    throw new ApiError(503, 'NATIVE_ORGANIZATION_AUTHORITY_INVALID', 'The native Organization authority is unavailable.');
  const config = configuration();
  const payload = await post(
    `${config.privateBaseUrl}/api/v1/companies/${encodeURIComponent(companyId)}/organization/owner-write`,
    {
      'content-type': 'application/json',
      'idempotency-key': key,
      'x-erpx-context-company': companyId,
    },
    command(kind, values, key, now, 'statement'),
    config.timeoutMs,
  );
  const data = payload.data!;
  if (typeof data.sql !== 'string' || !Array.isArray(data.values) || data.values.length > 64)
    throw new ApiError(503, 'NATIVE_ORGANIZATION_RESPONSE_INVALID', 'The native owner write was invalid.');
  return db.prepare(data.sql).bind(...data.values);
}

/** Company-keyed system write executed by the native owner unit with receipt,
 * audit and outbox effects. */
export async function nativeOrgExecute(
  companyId: string,
  kind: string,
  values: unknown[],
  key: string,
  now = Date.now(),
): Promise<{ kind: string; rows: number }> {
  if (!identifier(companyId))
    throw new ApiError(503, 'NATIVE_ORGANIZATION_AUTHORITY_INVALID', 'The native Organization authority is unavailable.');
  const config = configuration();
  const payload = await post(
    `${config.privateBaseUrl}/api/v1/companies/${encodeURIComponent(companyId)}/organization/owner-write`,
    {
      'content-type': 'application/json',
      'idempotency-key': key,
      'x-erpx-context-company': companyId,
    },
    command(kind, values, key, now),
    config.timeoutMs,
  );
  const data = payload.data!;
  if (typeof data.kind !== 'string' || typeof data.rows !== 'number')
    throw new ApiError(503, 'NATIVE_ORGANIZATION_RESPONSE_INVALID', 'The native owner write was invalid.');
  return { kind: data.kind, rows: data.rows };
}

/** Browser-WorkContext write executed by the native owner unit. */
export async function nativeOrgBrowserExecute(
  actor: WorkContext,
  kind: string,
  values: unknown[],
  key: string,
  now = Date.now(),
): Promise<{ kind: string; rows: number }> {
  const config = configuration();
  const holding = config.holdings.find(item => item.holdingSlug === actor.holdingSlug && item.holdingId === actor.holdingId);
  if (!holding || !identifier(actor.companyId) || !identifier(actor.userId))
    throw new ApiError(503, 'NATIVE_ORGANIZATION_AUTHORITY_INVALID', 'The native Organization authority is unavailable.');
  const headers: Record<string, string> = {
    host: new URL(holding.origin).host,
    origin: holding.origin,
    'content-type': 'application/json',
    'idempotency-key': key,
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-user': actor.userId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority?.revision ?? 0),
    'x-erpx-context-assignment-version': String(actor.authority?.assignmentVersion ?? 0),
  };
  const payload = await post(
    `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/organization/owner-write`,
    headers,
    command(kind, values, key, now),
    config.timeoutMs,
  );
  const data = payload.data!;
  if (typeof data.kind !== 'string' || typeof data.rows !== 'number')
    throw new ApiError(503, 'NATIVE_ORGANIZATION_RESPONSE_INVALID', 'The native owner write was invalid.');
  return { kind: data.kind, rows: data.rows };
}
