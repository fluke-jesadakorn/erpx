import 'server-only';
import { randomUUID } from 'node:crypto';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import type { Snapshot } from '@erpx/pms-server/public/rules/pms';

/**
 * Fail-closed native transport for booking-refund commands. No SQL or refund
 * orchestration is authored here: the native PMS owner runs the committed
 * refund producer and posts its declarative Finance postings/documents after
 * the owner unit commits. A missing/misconfigured native service throws an
 * explicit error; there is never a TypeScript writer fallback.
 */
const CONFIG_KEY = 'ERPX_NATIVE_PMS_CONFIG';
const BODY_LIMIT = 256 * 1024;

type Settings = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: ReadonlyArray<Readonly<{ origin: string; holdingSlug: string; holdingId: string }>>;
}>;

function config(): Settings {
  const raw = process.env[CONFIG_KEY];
  if (!raw) throw new ApiError(503, 'NATIVE_PMS_UNAVAILABLE', 'The native Hotel service is unavailable.');
  try {
    const parsed = JSON.parse(raw) as Settings;
    if (
      !parsed.privateBaseUrl ||
      !Number.isSafeInteger(parsed.timeoutMs) ||
      !Array.isArray(parsed.holdings) ||
      parsed.holdings.length === 0
    )
      throw new Error('invalid');
    return parsed;
  } catch {
    throw new ApiError(503, 'NATIVE_PMS_CONFIGURATION_INVALID', 'Native Hotel transport is misconfigured.');
  }
}

const fail = (status: number, code: string, message: string): never => {
  throw new ApiError(status, code, message);
};

/** Resolve the exact registered Holding for an actor, or the default one for a
 * system/jobs caller that carries no WorkContext. */
function target(settings: Settings, actor?: WorkContext) {
  if (actor) {
    const match = settings.holdings.find(
      h => h.holdingSlug === actor.holdingSlug && h.holdingId === actor.holdingId,
    );
    if (!match) return fail(503, 'NATIVE_PMS_AUTHORITY_INVALID', 'The native Hotel authority is unavailable.');
    return match;
  }
  return settings.holdings[0];
}

async function command(
  action: string,
  body: Record<string, unknown>,
  actor: WorkContext | undefined,
  incoming?: Pick<Headers, 'get'>,
): Promise<any> {
  const settings = config();
  const holding = target(settings, actor);
  const url = `${settings.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/pms/booking-refunds`;
  const headers = new Headers({
    'content-type': 'application/json',
    accept: 'application/json',
    host: new URL(holding.origin).host,
    origin: holding.origin,
    'x-erpx-context-company': actor?.companyId ?? 'pms-default',
    'idempotency-key': `${action}:${String(body.id ?? body.requestId ?? body.paymentId ?? '')}`.slice(0, 160),
  });
  if (actor) {
    headers.set('x-erpx-actor-user', actor.userId);
    headers.set('x-erpx-context-assignment', actor.assignmentId);
    headers.set('x-erpx-authority-json', JSON.stringify({
      revision: actor.authority?.revision ?? 0,
      assignmentVersion: actor.authority?.assignmentVersion ?? 0,
    }));
    if (actor.branchId) headers.set('x-erpx-workspace-branch', actor.branchId);
    if (actor.departmentId) headers.set('x-erpx-workspace-department', actor.departmentId);
  }
  const cookie = incoming?.get('cookie');
  if (cookie) headers.set('cookie', cookie);
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'POST',
      headers,
      body: JSON.stringify({ action, ...body }),
      cache: 'no-store',
      redirect: 'error',
      signal: AbortSignal.timeout(settings.timeoutMs),
    });
  } catch {
    return fail(503, 'NATIVE_PMS_UNAVAILABLE', 'The native Hotel service is unavailable.');
  }
  const text = await response.text();
  let payload: any;
  try {
    payload = text.length <= BODY_LIMIT ? JSON.parse(text) : null;
  } catch {
    payload = null;
  }
  if (!response.ok || !payload?.data)
    return fail(
      response.status,
      payload?.error?.code ?? 'NATIVE_PMS_REFUND_FAILED',
      payload?.error?.message ?? 'The native booking refund failed.',
    );
  return payload.data;
}

export function requestBookingRefund(
  _tx: unknown,
  s: Snapshot,
  paymentId: string,
  wanted: Array<{ stayId: string; amount: number }>,
  reason: string,
  requestedBy: string,
  now = Date.now(),
  requestId = randomUUID(),
): Promise<any> {
  return command('request', {
    requestId,
    propertyId: s.property.id,
    paymentId,
    wanted,
    reason,
    requestedBy,
    now,
  }, undefined);
}

export function approveBookingRefund(_db: unknown, actor: WorkContext, id: string): Promise<any> {
  return command('approve', { id }, actor);
}

export function bookingRefundsForFinance(actor: WorkContext): Promise<any> {
  return command('finance', {}, actor);
}

export function recoverBookingReceipt(
  _db: unknown,
  actor: WorkContext,
  requestId: string,
  reason: string,
): Promise<any> {
  return command('recover-receipt', { id: requestId, reason }, actor);
}

export function processBookingRefund(_db: unknown, id: string, now = Date.now()): Promise<void> {
  return command('settle', { id, now }, undefined).then(() => undefined);
}

export function retryBookingRefund(
  _db: unknown,
  actor: WorkContext,
  id: string,
  reason: string,
  incoming: Pick<Headers, 'get'>,
): Promise<any> {
  return command('retry', { id, reason }, actor, incoming);
}
