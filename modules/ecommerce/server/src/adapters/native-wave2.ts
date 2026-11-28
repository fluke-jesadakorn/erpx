import 'server-only';

/**
 * Native Ecommerce wave2 owner-write transport. A thin private fetch client to
 * the single owner axum command router: it carries the exact registered Holding
 * Host/Origin, one Idempotency-Key and the caller identity, and validates the
 * exact acknowledgement. `values` is a positional data array (never SQL text).
 * No SQL, no fallback writer: missing or malformed config fails closed.
 */
type NativeConfig = Readonly<{
  base: string;
  host: string;
  holdingSlug: string;
  origin: string;
  timeoutMs: number;
}>;

function config(): NativeConfig {
  const raw = process.env.ERPX_NATIVE_ECOMMERCE_WAVE2_CONFIG;
  if (raw === undefined)
    throw new NativeEcommerceWave2Error(
      'ECOMMERCE_WAVE2_UNAVAILABLE',
      503,
      'The native E-commerce service is not configured.',
    );
  let value: {
    privateBaseUrl?: unknown;
    host?: unknown;
    holdingSlug?: unknown;
    origin?: unknown;
    timeoutMs?: unknown;
  };
  try {
    value = JSON.parse(raw) as typeof value;
  } catch {
    throw new NativeEcommerceWave2Error(
      'ECOMMERCE_WAVE2_CONFIGURATION_INVALID',
      503,
      'The native E-commerce transport is misconfigured.',
    );
  }
  let base: URL, origin: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
    origin = new URL(String(value.origin));
  } catch {
    throw new NativeEcommerceWave2Error(
      'ECOMMERCE_WAVE2_CONFIGURATION_INVALID',
      503,
      'The native E-commerce transport is misconfigured.',
    );
  }
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    origin.protocol !== 'https:' ||
    typeof value.host !== 'string' ||
    value.host !== origin.host ||
    typeof value.holdingSlug !== 'string' ||
    !/^[a-z0-9_-]{1,128}$/.test(value.holdingSlug) ||
    !Number.isInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30_000
  )
    throw new NativeEcommerceWave2Error(
      'ECOMMERCE_WAVE2_CONFIGURATION_INVALID',
      503,
      'The native E-commerce transport is misconfigured.',
    );
  return {
    base: base.origin,
    host: value.host,
    holdingSlug: value.holdingSlug,
    origin: origin.origin,
    timeoutMs: value.timeoutMs as number,
  };
}

export class NativeEcommerceWave2Error extends Error {
  readonly code: string;
  readonly status: number;
  constructor(code: string, status: number, message: string) {
    super(message);
    this.name = 'NativeEcommerceWave2Error';
    this.code = code;
    this.status = status;
  }
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

export type EcommerceOwnerWriteKind =
  | 'channel.create'
  | 'listing.create'
  | 'cart.create'
  | 'cart.add-line'
  | 'cart.version-bump'
  | 'cart.abandon'
  | 'checkout.create'
  | 'checkout.confirm'
  | 'checkout.fulfillment'
  | 'checkout.return'
  | 'channel.review'
  | 'listing.review';

/** Commit one owner write through the native axum command router. */
export async function nativeEcommerceOwnerWrite(
  kind: EcommerceOwnerWriteKind,
  values: readonly unknown[],
  recordId: string,
  actorId: string,
  idempotencyKey: string,
): Promise<{ id: string; kind: string }> {
  const cfg = config();
  const headers = new Headers({
    host: cfg.host,
    origin: cfg.origin,
    accept: 'application/json',
    'content-type': 'application/json',
    'idempotency-key': idempotencyKey,
    'x-erpx-context-user': actorId,
  });
  let response: Response;
  try {
    response = await fetch(
      `${cfg.base}/api/v1/holdings/${encodeURIComponent(cfg.holdingSlug)}/ecommerce/checkout-wave2`,
      {
        method: 'POST',
        headers,
        body: JSON.stringify({ kind, values, recordId }),
        cache: 'no-store',
        signal: AbortSignal.timeout(cfg.timeoutMs),
      },
    );
  } catch {
    throw new NativeEcommerceWave2Error(
      'ECOMMERCE_WAVE2_UNAVAILABLE',
      503,
      'The native E-commerce service is unavailable.',
    );
  }
  const value: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const error = isRecord(value) && isRecord(value.error) ? value.error : {};
    throw new NativeEcommerceWave2Error(
      typeof error.code === 'string' ? error.code : 'ECOMMERCE_OWNER_WRITE_FAILED',
      response.status,
      typeof error.message === 'string' ? error.message : 'The E-commerce write could not be completed.',
    );
  }
  if (
    !isRecord(value) ||
    !isRecord(value.data) ||
    typeof value.data.id !== 'string' ||
    typeof value.data.kind !== 'string'
  )
    throw new NativeEcommerceWave2Error(
      'ECOMMERCE_WAVE2_RESPONSE_INVALID',
      503,
      'The native E-commerce acknowledgement was invalid.',
    );
  return { id: value.data.id, kind: value.data.kind };
}
