import 'server-only';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';

/**
 * Fail-closed transport for the native Legal wf3a13 write slice.
 *
 * The owner Rust crate owns every SQL string and argument order. This bridge
 * carries a typed `kind` and bounded positional `values` only; it never
 * assembles SQL and has no TypeScript fallback. The caller executes the
 * returned statement in its own serializable unit, together with the same-unit
 * receipt/audit/outbox effects that remain in the caller transaction.
 */
type Holding = Readonly<{ holdingId: string; holdingSlug: string; origin: string }>;
type Settings = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: readonly Holding[];
  systemHoldingId?: string;
}>;
type Statement = Readonly<{ sql: string; values: unknown[] }>;

export type LegalWave3Scope =
  | Readonly<{ kind: 'holding'; id: string }>
  | Readonly<{ kind: 'company'; id: string }>
  | Readonly<{ kind: 'system' }>
  | Readonly<{ kind: 'browser'; holdingId: string; holdingSlug: string }>;

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_LEGAL_WAVE3_CONFIG;
  if (!raw)
    throw new ApiError(503, 'NATIVE_LEGAL_WAVE3_UNAVAILABLE', 'The native Legal service is unavailable.');
  try {
    return JSON.parse(raw) as Settings;
  } catch {
    throw new ApiError(503, 'NATIVE_LEGAL_WAVE3_CONFIG_INVALID', 'Native Legal transport is misconfigured.');
  }
}

function endpoint(scope: LegalWave3Scope, config: Settings): { url: string; headers: Headers } {
  const keyed = (url: string, headerName: string, id: string) => {
    const headers = new Headers({ 'content-type': 'application/json' });
    headers.set(headerName, id);
    return { url, headers };
  };
  if (scope.kind === 'system') {
    if (!config.systemHoldingId)
      throw new ApiError(503, 'NATIVE_LEGAL_WAVE3_AUTHORITY_INVALID', 'No native Legal system Holding is configured.');
    return keyed(
      `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(config.systemHoldingId)}/legal/wave3-command-by-holding`,
      'x-erpx-context-holding',
      config.systemHoldingId,
    );
  }
  if (scope.kind === 'company')
    return keyed(
      `${config.privateBaseUrl}/api/v1/companies/${encodeURIComponent(scope.id)}/legal/wave3-command`,
      'x-erpx-context-company',
      scope.id,
    );
  if (scope.kind === 'holding')
    return keyed(
      `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(scope.id)}/legal/wave3-command-by-holding`,
      'x-erpx-context-holding',
      scope.id,
    );
  const holding = config.holdings.find(
    h => h.holdingId === scope.holdingId && h.holdingSlug === scope.holdingSlug,
  );
  if (!holding)
    throw new ApiError(503, 'NATIVE_LEGAL_WAVE3_AUTHORITY_INVALID', 'The native Legal authority is unavailable.');
  return {
    url: `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(scope.holdingSlug)}/legal/wave3-command`,
    headers: new Headers({
      host: new URL(holding.origin).host,
      origin: holding.origin,
      'content-type': 'application/json',
      'x-erpx-context-holding': scope.holdingId,
    }),
  };
}

export async function legalWave3Statement(
  db: SqlDatabase,
  scope: LegalWave3Scope,
  kind: string,
  values: readonly unknown[],
  key: string,
  now = Date.now(),
): Promise<SqlStatement> {
  if (!/^[a-z0-9_]{3,80}$/.test(kind) || values.length === 0 || values.length > 64)
    throw new ApiError(422, 'LEGAL_WAVE3_INVALID', 'Review the Legal owner write request.');
  const config = configuration();
  const { url, headers } = endpoint(scope, config);
  headers.set('idempotency-key', key);
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'POST',
      headers,
      body: JSON.stringify({ kind, values, now }),
      cache: 'no-store',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch {
    throw new ApiError(503, 'NATIVE_LEGAL_WAVE3_UNAVAILABLE', 'The native Legal service is unavailable.');
  }
  const body = (await response.json().catch(() => null)) as
    | { data?: Statement; error?: { code?: string; message?: string } }
    | null;
  if (!response.ok || !body?.data || typeof body.data.sql !== 'string' || !Array.isArray(body.data.values))
    throw new ApiError(
      response.ok ? 503 : response.status,
      body?.error?.code ?? 'NATIVE_LEGAL_WAVE3_FAILED',
      body?.error?.message ?? 'The native Legal request failed.',
    );
  return db.prepare(body.data.sql).bind(...body.data.values);
}

/** Fetch an owner-native CTE text from the Rust owner route. The returned SQL
 * is authored entirely in the owner crate; the caller hands it to the existing
 * native notification helper. No CTE literal remains in TypeScript. */
export async function legalWave3Cte(
  scope: LegalWave3Scope,
  kind: string,
  predicate: string,
  values: readonly unknown[] = [],
  now = Date.now(),
): Promise<string> {
  const config = configuration();
  const { url, headers } = endpoint(scope, config);
  headers.set('idempotency-key', `legal-cte:${kind}:${now}`);
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'POST', headers,
      body: JSON.stringify({ kind, values: [...values, predicate], now }),
      cache: 'no-store', signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch { throw new ApiError(503, 'NATIVE_LEGAL_WAVE3_UNAVAILABLE', 'The native Legal service is unavailable.'); }
  const body = (await response.json().catch(() => null)) as { data?: Statement; error?: { code?: string; message?: string } } | null;
  if (!response.ok || !body?.data || typeof body.data.sql !== 'string')
    throw new ApiError(response.ok ? 503 : response.status, body?.error?.code ?? 'NATIVE_LEGAL_WAVE3_FAILED', body?.error?.message ?? 'The native Legal request failed.');
  return body.data.sql;
}
