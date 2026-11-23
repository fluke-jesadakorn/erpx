import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

type Settings = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: ReadonlyArray<Readonly<{ origin: string; holdingSlug: string; holdingId: string }>>;
}>;

/** Bounded auxiliary read resource names owned by the native Legal port. */
export type LegalAuxiliaryResource =
  | 'workspace'
  | 'reports'
  | 'trust'
  | 'views'
  | 'laws'
  | 'knowledge'
  | 'citations'
  | 'retention'
  | 'governance'
  | 'references'
  | 'ai';

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_LEGAL_RECORDS_CONFIG;
  if (!raw)
    throw new ApiError(503, 'NATIVE_LEGAL_UNAVAILABLE', 'The native Legal service is unavailable.');
  try {
    const config = JSON.parse(raw) as Settings;
    if (
      typeof config.privateBaseUrl !== 'string' ||
      !Number.isInteger(config.timeoutMs) ||
      !Array.isArray(config.holdings)
    )
      throw new Error('NATIVE_LEGAL_CONFIGURATION_INVALID');
    return config;
  } catch {
    throw new ApiError(
      503,
      'NATIVE_LEGAL_CONFIGURATION_INVALID',
      'Native Legal transport is misconfigured.',
    );
  }
}

/**
 * Owner-issued Legal auxiliary read. The native owner owns the SQL, the
 * Company/branch/department visibility and the role permission; this bridge
 * carries only a bounded resource name, an optional opaque item id and the
 * signed-in session context. No SQL text remains in the caller, and there is
 * no TypeScript fallback.
 */
export async function nativeLegalAuxiliaryRead<T>(
  actor: WorkContext,
  resource: LegalAuxiliaryResource,
  id?: string,
  query?: URLSearchParams,
): Promise<T> {
  const config = configuration();
  const target = config.holdings.find(
    h => h.holdingSlug === actor.holdingSlug && h.holdingId === actor.holdingId,
  );
  if (!target)
    throw new ApiError(
      503,
      'NATIVE_LEGAL_AUTHORITY_INVALID',
      'The native Legal authority is unavailable.',
    );
  const suffix = query?.size ? `?${query.toString()}` : '';
  const path = id ? `${resource}/${encodeURIComponent(id)}` : resource;
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/legal/${path}${suffix}`,
    config.privateBaseUrl,
  );
  const headers = new Headers({
    host: new URL(target.origin).host,
    origin: target.origin,
    'content-type': 'application/json',
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority?.revision ?? 0),
    'x-erpx-context-assignment-version': String(actor.authority?.assignmentVersion ?? 0),
  });
  if (actor.branchId) headers.set('x-erpx-workspace-branch', actor.branchId);
  if (actor.departmentId) headers.set('x-erpx-workspace-department', actor.departmentId);
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'GET',
      headers,
      cache: 'no-store',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch {
    throw new ApiError(503, 'NATIVE_LEGAL_UNAVAILABLE', 'The native Legal service is unavailable.');
  }
  if (!response.ok) {
    const body = (await response.json().catch(() => null)) as
      | { error?: { code?: string; message?: string } }
      | null;
    throw new ApiError(
      response.status,
      body?.error?.code ?? 'NATIVE_LEGAL_READ_FAILED',
      body?.error?.message ?? 'The native Legal read failed.',
    );
  }
  return (await response.json()) as T;
}
