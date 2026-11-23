import 'server-only';

/**
 * Fail-closed transport for the native Commercial owner-read registry.
 *
 * The axum owner plans one of three exact static usage SELECTs for a closed
 * read kind. This bridge emits no SQL and accepts none from a caller: it only
 * returns the registered owner query so the caller can run it as a subquery on its
 * own already-open unit, keeping its own filters and `FOR UPDATE` inside the
 * caller's transaction. Missing configuration or any configured failure fails
 * closed with no TypeScript fallback.
 *
 * The transport reuses an existing registered Commercial/Billing owner target
 * on the private listener; `ERPX_NATIVE_COMMERCIAL_OWNER_READ_CONFIG` may supply
 * a dedicated value. Both the single-target and holdings-array config shapes
 * are accepted. Missing or malformed configuration fails closed.
 */
type Target = Readonly<{ host: string; holdingSlug: string; origin: string }>;
type Config = Readonly<{ base: string; timeoutMs: number; targets: readonly Target[] }>;

const isRecord = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === 'object' && !Array.isArray(v);

export class NativeCommercialOwnerReadError extends Error {
  readonly code: string;
  readonly status: number;
  constructor(code: string, status: number, message: string) {
    super(message);
    this.name = 'NativeCommercialOwnerReadError';
    this.code = code;
    this.status = status;
  }
}

const unavailable = (): never => {
  throw new NativeCommercialOwnerReadError('COMMERCIAL_OWNER_READ_UNAVAILABLE', 503, 'The native Commercial read service is unavailable.');
};

const invalidConfig = (): never => {
  throw new NativeCommercialOwnerReadError('COMMERCIAL_OWNER_READ_CONFIG_INVALID', 503, 'The native Commercial read transport is misconfigured.');
};

const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function config(): Config {
  const raw = process.env.ERPX_NATIVE_COMMERCIAL_OWNER_READ_CONFIG ??
    process.env.ERPX_NATIVE_COMMERCIAL_CONFIG ??
    process.env.ERPX_NATIVE_BILLING_CONFIG ??
    process.env.ERPX_NATIVE_COMMERCIAL_OWNER_WRITE_CONFIG;
  if (raw === undefined) throw unavailable();
  let value: Record<string, unknown>;
  try { value = JSON.parse(raw) as Record<string, unknown>; } catch { return invalidConfig(); }
  if (!isRecord(value) || typeof value.privateBaseUrl !== 'string') return invalidConfig();
  if (typeof value.timeoutMs !== 'number' || !Number.isSafeInteger(value.timeoutMs) || value.timeoutMs < 250 || value.timeoutMs > 30000)
    return invalidConfig();
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { return invalidConfig(); }
  const port = Number(base.port);
  if (base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port ||
      reservedPorts.has(port) || base.username || base.password || value.privateBaseUrl !== base.origin)
    return invalidConfig();

  const asTarget = (item: unknown): Target => {
    if (!isRecord(item) || typeof item.origin !== 'string' || typeof item.holdingSlug !== 'string') return invalidConfig();
    let origin: URL;
    try { origin = new URL(item.origin); } catch { return invalidConfig(); }
    if (origin.protocol !== 'https:' || item.origin !== origin.origin || origin.username || origin.password ||
        !/^[a-z0-9_-]{1,128}$/.test(item.holdingSlug))
      return invalidConfig();
    return { origin: item.origin, holdingSlug: item.holdingSlug, host: origin.host };
  };

  let targets: readonly Target[];
  if (Array.isArray(value.holdings)) {
    targets = value.holdings.map(asTarget);
  } else {
    if (typeof value.host !== 'string' || typeof value.holdingSlug !== 'string' || typeof value.origin !== 'string')
      return invalidConfig();
    const target = asTarget({ origin: value.origin, holdingSlug: value.holdingSlug });
    if (target.host !== value.host) return invalidConfig();
    targets = [target];
  }
  if (targets.length < 1 || targets.length > 64) return invalidConfig();
  return { base: base.origin, timeoutMs: value.timeoutMs, targets };
}

export type CommercialOwnerReadKind = 'usage.snapshot' | 'usage.meter' | 'usage.seat';

export const COMMERCIAL_RD_USAGE_SNAPSHOT: CommercialOwnerReadKind = 'usage.snapshot';
export const COMMERCIAL_RD_USAGE_METER: CommercialOwnerReadKind = 'usage.meter';
export const COMMERCIAL_RD_USAGE_SEAT: CommercialOwnerReadKind = 'usage.seat';

const KINDS = new Set<string>([
  COMMERCIAL_RD_USAGE_SNAPSHOT,
  COMMERCIAL_RD_USAGE_METER,
  COMMERCIAL_RD_USAGE_SEAT,
]);

/** Return the exact owner-owned query registered for `kind`. */
export async function nativeCommercialOwnerRead(kind: CommercialOwnerReadKind): Promise<string> {
  if (!KINDS.has(kind))
    throw new NativeCommercialOwnerReadError('COMMERCIAL_OWNER_READ_INVALID', 503, 'Review the Commercial owner read.');
  const cfg = config();
  const target = cfg.targets[0]!;
  let response: Response;
  try {
    response = await fetch(
      `${cfg.base}/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/commercial/owner-read`,
      {
        method: 'POST',
        headers: new Headers({
          host: target.host,
          origin: target.origin,
          accept: 'application/json',
          'content-type': 'application/json',
          'cache-control': 'no-cache',
        }),
        body: JSON.stringify({ kind }),
        cache: 'no-store',
        redirect: 'error',
        signal: AbortSignal.timeout(cfg.timeoutMs),
      },
    );
  } catch { return unavailable(); }
  const value: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const error = isRecord(value) && isRecord(value.error) ? value.error : {};
    throw new NativeCommercialOwnerReadError(
      typeof error.code === 'string' ? error.code : 'COMMERCIAL_OWNER_READ_FAILED',
      response.status,
      typeof error.message === 'string' ? error.message : 'The Commercial owner read could not be completed.',
    );
  }
  const sql = isRecord(value) && isRecord(value.data) ? value.data.sql : undefined;
  if (typeof sql !== 'string' || !sql.trimStart().toUpperCase().startsWith('SELECT'))
    throw new NativeCommercialOwnerReadError('COMMERCIAL_OWNER_READ_RESPONSE_INVALID', 503, 'The native Commercial read plan was invalid.');
  return sql;
}
