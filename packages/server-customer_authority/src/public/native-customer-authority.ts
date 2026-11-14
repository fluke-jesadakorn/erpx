import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';

/** Strict owner transport for the native Customer Authority duplicate/identity routers. */
export type NativeCustomerAuthorityConfiguration = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
}>;

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

export function nativeCustomerAuthorityConfiguration(
  raw: NativeCustomerAuthorityConfiguration,
): NativeCustomerAuthorityConfiguration {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    throw new ApiError(503, 'NATIVE_CUSTOMER_AUTHORITY_CONFIGURATION_INVALID', 'Customer Authority transport is misconfigured.');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    reserved.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    raw.privateBaseUrl !== base.origin ||
    !Number.isInteger(raw.timeoutMs) ||
    raw.timeoutMs < 250 ||
    raw.timeoutMs > 30_000
  )
    throw new ApiError(503, 'NATIVE_CUSTOMER_AUTHORITY_CONFIGURATION_INVALID', 'Customer Authority transport is misconfigured.');
  return { privateBaseUrl: base.origin, timeoutMs: raw.timeoutMs };
}

function configuration(): NativeCustomerAuthorityConfiguration {
  const raw = process.env.ERPX_NATIVE_CUSTOMER_AUTHORITY_CONFIG;
  if (raw === undefined)
    throw new ApiError(503, 'NATIVE_CUSTOMER_AUTHORITY_UNAVAILABLE', 'The native Customer Authority service is unavailable.');
  try {
    return nativeCustomerAuthorityConfiguration(JSON.parse(raw) as NativeCustomerAuthorityConfiguration);
  } catch (error) {
    if (error instanceof ApiError) throw error;
    throw new ApiError(503, 'NATIVE_CUSTOMER_AUTHORITY_UNAVAILABLE', 'The native Customer Authority service is unavailable.');
  }
}

function forwarded(request: Request) {
  const headers = new Headers({ accept: 'application/json' });
  const cookie = request.headers.get('cookie');
  const origin = request.headers.get('origin');
  if (cookie) headers.set('cookie', cookie);
  if (origin) headers.set('origin', origin);
  return headers;
}

async function call(
  request: Request,
  holdingSlug: string,
  path: string,
  init?: { method: 'POST'; body: string; key: string },
) {
  const config = configuration();
  const headers = forwarded(request);
  if (init) {
    headers.set('content-type', 'application/json');
    headers.set('idempotency-key', init.key);
  }
  let response: Response;
  try {
    response = await fetch(`${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(holdingSlug)}${path}`, {
      method: init?.method ?? 'GET',
      headers,
      body: init?.body,
      cache: 'no-store',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch {
    throw new ApiError(503, 'NATIVE_CUSTOMER_AUTHORITY_UNAVAILABLE', 'The native Customer Authority service is unavailable.');
  }
  const payload = (await response.json().catch(() => null)) as
    | { data?: unknown; error?: { code?: string; message?: string } }
    | null;
  if (!response.ok) {
    const code = payload?.error?.code ?? 'NATIVE_CUSTOMER_AUTHORITY_FAILED';
    const message = payload?.error?.message ?? 'The Customer Authority request failed.';
    throw new ApiError(response.status, code, message);
  }
  return payload?.data;
}

export function nativeCustomerDuplicates(request: Request, holdingSlug: string, id: string | null) {
  return call(
    request,
    holdingSlug,
    id ? `/customer-duplicates?id=${encodeURIComponent(id)}` : '/customer-duplicates',
  );
}

export function nativeCustomerDuplicateCommand(
  request: Request,
  holdingSlug: string,
  body: unknown,
  key: string,
) {
  return call(request, holdingSlug, '/customer-duplicates', {
    method: 'POST',
    body: JSON.stringify(body),
    key,
  });
}

export function nativeCustomerIdentity(request: Request, holdingSlug: string, id: string | null) {
  return call(
    request,
    holdingSlug,
    id ? `/customer-identity?id=${encodeURIComponent(id)}` : '/customer-identity',
  );
}

export function nativeCustomerIdentityCommand(
  request: Request,
  holdingSlug: string,
  body: unknown,
  key: string,
) {
  return call(request, holdingSlug, '/customer-identity', {
    method: 'POST',
    body: JSON.stringify(body),
    key,
  });
}
