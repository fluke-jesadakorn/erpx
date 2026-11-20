import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { ReceiptQualityHold } from '@erpx/server-quality/domain/quality';

type Settings = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: ReadonlyArray<Readonly<{ origin: string; holdingSlug: string; holdingId: string }>>;
}>;

export type HoldStatement = Readonly<{ sql: string; values: unknown[] }>;
export type CustomerReturnHoldStatements = Readonly<{
  specGuard: HoldStatement | null;
  hold: HoldStatement;
}>;

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_QUALITY_CONFIG;
  if (!raw) throw new ApiError(503, 'NATIVE_QUALITY_UNAVAILABLE', 'The native Quality service is unavailable.');
  try {
    return JSON.parse(raw) as Settings;
  } catch {
    throw new ApiError(503, 'NATIVE_QUALITY_CONFIGURATION_INVALID', 'Quality transport is misconfigured.');
  }
}

/** Ask the native Quality owner for the authoritative customer-return hold
 * statements. The caller executes them inside its own serializable unit. */
export async function nativeCustomerReturnHoldStatements(
  actor: WorkContext,
  input: ReceiptQualityHold,
  createdBy: string,
  now: number,
): Promise<CustomerReturnHoldStatements> {
  const config = configuration();
  const target = config.holdings.find(
    (holding) => holding.holdingSlug === actor.holdingSlug && holding.holdingId === actor.holdingId,
  );
  if (!target) throw new ApiError(503, 'NATIVE_QUALITY_AUTHORITY_INVALID', 'The native Quality authority is unavailable.');
  const headers = new Headers({
    host: new URL(target.origin).host,
    origin: target.origin,
    'content-type': 'application/json',
    'idempotency-key': input.key,
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority?.revision ?? 0),
    'x-erpx-context-assignment-version': String(actor.authority?.assignmentVersion ?? 0),
  });
  if (actor.branchId) headers.set('x-erpx-workspace-branch', actor.branchId);
  if (actor.departmentId) headers.set('x-erpx-workspace-department', actor.departmentId);
  const payload = {
    id: input.id,
    key: input.key,
    branchId: input.branchId,
    warehouseId: input.warehouseId,
    productId: input.productId,
    lotId: input.lotId ?? null,
    quantityMilli: input.quantityMilli,
    reason: input.reason,
    dueDate: input.dueDate,
    sourceId: input.sourceId,
    createdBy,
    now,
  };
  let response: Response;
  try {
    response = await fetch(
      `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/quality/customer-return-hold-statement`,
      { method: 'POST', headers, body: JSON.stringify(payload), cache: 'no-store', signal: AbortSignal.timeout(config.timeoutMs) },
    );
  } catch {
    throw new ApiError(503, 'NATIVE_QUALITY_UNAVAILABLE', 'The native Quality service is unavailable.');
  }
  const body = (await response.json().catch(() => null)) as
    | { data?: CustomerReturnHoldStatements; error?: { code?: string; message?: string } }
    | null;
  if (!response.ok || !body?.data)
    throw new ApiError(
      response.status,
      body?.error?.code ?? 'NATIVE_QUALITY_FAILED',
      body?.error?.message ?? 'The Quality request failed.',
    );
  return body.data;
}
