import 'server-only';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';

/** Fail-closed transport for the native Customer Authority wf3a13 write slice.
 * Only a typed kind and bounded positional values are sent; the owner Rust
 * crate owns every SQL string. No TS fallback exists. */
type Holding = Readonly<{ holdingId: string; holdingSlug: string; origin: string }>;
type Settings = Readonly<{ privateBaseUrl: string; timeoutMs: number; holdings: readonly Holding[]; systemCompanyId?: string }>;
type Statement = Readonly<{ sql: string; values: unknown[] }>;

export type CustomerAuthorityWave3Scope =
  | Readonly<{ kind: 'company'; id: string }>
  | Readonly<{ kind: 'system' }>
  | Readonly<{ kind: 'browser'; holdingId: string; holdingSlug: string }>;

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_CUSTOMER_AUTHORITY_WAVE3_CONFIG;
  if (!raw)
    throw new ApiError(503, 'NATIVE_CUSTOMER_AUTHORITY_WAVE3_UNAVAILABLE', 'The native Customer Authority service is unavailable.');
  try { return JSON.parse(raw) as Settings; }
  catch { throw new ApiError(503, 'NATIVE_CUSTOMER_AUTHORITY_WAVE3_CONFIG_INVALID', 'Native Customer Authority transport is misconfigured.'); }
}

export async function customerAuthorityWave3Statement(
  db: SqlDatabase,
  scope: CustomerAuthorityWave3Scope,
  kind: string,
  values: readonly unknown[],
  key: string,
  now = Date.now(),
): Promise<SqlStatement> {
  if (!/^[a-z0-9_]{3,80}$/.test(kind) || values.length === 0 || values.length > 64)
    throw new ApiError(422, 'CUSTOMER_AUTHORITY_WAVE3_INVALID', 'Review the Customer Authority owner write request.');
  const config = configuration();
  let url: string;
  const headers = new Headers({ 'content-type': 'application/json' });
  if (scope.kind === 'system') {
    if (!config.systemCompanyId) throw new ApiError(503, 'NATIVE_CUSTOMER_AUTHORITY_WAVE3_AUTHORITY_INVALID', 'No native Customer Authority system Company is configured.');
    url = `${config.privateBaseUrl}/api/v1/companies/${encodeURIComponent(config.systemCompanyId)}/customer-authority/wave3-command`;
    headers.set('x-erpx-context-company', config.systemCompanyId);
  } else if (scope.kind === 'company') {
    url = `${config.privateBaseUrl}/api/v1/companies/${encodeURIComponent(scope.id)}/customer-authority/wave3-command`;
    headers.set('x-erpx-context-company', scope.id);
  } else {
    const holding = config.holdings.find(h => h.holdingId === scope.holdingId && h.holdingSlug === scope.holdingSlug);
    if (!holding)
      throw new ApiError(503, 'NATIVE_CUSTOMER_AUTHORITY_WAVE3_AUTHORITY_INVALID', 'The native Customer Authority authority is unavailable.');
    url = `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(scope.holdingSlug)}/customer-authority/wave3-command`;
    headers.set('host', new URL(holding.origin).host);
    headers.set('origin', holding.origin);
    headers.set('x-erpx-context-holding', scope.holdingId);
  }
  headers.set('idempotency-key', key);
  let response: Response;
  try {
    response = await fetch(url, { method: 'POST', headers, body: JSON.stringify({ kind, values, now }), cache: 'no-store', signal: AbortSignal.timeout(config.timeoutMs) });
  } catch {
    throw new ApiError(503, 'NATIVE_CUSTOMER_AUTHORITY_WAVE3_UNAVAILABLE', 'The native Customer Authority service is unavailable.');
  }
  const body = (await response.json().catch(() => null)) as { data?: Statement; error?: { code?: string; message?: string } } | null;
  if (!response.ok || !body?.data || typeof body.data.sql !== 'string' || !Array.isArray(body.data.values))
    throw new ApiError(response.ok ? 503 : response.status, body?.error?.code ?? 'NATIVE_CUSTOMER_AUTHORITY_WAVE3_FAILED', body?.error?.message ?? 'The native Customer Authority request failed.');
  return db.prepare(body.data.sql).bind(...body.data.values);
}
