import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import { nativeRequest } from '@erpx/server-runtime/public/native-http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

/**
 * Strict private transport for the owner-local Finance customer master reads.
 * The axum owner executes the statements and owns every projection, page cursor
 * and response shape; this bridge emits no SQL, accepts no SQL from a caller and
 * exposes no TypeScript fallback. Missing configuration or any configured
 * failure fails closed with an explicit 503.
 */
type Actor = Pick<WorkContext, 'holdingId' | 'holdingSlug' | 'companyId' | 'branchId'>;
type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
type Settings = Readonly<{ base: string; timeoutMs: number; holdings: readonly Holding[] }>;
type Args = Readonly<{
  id?: string;
  currency?: string;
  search?: string;
  status?: string;
  cursor?: string;
  version?: number | null;
}>;

export const FINANCE_LANE5_RD_PRICING_CUSTOMERS = 'pricing_customers';
export const FINANCE_LANE5_RD_PRICING_CUSTOMER_VALID = 'pricing_customer_valid';
export const FINANCE_LANE5_RD_CUSTOMER_PROFILE = 'customer_profile';
export const FINANCE_LANE5_RD_CUSTOMER_DIRECTORY = 'customer_directory';
export const FINANCE_LANE5_RD_NOTE_CUSTOMER = 'note_customer';

const ENV = 'ERPX_NATIVE_FINANCE_CUSTOMER_READ_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

const unavailable = (code = 'NATIVE_FINANCE_CUSTOMER_READ_UNAVAILABLE'): never => {
  throw new ApiError(503, code, 'The native Finance customer read service is unavailable.');
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 8192): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;

function configuration(): Settings {
  const raw = process.env[ENV];
  if (!raw) return unavailable('NATIVE_FINANCE_CUSTOMER_READ_CONFIGURATION_REQUIRED');
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable('NATIVE_FINANCE_CUSTOMER_READ_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl, 512) ||
    typeof value.timeoutMs !== 'number' ||
    !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailable('NATIVE_FINANCE_CUSTOMER_READ_CONFIGURATION_INVALID');
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return unavailable('NATIVE_FINANCE_CUSTOMER_READ_CONFIGURATION_INVALID');
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
    base.origin !== value.privateBaseUrl
  )
    return unavailable('NATIVE_FINANCE_CUSTOMER_READ_CONFIGURATION_INVALID');
  const holdings: Holding[] = [];
  for (const item of value.holdings) {
    if (
      !record(item) ||
      !text(item.holdingId, 200) ||
      !text(item.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(String(item.holdingSlug)) ||
      !text(item.host, 512) ||
      item.host === '' ||
      (item.companyId !== undefined && !text(item.companyId, 200))
    )
      return unavailable('NATIVE_FINANCE_CUSTOMER_READ_CONFIGURATION_INVALID');
    let origin: URL;
    try {
      if (!text(item.origin, 512)) return unavailable('NATIVE_FINANCE_CUSTOMER_READ_CONFIGURATION_INVALID');
      origin = new URL(item.origin);
    } catch {
      return unavailable('NATIVE_FINANCE_CUSTOMER_READ_CONFIGURATION_INVALID');
    }
    // The configured Host must be the exact origin Host: a config cannot
    // redirect a Holding's request to another registered target.
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password ||
      item.host !== origin.host
    )
      return unavailable('NATIVE_FINANCE_CUSTOMER_READ_CONFIGURATION_INVALID');
    holdings.push({
      origin: origin.origin,
      host: String(item.host),
      holdingSlug: String(item.holdingSlug),
      holdingId: String(item.holdingId),
      ...(item.companyId !== undefined ? { companyId: String(item.companyId) } : {}),
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

function validValue(kind: string, value: unknown): boolean {
  const row = (v: unknown) => record(v) && text(v.id, 200);
  switch (kind) {
    case FINANCE_LANE5_RD_PRICING_CUSTOMERS:
      return Array.isArray(value) && value.every((v) => row(v) && text((v as Record<string, unknown>).code) && text((v as Record<string, unknown>).name) && text((v as Record<string, unknown>).currency, 16));
    case FINANCE_LANE5_RD_PRICING_CUSTOMER_VALID:
      return typeof value === 'boolean';
    case FINANCE_LANE5_RD_CUSTOMER_PROFILE:
      return value === null || row(value);
    case FINANCE_LANE5_RD_CUSTOMER_DIRECTORY:
      return (
        record(value) &&
        Array.isArray(value.items) &&
        value.items.every((v) => record(v) && text(v.id, 200) && text(v.page_key)) &&
        (value.nextCursor === null || text(value.nextCursor, 8192)) &&
        (value.previousCursor === null || text(value.previousCursor, 8192)) &&
        typeof value.hasMore === 'boolean'
      );
    case FINANCE_LANE5_RD_NOTE_CUSTOMER:
      return (
        value === null ||
        (row(value) &&
          text((value as Record<string, unknown>).name) &&
          text((value as Record<string, unknown>).code) &&
          typeof (value as Record<string, unknown>).version === 'number')
      );
    default:
      return false;
  }
}

/** Execute one registered Finance customer read on the native owner. */
export async function nativeCustomerRead<T>(
  kind: string,
  actor: Actor,
  args: Args = {},
): Promise<T> {
  const config = configuration();
  // Never fall back to the first configured Holding: only the exact
  // (holdingId, holdingSlug) from the caller's WorkContext is admitted.
  const target = config.holdings.find(
    (holding) =>
      holding.holdingId === actor.holdingId && holding.holdingSlug === actor.holdingSlug,
  );
  if (!target) throw new ApiError(409, 'WORK_CONTEXT_CHANGED', 'The selected Holding is unavailable.');
  if (!actor.companyId || !text(actor.companyId, 200)) return unavailable('FINANCE_CUSTOMER_COMPANY_INVALID');
  if (target.companyId !== undefined && target.companyId !== actor.companyId)
    throw new ApiError(409, 'WORK_CONTEXT_CHANGED', 'The selected Holding is unavailable.');
  if (actor.branchId && !text(actor.branchId, 200)) return unavailable('FINANCE_CUSTOMER_SCOPE_INVALID');

  const headers = new Headers({
    host: target.host,
    origin: target.origin,
    accept: 'application/json',
    'content-type': 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-company': actor.companyId,
  });
  if (actor.branchId) headers.set('x-erpx-context-branch', actor.branchId);
  const body = JSON.stringify({
    kind,
    id: args.id ?? null,
    currency: args.currency ?? null,
    search: args.search ?? null,
    status: args.status ?? null,
    cursor: args.cursor ?? null,
    version: args.version ?? null,
  });
  if (new TextEncoder().encode(body).length > 16 * 1024)
    throw new ApiError(422, 'FINANCE_CUSTOMER_READ_INPUT_INVALID', 'The read request is too large.');

  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/finance/customer-read`,
    config.base,
  );
  let response: Response;
  try {
    response = await nativeRequest(
      url,
      {
        method: 'POST',
        headers: Object.fromEntries(headers),
        body,
        signal: AbortSignal.timeout(config.timeoutMs),
      },
      config.timeoutMs,
    );
  } catch {
    return unavailable();
  }
  if (response.status !== 200) {
    const payload = await response.json().catch((): unknown => null);
    const error = record(payload) && record(payload.error) ? payload.error : undefined;
    if (response.status >= 400 && response.status < 500)
      throw new ApiError(
        response.status,
        text(error?.code, 100) ? String(error!.code) : 'NATIVE_FINANCE_CUSTOMER_READ_FAILED',
        text(error?.message, 300) ? String(error!.message) : 'The native Finance customer read was rejected.',
      );
    return unavailable();
  }
  const payload = await response.json().catch((): unknown => null);
  if (
    !record(payload) ||
    payload.kind !== kind ||
    !validValue(kind, payload.value)
  )
    return unavailable('NATIVE_FINANCE_CUSTOMER_READ_RESPONSE_INVALID');
  return payload.value as T;
}

/** True when the native Finance customer read consumer switch is configured. */
export function nativeCustomerReadConfigured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}
