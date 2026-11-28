import 'server-only';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import { nativeRequest } from '@erpx/platform-runtime-server/public/native-http';

/**
 * Fail-closed transport for the mounted PMS PromptPay payment-status read.
 * The Rust owner owns the exact query, the cashier/purchase gates, the property
 * scope and the provider reconcile; this bridge sends only the bounded `id`
 * and the authenticated session context, then returns the owner projection.
 */
export const PAYMENT_STATUS_CONFIG_KEY = 'ERPX_NATIVE_PMS_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

type Holding = Readonly<{ origin: string; holdingSlug: string; holdingId: string; companyId: string }>;
export type NativePaymentStatusActor = Readonly<{
  holdingId: string;
  companyId: string;
  assignmentId: string;
  authority: Readonly<{ revision: number; assignmentVersion: number }>;
  scope: Readonly<
    | { kind: 'holding' }
    | { kind: 'branch'; branchSlug: string }
    | { kind: 'department'; departmentSlug: string }
  >;
}>;
type Settings = Readonly<{ privateBaseUrl: string; timeoutMs: number; holdings: readonly Holding[] }>;

export type NativePaymentStatus = Readonly<{
  id: string;
  state: string;
  kind: string;
  amountMinor: number;
  qrData: string | null;
  expiresAt: number;
  entryId: string | null;
  refundState: string | null;
  payerEmailProvided: boolean;
}>;

const fail = (status = 503, code = 'NATIVE_PMS_PAYMENT_STATUS_UNAVAILABLE'): never => {
  throw new ApiError(status, code, 'The native PMS payment status is unavailable.');
};

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

const text = (value: unknown): value is string =>
  typeof value === 'string' && value.length > 0 && value.length <= 5000;

function settings(): Settings {
  const raw = process.env[PAYMENT_STATUS_CONFIG_KEY];
  if (raw === undefined) return fail();
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail();
  }
  if (
    !record(value) ||
    typeof value.privateBaseUrl !== 'string' ||
    typeof value.timeoutMs !== 'number' ||
    !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30000 ||
    !Array.isArray(value.holdings)
  )
    return fail();
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return fail();
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    value.privateBaseUrl !== base.origin
  )
    return fail();
  const holdings: Holding[] = [];
  for (const entry of value.holdings) {
    if (
      !record(entry) ||
      typeof entry.origin !== 'string' ||
      typeof entry.holdingSlug !== 'string' ||
      typeof entry.holdingId !== 'string' ||
      typeof entry.companyId !== 'string'
    )
      return fail();
    let origin: URL;
    try {
      origin = new URL(entry.origin);
    } catch {
      return fail();
    }
    if (origin.protocol !== 'https:' || origin.username || origin.password) return fail();
    holdings.push({
      origin: entry.origin,
      holdingSlug: entry.holdingSlug,
      holdingId: entry.holdingId,
      companyId: entry.companyId,
    });
  }
  return { privateBaseUrl: value.privateBaseUrl, timeoutMs: value.timeoutMs, holdings };
}

function view(value: unknown): NativePaymentStatus {
  if (!record(value)) return fail(502, 'NATIVE_PMS_PAYMENT_STATUS_RESPONSE_INVALID');
  const { id, state, kind, amount_minor: amountMinor, qr_data: qrData, expires_at: expiresAt } = value;
  const { entry_id: entryId, refund_state: refundState, payer_email_provided: payerEmailProvided } = value;
  if (
    typeof id !== 'string' ||
    typeof state !== 'string' ||
    typeof kind !== 'string' ||
    typeof amountMinor !== 'number' ||
    !Number.isSafeInteger(amountMinor) ||
    !(qrData === null || typeof qrData === 'string') ||
    typeof expiresAt !== 'number' ||
    !Number.isSafeInteger(expiresAt) ||
    !(entryId === null || typeof entryId === 'string') ||
    !(refundState === null || typeof refundState === 'string') ||
    typeof payerEmailProvided !== 'boolean'
  )
    return fail(502, 'NATIVE_PMS_PAYMENT_STATUS_RESPONSE_INVALID');
  return {
    id,
    state,
    kind,
    amountMinor,
    qrData,
    expiresAt,
    entryId,
    refundState,
    payerEmailProvided,
  };
}

/** Read one PromptPay counter payment for the authenticated Company. */
export async function nativePaymentStatus(
  incoming: Pick<Headers, 'get'>,
  actor: NativePaymentStatusActor,
  id: string,
): Promise<NativePaymentStatus> {
  if (!text(actor.holdingId) || !text(id)) return fail(422, 'PMS_PAYMENT_STATUS_REQUEST_INVALID');
  const config = settings();
  const holding = config.holdings.find((item) => item.holdingId === actor.holdingId);
  if (!holding || actor.companyId !== holding.companyId) return fail();
  const cookie = incoming.get('cookie');
  const headers: Record<string, string> = {
    host: new URL(holding.origin).host,
    origin: holding.origin,
    accept: 'application/json',
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority.revision),
    'x-erpx-context-assignment-version': String(actor.authority.assignmentVersion),
    ...(cookie ? { cookie } : {}),
  };
  if (actor.scope.kind === 'branch') headers['x-erpx-workspace-branch'] = actor.scope.branchSlug;
  if (actor.scope.kind === 'department')
    headers['x-erpx-workspace-department'] = actor.scope.departmentSlug;
  let response: Response;
  try {
    response = await nativeRequest(
      new URL(
        `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/pms/stripe-payments?id=${encodeURIComponent(id)}`,
        config.privateBaseUrl,
      ),
      { method: 'GET', headers },
      config.timeoutMs,
    );
  } catch {
    return fail();
  }
  const payload: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const error = record(payload) && record(payload.error) ? payload.error : undefined;
    throw new ApiError(
      response.status,
      text(error?.code) ? (error!.code as string) : 'NATIVE_PMS_PAYMENT_STATUS_FAILED',
      text(error?.message) ? (error!.message as string) : 'The native PMS payment status was rejected.',
    );
  }
  if (!record(payload) || !('data' in payload))
    return fail(502, 'NATIVE_PMS_PAYMENT_STATUS_RESPONSE_INVALID');
  return view(payload.data);
}
