import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

type Settings = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: ReadonlyArray<
    Readonly<{
      origin: string;
      holdingSlug: string;
      holdingId: string;
      companyId?: string;
      assignmentId?: string;
    }>
  >;
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
  | 'inspection-source'
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

/**
 * Native inspection-source point read. The caller holds only the selected
 * Company and an opaque document id, so the exact registered Holding is
 * selected by its configured Company; there is no first-Holding default and
 * no TypeScript SQL. The owner binds the document id and Company in one read.
 */
export async function nativeInspectionSourceRead<T>(
  companyId: string,
  documentId: string,
): Promise<T> {
  if (
    !companyId ||
    companyId.length > 200 ||
    !documentId ||
    documentId.length > 200 ||
    !/^[A-Za-z0-9._:\/-]+$/.test(documentId)
  )
    throw new ApiError(
      422,
      'NATIVE_LEGAL_READ_REQUEST_INVALID',
      'Use one bounded Legal inspection source.',
    );
  const config = configuration();
  const target = config.holdings.find(holding => holding.companyId === companyId);
  if (!target)
    throw new ApiError(
      503,
      'NATIVE_LEGAL_AUTHORITY_INVALID',
      'The native Legal authority is unavailable.',
    );
  const actor = {
    companyId,
    holdingId: target.holdingId,
    holdingSlug: target.holdingSlug,
    assignmentId: target.assignmentId ?? `${companyId}:inspection-source`,
    branchId: null,
    departmentId: null,
    authority: { revision: 0, assignmentVersion: 0 },
  } as unknown as WorkContext;
  return nativeLegalAuxiliaryRead<T>(actor, 'inspection-source', documentId);
}

export type LegalReadPlan = Readonly<{ sql: string; values: unknown[] }>;

/**
 * Owner-issued bounded read plan. The native Legal service resolves one
 * allow-listed kind to the exact statement the retired owner literal bound;
 * this bridge carries only the kind and positional values and executes the
 * returned statement on the caller's already-open unit. No SQL is accepted
 * from the wire and there is no TypeScript fallback.
 */
export async function nativeLegalReadPlan(
  kind: string,
  values: readonly unknown[],
): Promise<LegalReadPlan> {
  if (!kind || kind.length > 120 || values.length > 16)
    throw new ApiError(
      422,
      'NATIVE_LEGAL_READ_REQUEST_INVALID',
      'Use one bounded Legal read plan.',
    );
  const config = configuration();
  const target = config.holdings[0];
  if (!target)
    throw new ApiError(
      503,
      'NATIVE_LEGAL_AUTHORITY_INVALID',
      'The native Legal authority is unavailable.',
    );
  if (
    !Number.isInteger(config.timeoutMs) ||
    config.timeoutMs < 250 ||
    config.timeoutMs > 30_000
  )
    throw new ApiError(
      503,
      'NATIVE_LEGAL_CONFIGURATION_INVALID',
      'Native Legal transport is misconfigured.',
    );
  let base: URL;
  let origin: URL;
  try {
    base = new URL(config.privateBaseUrl);
    origin = new URL(target.origin);
  } catch {
    throw new ApiError(
      503,
      'NATIVE_LEGAL_CONFIGURATION_INVALID',
      'Native Legal transport is misconfigured.',
    );
  }
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    base.username ||
    base.password ||
    origin.protocol !== 'https:' ||
    origin.origin !== target.origin ||
    origin.username ||
    origin.password
  )
    throw new ApiError(
      503,
      'NATIVE_LEGAL_CONFIGURATION_INVALID',
      'Native Legal transport is misconfigured.',
    );
  const body = JSON.stringify({ kind, values });
  if (new TextEncoder().encode(body).length > 16 * 1024)
    throw new ApiError(
      422,
      'NATIVE_LEGAL_READ_REQUEST_INVALID',
      'Use one bounded Legal read plan.',
    );
  let response: Response;
  try {
    response = await fetch(
      new URL(
        `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/legal/command-plan`,
        base,
      ),
      {
        method: 'POST',
        headers: {
          host: origin.host,
          origin: target.origin,
          accept: 'application/json',
          'content-type': 'application/json',
        },
        body,
        cache: 'no-store',
        redirect: 'error',
        signal: AbortSignal.timeout(config.timeoutMs),
      },
    );
  } catch {
    throw new ApiError(
      503,
      'NATIVE_LEGAL_UNAVAILABLE',
      'The native Legal service is unavailable.',
    );
  }
  if (!response.ok) {
    const payload = (await response.json().catch(() => null)) as
      | { error?: { code?: unknown; message?: unknown } }
      | null;
    throw new ApiError(
      response.status,
      typeof payload?.error?.code === 'string'
        ? payload.error.code
        : 'NATIVE_LEGAL_READ_FAILED',
      typeof payload?.error?.message === 'string'
        ? payload.error.message
        : 'The native Legal read failed.',
    );
  }
  const payload = (await response.json().catch(() => null)) as
    | { data?: { statements?: unknown } }
    | null;
  const statements = payload?.data?.statements;
  if (!Array.isArray(statements) || statements.length !== 1)
    throw new ApiError(
      502,
      'NATIVE_LEGAL_READ_RESPONSE_INVALID',
      'The native Legal plan reply was invalid.',
    );
  const entry = statements[0];
  if (entry === null || typeof entry !== 'object' || Array.isArray(entry))
    throw new ApiError(
      502,
      'NATIVE_LEGAL_READ_RESPONSE_INVALID',
      'The native Legal plan reply was invalid.',
    );
  const planned = entry as { sql?: unknown; values?: unknown };
  if (
    typeof planned.sql !== 'string' ||
    planned.sql.length === 0 ||
    planned.sql.length > 200_000 ||
    !Array.isArray(planned.values) ||
    planned.values.length > 16
  )
    throw new ApiError(
      502,
      'NATIVE_LEGAL_READ_RESPONSE_INVALID',
      'The native Legal plan reply was invalid.',
    );
  return { sql: planned.sql, values: planned.values };
}
