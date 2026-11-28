import 'server-only';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';

type Settings = Readonly<{ privateBaseUrl: string; timeoutMs: number }>;
type Statement = Readonly<{ sql: string; values: unknown[] }>;

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_COLLABORATION_CONFIG;
  if (!raw) throw new ApiError(503, 'NATIVE_COLLABORATION_UNAVAILABLE', 'The native Collaboration service is unavailable.');
  try { return JSON.parse(raw) as Settings; }
  catch { throw new ApiError(503, 'NATIVE_COLLABORATION_CONFIGURATION_INVALID', 'Collaboration transport is misconfigured.'); }
}

/** Ask the native Collaboration owner for an authoritative notification INSERT
 * using a Company id only (no browser WorkContext). Reachable only on the
 * private native API. The caller executes the statement in its own unit. */
export async function nativeSystemNotificationStatement(
  db: SqlDatabase,
  companyId: string,
  input: {
    userId: string;
    recordType: string;
    recordId: string;
    recordVersion: number;
    message: string;
    kind?: string;
    node_id?: string;
    scope_node_id?: string;
    runtime_predicate?: string;
    table_name?: string;
    cte_prefix?: string;
    source_access_sql?: string;
    target_kind?: string;
    target_id?: string;
    identifier?: string;
    item_id?: string;
    timestamp?: number;
    order_id?: string;
    source_hash?: string;
    event?: string;
    values_json?: string;
  },
  key: string,
  now: number,
): Promise<SqlStatement> {
  const config = configuration();
  const headers = new Headers({
    'content-type': 'application/json',
    'idempotency-key': key,
    'x-erpx-context-company': companyId,
  });
  // The native input is camelCase with deny_unknown_fields; the retained
  // TypeScript callers still carry a few snake_case keys (values_json, ...).
  const payload = Object.fromEntries(
    Object.entries({ ...input, companyId, now, idempotencyKey: key }).map(([name, value]) => [
      name.replace(/_([a-z])/g, (_, letter: string) => letter.toUpperCase()),
      value,
    ]),
  );
  let response: Response;
  try {
    response = await fetch(
      `${config.privateBaseUrl}/api/v1/companies/${encodeURIComponent(companyId)}/collaboration/notification-statement`,
      { method: 'POST', headers, body: JSON.stringify(payload), cache: 'no-store', signal: AbortSignal.timeout(config.timeoutMs) },
    );
  } catch {
    throw new ApiError(503, 'NATIVE_COLLABORATION_UNAVAILABLE', 'The native Collaboration service is unavailable.');
  }
  const body = (await response.json().catch(() => null)) as { data?: Statement; error?: { code?: string; message?: string } } | null;
  if (!response.ok || !body?.data)
    throw new ApiError(response.status, body?.error?.code ?? 'NATIVE_COLLABORATION_FAILED', body?.error?.message ?? 'The native Collaboration request failed.');
  return db.prepare(body.data.sql).bind(...body.data.values);
}


/** Holding-keyed variant for system workers with a Holding id but no Company id. */
export async function nativeHoldingNotificationStatement(
  db: SqlDatabase,
  holdingId: string,
  input: {
    userId: string; recordType: string; recordId: string; recordVersion: number;
    message: string; kind?: string; runtime_predicate?: string; table_name?: string; cte_prefix?: string; timestamp?: number; values_json?: string;
  },
  key: string,
  now: number,
): Promise<SqlStatement> {
  const config = configuration();
  const headers = new Headers({ 'content-type': 'application/json', 'idempotency-key': key, 'x-erpx-context-holding': holdingId });
  // The native input is camelCase with deny_unknown_fields; the retained
  // TypeScript callers still carry snake_case keys (table_name, values_json).
  const payload = Object.fromEntries(
    Object.entries({ ...input, now, idempotencyKey: key }).map(([name, value]) => [
      name.replace(/_([a-z])/g, (_, letter: string) => letter.toUpperCase()),
      value,
    ]),
  );
  let response: Response;
  try {
    response = await fetch(
      `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(holdingId)}/collaboration/notification-statement-by-holding`,
      { method: 'POST', headers, body: JSON.stringify(payload), cache: 'no-store', signal: AbortSignal.timeout(config.timeoutMs) },
    );
  } catch { throw new ApiError(503, 'NATIVE_COLLABORATION_UNAVAILABLE', 'The native Collaboration service is unavailable.'); }
  const body = (await response.json().catch(() => null)) as { data?: Statement; error?: { code?: string; message?: string } } | null;
  if (!response.ok || !body?.data) throw new ApiError(response.status, body?.error?.code ?? 'NATIVE_COLLABORATION_FAILED', body?.error?.message ?? 'The native Collaboration request failed.');
  return db.prepare(body.data.sql).bind(...body.data.values);
}
