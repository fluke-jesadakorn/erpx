import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

type Settings = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: ReadonlyArray<Readonly<{ origin: string; holdingSlug: string; holdingId: string }>>;
}>;

type Statement = Readonly<{ sql: string; values: unknown[] }>;

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_COLLABORATION_CONFIG;
  if (!raw) throw new ApiError(503, 'NATIVE_COLLABORATION_UNAVAILABLE', 'The native Collaboration service is unavailable.');
  try {
    return JSON.parse(raw) as Settings;
  } catch {
    throw new ApiError(503, 'NATIVE_COLLABORATION_CONFIGURATION_INVALID', 'Collaboration transport is misconfigured.');
  }
}

/** Ask the native Collaboration owner for the authoritative notification INSERT.
 * The caller executes it inside its own serializable unit. */
export async function nativeNotificationStatement(
  db: SqlDatabase,
  actor: WorkContext,
  input: {
    companyId: string;
    userId: string;
    recordType: string;
    recordId: string;
    recordVersion: number;
    message: string;
    kind?: string;
    leadId?: string;
    identifier?: string;
    approver_user_id?: string;
    item_id?: string;
    bill_id?: string;
    rfq_id?: string;
  },
  key: string,
  now: number,
): Promise<SqlStatement> {
  const config = configuration();
  const target = config.holdings.find(
    (holding) => holding.holdingSlug === actor.holdingSlug && holding.holdingId === actor.holdingId,
  );
  if (!target) throw new ApiError(503, 'NATIVE_COLLABORATION_AUTHORITY_INVALID', 'The native Collaboration authority is unavailable.');
  const headers = new Headers({
    host: new URL(target.origin).host,
    origin: target.origin,
    'content-type': 'application/json',
    'idempotency-key': key,
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority?.revision ?? 0),
    'x-erpx-context-assignment-version': String(actor.authority?.assignmentVersion ?? 0),
  });
  if (actor.branchId) headers.set('x-erpx-workspace-branch', actor.branchId);
  if (actor.departmentId) headers.set('x-erpx-workspace-department', actor.departmentId);
  let response: Response;
  try {
    response = await fetch(
      `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/collaboration/notification-statement`,
      {
        method: 'POST',
        headers,
        body: JSON.stringify({ ...input, now, idempotencyKey: key }),
        cache: 'no-store',
        signal: AbortSignal.timeout(config.timeoutMs),
      },
    );
  } catch {
    throw new ApiError(503, 'NATIVE_COLLABORATION_UNAVAILABLE', 'The native Collaboration service is unavailable.');
  }
  const body = (await response.json().catch(() => null)) as
    | { data?: Statement; error?: { code?: string; message?: string } }
    | null;
  if (!response.ok || !body?.data)
    throw new ApiError(response.status, body?.error?.code ?? 'NATIVE_COLLABORATION_FAILED', body?.error?.message ?? 'The Collaboration request failed.');
  return db.prepare(body.data.sql).bind(...body.data.values);
}
