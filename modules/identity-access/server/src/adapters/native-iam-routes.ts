import 'server-only';
import { canonicalDestinationLinks } from '@erpx/work-projection-ui/lib/destination-links';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import { directHttp } from '../public/native-iam-assignments';

/**
 * Native transport for the three shared Holding routes: the IAM catch-all,
 * `assignments/active` and `companies/active`.
 *
 * The Axum owner router executes every operation and owns all authorization,
 * context derivation and writes. This module only transports the request,
 * re-applies the gateway-owned canonical link normalization and forwards the
 * owner's cookie header actions. It sends no SQL and has no TypeScript fallback.
 */
export type NativeIamRoutesConfig = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
}>;

export type NativeIamRoutesTransport = (
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
) => Promise<Response>;

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const fail = (status: number, code: string, message: string): never => {
  throw new ApiError(status, code, message);
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, maximum = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= maximum;
const identifier = (value: unknown): value is string =>
  text(value, 200) && value.length > 0 && !/[\u0000-\u001f\u007f]/.test(value);
const integer = (value: unknown, minimum = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= minimum;

export function nativeIamRoutesAvailable(): boolean {
  return process.env.ERPX_NATIVE_IAM_ROUTES_CONFIG !== undefined;
}

export function nativeIamRoutesConfiguration(raw: string): NativeIamRoutesConfig {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail(503, 'NATIVE_IAM_ROUTES_CONFIGURATION_INVALID', 'The native IAM route transport is misconfigured.');
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs, 250) ||
    (value.timeoutMs as number) > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return fail(503, 'NATIVE_IAM_ROUTES_CONFIGURATION_INVALID', 'The native IAM route transport is misconfigured.');
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return fail(503, 'NATIVE_IAM_ROUTES_CONFIGURATION_INVALID', 'The native IAM route transport is misconfigured.');
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
    value.privateBaseUrl !== base.origin
  )
    return fail(503, 'NATIVE_IAM_ROUTES_CONFIGURATION_INVALID', 'The native IAM route transport is misconfigured.');
  const hosts = new Set<string>();
  for (const holding of value.holdings as unknown[]) {
    if (
      !record(holding) ||
      !text(holding.origin) ||
      !identifier(holding.holdingId) ||
      !text(holding.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug)
    )
      return fail(503, 'NATIVE_IAM_ROUTES_CONFIGURATION_INVALID', 'The native IAM route transport is misconfigured.');
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return fail(503, 'NATIVE_IAM_ROUTES_CONFIGURATION_INVALID', 'The native IAM route transport is misconfigured.');
    }
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      origin.username ||
      origin.password ||
      hosts.has(origin.host)
    )
      return fail(503, 'NATIVE_IAM_ROUTES_CONFIGURATION_INVALID', 'The native IAM route transport is misconfigured.');
    hosts.add(origin.host);
  }
  return {
    privateBaseUrl: base.origin,
    timeoutMs: value.timeoutMs,
    holdings: (value.holdings as NativeIamRoutesConfig['holdings']).map((holding) => ({
      origin: new URL(holding.origin).origin,
      holdingSlug: holding.holdingSlug,
      holdingId: holding.holdingId,
    })),
  };
}

const contextHeaders = [
  'x-erpx-context-company',
  'x-erpx-context-assignment',
  'x-erpx-context-revision',
  'x-erpx-context-assignment-version',
  'x-erpx-workspace-branch',
  'x-erpx-workspace-department',
  'x-erpx-platform-assignment',
] as const;

/**
 * Execute one shared route in Rust and return the gateway response. The caller
 * passes the unchanged incoming request so the owner can bind the Account
 * proof, Origin and private context headers itself.
 */
export async function nativeIamRouteResponse(
  request: Request,
  holdingSlug: string,
  path: string[],
  body: string | undefined,
  transport: NativeIamRoutesTransport = directHttp,
): Promise<Response> {
  const raw = process.env.ERPX_NATIVE_IAM_ROUTES_CONFIG;
  if (raw === undefined)
    return fail(503, 'NATIVE_IAM_REQUIRED', 'This IAM route requires the native owner service.');
  const config = nativeIamRoutesConfiguration(raw);
  const host = request.headers.get('host');
  const holding = host ? [...config.holdings].find((item) => new URL(item.origin).host === host) : undefined;
  if (!holding || holding.holdingSlug !== holdingSlug)
    return fail(503, 'NATIVE_IAM_ROUTE_AUTHORITY_INVALID', 'The native IAM route has no matching registered Holding.');
  const method: 'GET' | 'POST' = body === undefined ? 'GET' : 'POST';
  const headers = new Headers({
    host: host ?? '',
    accept: 'application/json',
    'cache-control': 'no-cache',
  });
  const cookie = request.headers.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return fail(422, 'NATIVE_IAM_ROUTE_REQUEST_INVALID', 'The request cookie is too large.');
    headers.set('cookie', cookie);
  }
  const origin = request.headers.get('origin');
  if (origin) headers.set('origin', origin);
  const activeAssignment = path.length === 2 && path[0] === 'assignments' && path[1] === 'active';
  const activeCompany = path.length === 2 && path[0] === 'companies' && path[1] === 'active';
  if (method === 'POST') {
    const key = request.headers.get('idempotency-key');
    // The active assignment read is a CAS selection; it has no durable command key.
    if (!activeAssignment && (!key || !/^[A-Za-z0-9:_.-]{8,180}$/.test(key)))
      return fail(422, 'COMMAND_KEY_REQUIRED', 'A valid Idempotency-Key is required.');
    if (key) headers.set('idempotency-key', key);
    headers.set('content-type', 'application/json');
  }
  for (const name of contextHeaders) {
    const value = request.headers.get(name);
    if (value) headers.set(name, value);
  }
  const suffix = path.length ? `/${path.map(encodeURIComponent).join('/')}` : '';
  const search = method === 'GET' ? new URL(request.url).search : '';
  // The owner mounts the two active routes outside the IAM catch-all; sending
  // them under /iam would resolve an unknown operation and fail closed.
  const ownerPath = activeAssignment
    ? '/assignments/active'
    : activeCompany
      ? '/companies/active'
      : `/iam${suffix}`;
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holdingSlug)}${ownerPath}${search}`,
    config.privateBaseUrl,
  );
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  let response: Response;
  try {
    response = await transport(method, url, headers, body, controller.signal);
  } catch {
    return fail(503, 'NATIVE_IAM_UNAVAILABLE', 'The native IAM service is unavailable.');
  } finally {
    clearTimeout(timer);
  }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    return fail(503, 'NATIVE_IAM_UNAVAILABLE', 'The native IAM request failed.');
  }
  if ((response.headers.get('content-type') ?? '').split(';')[0]?.trim() !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return fail(502, 'NATIVE_IAM_ROUTE_RESPONSE_INVALID', 'The native IAM reply was invalid.');
  }
  let payload: unknown;
  try {
    payload = await response.json();
  } catch {
    return fail(502, 'NATIVE_IAM_ROUTE_RESPONSE_INVALID', 'The native IAM reply was invalid.');
  }
  if (!record(payload) || !record(payload.data))
    return fail(502, 'NATIVE_IAM_ROUTE_RESPONSE_INVALID', 'The native IAM reply was invalid.');
  const outbound = new Headers({ 'Cache-Control': 'private, no-store' });
  const withCookies = response.headers as Headers & { getSetCookie?: () => string[] };
  const cookies =
    typeof withCookies.getSetCookie === 'function'
      ? withCookies.getSetCookie()
      : (response.headers.get('set-cookie') ?? '').split(',').map((item) => item.trim()).filter(Boolean);
  for (const cookie of cookies) outbound.append('set-cookie', cookie);
  return new Response(
    JSON.stringify({ data: canonicalDestinationLinks(payload.data) }),
    { status: 200, headers: outbound },
  );
}
