import 'server-only';
import { ApiError } from '@erpx/platform-runtime-server/public/http';

/**
 * Fail-closed native transport for the PMS refund request-context read.
 * No SQL is authored here; the Rust owner owns every statement. A missing or
 * invalid configuration, an unknown Holding or any unexpected payload fails
 * closed with an explicit error, never a TS SQL fallback and never null
 * masquerading as a found request (except the native 404, which maps to the
 * retained "no such request" null).
 */
export const REFUND_CONTEXT_CONFIG_KEY = 'ERPX_NATIVE_PMS_REFUND_CONTEXT_CONFIG';
const BODY_LIMIT = 2 * 1024 * 1024;
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

export type RefundContextSettings = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
}>;

export type RefundRequestContext = {
  id: string;
  payment_id: string;
  amount_minor: number;
  allocations_json: string;
  reason: string;
  requested_by: string;
  approved_by: string | null;
  provider_id: string | null;
  state: string;
  generation: number;
  legacy_review?: boolean;
  property_id: string;
  account_id: string;
  livemode: boolean;
};

const fail = (status = 503, code = 'NATIVE_PMS_REFUND_CONTEXT_UNAVAILABLE'): never => {
  throw new ApiError(
    status,
    code,
    'The native PMS refund request context is unavailable.',
  );
};

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

const text = (value: unknown): value is string =>
  typeof value === 'string' && value.length > 0 && value.length <= 5000;

const identifier = (value: unknown): value is string =>
  typeof value === 'string' &&
  value.length > 0 &&
  value.length <= 200 &&
  // eslint-disable-next-line no-control-regex
  !/[\u0000-\u001f/?#\\]/.test(value);

function settings(): RefundContextSettings {
  const raw = process.env[REFUND_CONTEXT_CONFIG_KEY];
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
  if (!Array.isArray(value.holdings) || value.holdings.length === 0) return fail();
  const holdings: Array<{ origin: string; holdingSlug: string; holdingId: string }> = [];
  for (const entry of value.holdings) {
    if (
      !record(entry) ||
      typeof entry.origin !== 'string' ||
      typeof entry.holdingSlug !== 'string' ||
      typeof entry.holdingId !== 'string' ||
      !identifier(entry.holdingId) ||
      entry.holdingSlug.length === 0 ||
      entry.holdingSlug.length > 128 ||
      // eslint-disable-next-line no-control-regex
      /[\u0000-\u001f/?#\\]/.test(entry.holdingSlug)
    )
      return fail();
    let origin: URL;
    try {
      origin = new URL(entry.origin);
    } catch {
      return fail();
    }
    if (origin.protocol !== 'https:' || origin.username || origin.password) return fail();
    holdings.push({ origin: entry.origin, holdingSlug: entry.holdingSlug, holdingId: entry.holdingId });
  }
  return { privateBaseUrl: value.privateBaseUrl, timeoutMs: value.timeoutMs as number, holdings };
}

async function readBody(body: ReadableStream<Uint8Array>): Promise<string> {
  const reader = body.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.byteLength;
      if (bytes > BODY_LIMIT) {
        await reader.cancel();
        return fail(502);
      }
      chunks.push(next.value);
    }
  } finally {
    reader.releaseLock();
  }
  const data = new Uint8Array(bytes);
  let offset = 0;
  for (const chunk of chunks) {
    data.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return new TextDecoder('utf-8', { fatal: true }).decode(data);
}

function validContext(value: unknown): value is RefundRequestContext {
  if (!record(value)) return false;
  const {
    id,
    payment_id: paymentId,
    amount_minor: amountMinor,
    allocations_json: allocationsJson,
    reason,
    requested_by: requestedBy,
    approved_by: approvedBy,
    provider_id: providerId,
    state,
    generation,
    legacy_review: legacyReview,
    property_id: propertyId,
    account_id: accountId,
    livemode,
  } = value as Record<string, unknown>;
  return (
    identifier(id) &&
    identifier(paymentId) &&
    typeof amountMinor === 'number' &&
    Number.isSafeInteger(amountMinor) &&
    typeof allocationsJson === 'string' &&
    text(reason) &&
    text(requestedBy) &&
    (approvedBy === null || text(approvedBy)) &&
    (providerId === null || text(providerId)) &&
    text(state) &&
    typeof generation === 'number' &&
    Number.isSafeInteger(generation) &&
    (legacyReview === undefined || typeof legacyReview === 'boolean') &&
    identifier(propertyId) &&
    identifier(accountId) &&
    typeof livemode === 'boolean'
  );
}

/**
 * Read one refund request joined with its payment's property, account and
 * livemode facts from the native PMS owner. Returns null when the native
 * owner reports no such request for the session Company (the retained
 * "no such request" contract); every other failure throws fail-closed.
 */
export async function refundRequestContext(
  incoming: Pick<Headers, 'get'>,
  holdingId: string,
  requestId: string,
): Promise<RefundRequestContext | null> {
  if (!identifier(holdingId) || !identifier(requestId)) return fail(422);
  const config = settings();
  const holding = config.holdings.find((item) => item.holdingId === holdingId);
  if (!holding) return fail(503);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/pms/booking-refunds/context?request=${encodeURIComponent(requestId)}`,
    config.privateBaseUrl,
  );
  const cookie = incoming.get('cookie');
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'GET',
      headers: {
        host: new URL(holding.origin).host,
        origin: holding.origin,
        accept: 'application/json',
        ...(cookie ? { cookie } : {}),
      },
      cache: 'no-store',
      redirect: 'error',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch {
    return fail(503);
  }
  if (response.status === 404) {
    // Null only for the native record-absent signal. Any other 404 (unknown
    // target, unmounted route, gateway page) is a configuration or routing
    // failure and fails closed.
    try {
      const text = await response.text();
      const body = JSON.parse(text) as unknown;
      if (
        record(body) &&
        record(body.error) &&
        body.error.code === 'PMS_REFUND_CONTEXT_ABSENT'
      )
        return null;
    } catch {
      // Fall through to fail-closed below.
    }
    await response.body?.cancel().catch(() => undefined);
    return fail(503);
  }
  if (response.redirected || response.status !== 200 || !response.body) {
    await response.body?.cancel().catch(() => undefined);
    return fail(response.status);
  }
  if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    await response.body.cancel().catch(() => undefined);
    return fail(502);
  }
  let payload: unknown;
  try {
    payload = JSON.parse(await readBody(response.body));
  } catch {
    return fail(502);
  }
  if (!record(payload) || !('data' in payload)) return fail(502);
  if (!validContext(payload.data)) return fail(502);
  return payload.data;
}
