import 'server-only';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';

type Settings = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: ReadonlyArray<Readonly<{ origin: string; holdingSlug: string; holdingId: string }>>;
}>;

export type NativePmsOwnerWriteResult = Readonly<{
  ok: boolean;
  kind: string;
  now: number;
  key: string;
  row: unknown;
}>;

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_PMS_CONFIG;
  if (!raw)
    throw new ApiError(503, 'NATIVE_PMS_UNAVAILABLE', 'The native Hotel service is unavailable.');
  try {
    const parsed = JSON.parse(raw) as Settings;
    if (!parsed.privateBaseUrl || !Array.isArray(parsed.holdings) || !parsed.timeoutMs)
      throw new Error('invalid');
    return parsed;
  } catch {
    throw new ApiError(503, 'NATIVE_PMS_CONFIGURATION_INVALID', 'Native Hotel transport is misconfigured.');
  }
}

/** Idempotency-Key accepted by the native CommandKey validator. */
function safeKey(kind: string, values: readonly unknown[], supplied?: string): string {
  const source = supplied || `${kind}:${String(values[0] ?? '')}`;
  const cleaned = source.replace(/[^a-zA-Z0-9:._-]/g, '_').slice(0, 160);
  return cleaned.length >= 8 ? cleaned : `pms-owner:${cleaned}`;
}

function parse(response: Response, body: unknown): NativePmsOwnerWriteResult {
  const payload = body as { data?: NativePmsOwnerWriteResult; error?: { code?: string; message?: string } } | null;
  if (!response.ok || !payload?.data || payload.data.ok !== true)
    throw new ApiError(
      response.status,
      payload?.error?.code ?? 'NATIVE_PMS_OWNER_WRITE_FAILED',
      payload?.error?.message ?? 'The native Hotel write failed.',
    );
  return payload.data;
}

/**
 * Execute one owner write on the native PMS axum executor. No SQL crosses the
 * wire and there is no TypeScript fallback: a missing native configuration
 * fails closed. The browser variant pins the exact registered Holding host and
 * origin; the system variant is Company-keyed for workers without a WorkContext.
 */
export async function nativePmsOwnerWrite(
  companyId: string,
  kind: string,
  values: unknown[],
  suppliedKey?: string,
  now = Date.now(),
  actor?: WorkContext,
): Promise<NativePmsOwnerWriteResult> {
  const config = configuration();
  const key = safeKey(kind, values, suppliedKey);
  // A retained worker path without a WorkContext uses the registered default
  // Company token; composition maps it to the configured Holding company.
  const resolvedCompany = companyId || 'pms-default';
  const headers = new Headers({
    'content-type': 'application/json',
    'idempotency-key': key,
    'x-erpx-context-company': resolvedCompany,
  });
  let url: string;
  if (actor) {
    const target = config.holdings.find(
      h => h.holdingSlug === actor.holdingSlug && h.holdingId === actor.holdingId,
    );
    if (!target)
      throw new ApiError(503, 'NATIVE_PMS_AUTHORITY_INVALID', 'The native Hotel authority is unavailable.');
    headers.set('host', new URL(target.origin).host);
    headers.set('origin', target.origin);
    headers.set('x-erpx-actor-user', actor.userId);
    headers.set('x-erpx-context-assignment', actor.assignmentId);
    headers.set('x-erpx-authority-json', JSON.stringify({
      revision: actor.authority?.revision ?? 0,
      assignmentVersion: actor.authority?.assignmentVersion ?? 0,
    }));
    if (actor.branchId) headers.set('x-erpx-workspace-branch', actor.branchId);
    if (actor.departmentId) headers.set('x-erpx-workspace-department', actor.departmentId);
    url = `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/pms/owner-write`;
  } else {
    url = `${config.privateBaseUrl}/api/v1/companies/${encodeURIComponent(resolvedCompany)}/pms/owner-write`;
  }
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
    throw new ApiError(503, 'NATIVE_PMS_UNAVAILABLE', 'The native Hotel service is unavailable.');
  }
  return parse(response, await response.json().catch(() => null));
}
