import 'server-only';
import type { CustomerPortalItem } from '../domain/portal';

type Configuration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Incoming = Pick<Headers, 'get'>;
type Send = (url: URL, init: RequestInit, signal: AbortSignal) => Promise<Response>;

/** Native customer-portal projection returned in the same shape as the owner TS application. */
export type NativeCustomerPortalProjection = {
  items: CustomerPortalItem[];
  projectionVersion: string;
};
/** Native acknowledgement receipt; field names match the owner TS command result. */
export type NativeCustomerPortalOutcome = {
  id: string;
  bindingId: string;
  orderId: string;
  action: string;
  status: string;
  createdAt: number;
};

const BODY_LIMIT = 50000;
const RESPONSE_LIMIT = 1024 * 1024;
const PROJECTION_VERSION = 'customer-order-v1';

export class NativeCustomerPortalError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number) {
    super('The native customer portal request could not be completed.');
    this.name = 'NativeCustomerPortalError';
    this.code = code;
    this.status = status;
  }
}

const fail = (code: string, status?: number): never => {
  throw new NativeCustomerPortalError(code, status);
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, maximum = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= maximum;
const integer = (value: unknown, minimum = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= minimum;

function configuration(raw: string): Configuration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail('NATIVE_CUSTOMER_PORTAL_CONFIGURATION_INVALID');
  }
  if (
    !object(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs, 250) ||
    value.timeoutMs > 30000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return fail('NATIVE_CUSTOMER_PORTAL_CONFIGURATION_INVALID');
  const base = new URL(value.privateBaseUrl);
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    [3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080].includes(port) ||
    (port >= 3200 && port <= 3299) ||
    value.privateBaseUrl !== base.origin
  )
    return fail('NATIVE_CUSTOMER_PORTAL_CONFIGURATION_INVALID');
  const hosts = new Set<string>();
  const slugs = new Set<string>();
  for (const holding of value.holdings) {
    if (
      !object(holding) ||
      !text(holding.origin) ||
      !text(holding.holdingId) ||
      !text(holding.holdingSlug, 128) ||
      !/^[a-z0-9_-]{1,128}$/.test(holding.holdingSlug)
    )
      return fail('NATIVE_CUSTOMER_PORTAL_CONFIGURATION_INVALID');
    const origin = new URL(holding.origin);
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      hosts.has(origin.host) ||
      slugs.has(holding.holdingSlug)
    )
      return fail('NATIVE_CUSTOMER_PORTAL_CONFIGURATION_INVALID');
    hosts.add(origin.host);
    slugs.add(holding.holdingSlug);
  }
  return {
    privateBaseUrl: value.privateBaseUrl,
    holdings: value.holdings as Configuration['holdings'],
    timeoutMs: value.timeoutMs,
  };
}

function target(config: Configuration, incoming: Incoming, slug: string) {
  if (!/^[a-z0-9_-]{1,128}$/.test(slug))
    return fail('NATIVE_CUSTOMER_PORTAL_AUTHORITY_INVALID');
  const forwarded = incoming.get('x-forwarded-host')?.split(',')[0]?.trim();
  const host = forwarded && forwarded.length > 0 ? forwarded : incoming.get('host');
  const holding = host
    ? config.holdings.find(
        (entry) => new URL(entry.origin).host === host && entry.holdingSlug === slug,
      )
    : undefined;
  if (!holding) return fail('NATIVE_CUSTOMER_PORTAL_AUTHORITY_INVALID');
  return holding;
}

async function boundedJson(response: Response): Promise<unknown> {
  if (
    response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json' ||
    !response.body
  ) {
    void response.body?.cancel().catch(() => undefined);
    return fail('NATIVE_CUSTOMER_PORTAL_RESPONSE_INVALID');
  }
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    total += value.byteLength;
    if (total > RESPONSE_LIMIT) {
      await reader.cancel();
      return fail('NATIVE_CUSTOMER_PORTAL_RESPONSE_LIMIT');
    }
    chunks.push(value);
  }
  const bytes = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
  } catch {
    return fail('NATIVE_CUSTOMER_PORTAL_RESPONSE_INVALID');
  }
}

function projection(value: unknown): NativeCustomerPortalProjection {
  if (
    !object(value) ||
    !Array.isArray(value.items) ||
    value.projectionVersion !== PROJECTION_VERSION
  )
    return fail('NATIVE_CUSTOMER_PORTAL_RESPONSE_INVALID');
  for (const item of value.items) {
    if (
      !object(item) ||
      !text(item.binding_id, 200) ||
      !integer(item.binding_version, 1) ||
      !text(item.order_id, 200) ||
      !Array.isArray(item.fulfillments) ||
      !Array.isArray(item.actions) ||
      !Array.isArray(item.sharedFiles)
    )
      return fail('NATIVE_CUSTOMER_PORTAL_RESPONSE_INVALID');
  }
  return value as unknown as NativeCustomerPortalProjection;
}

function outcome(value: unknown): NativeCustomerPortalOutcome {
  if (
    !object(value) ||
    !text(value.id, 200) ||
    !text(value.bindingId, 200) ||
    !text(value.orderId, 200) ||
    !text(value.action, 64) ||
    !text(value.status, 64) ||
    !integer(value.createdAt) ||
    value.createdAt > 8.64e15
  )
    return fail('NATIVE_CUSTOMER_PORTAL_RESPONSE_INVALID');
  return value as unknown as NativeCustomerPortalOutcome;
}

const FORWARDED = [
  'cookie',
  'authorization',
  'x-erpx-context-company',
  'x-erpx-context-assignment',
  'x-erpx-context-revision',
  'x-erpx-context-assignment-version',
  'x-erpx-workspace-branch',
  'x-erpx-workspace-department',
];

async function call(
  config: Configuration,
  incoming: Incoming,
  slug: string,
  method: 'GET' | 'POST',
  body: string | undefined,
  key: string | undefined,
  send: Send,
) {
  if (body !== undefined && new TextEncoder().encode(body).length > BODY_LIMIT)
    return fail('NATIVE_CUSTOMER_PORTAL_BODY_LIMIT', 413);
  const holding = target(config, incoming, slug);
  const origin = new URL(holding.origin);
  const headers = new Headers({
    host: origin.host,
    accept: 'application/json',
    origin: holding.origin,
    'cache-control': 'no-cache',
  });
  for (const name of FORWARDED) {
    const value = incoming.get(name);
    if (value) {
      if (new TextEncoder().encode(value).length > 8192)
        return fail('NATIVE_CUSTOMER_PORTAL_HEADER_LIMIT', 413);
      headers.set(name, value);
    }
  }
  if (method === 'POST') {
    headers.set('content-type', 'application/json');
    if (!key) return fail('COMMAND_KEY_REQUIRED', 422);
    headers.set('idempotency-key', key);
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/customer-portal`,
    config.privateBaseUrl,
  );
  const signal = AbortSignal.timeout(config.timeoutMs);
  const response = await send(url, { method, headers, body, cache: 'no-store' }, signal).catch(
    () => fail('NATIVE_CUSTOMER_PORTAL_UNAVAILABLE', 503),
  );
  const value = await boundedJson(response);
  if (response.status !== 200 && response.status !== 201) {
    if (object(value) && object(value.error) && text(value.error.code, 200))
      return fail(value.error.code, response.status);
    return fail('NATIVE_CUSTOMER_PORTAL_UNAVAILABLE', response.status);
  }
  if (!object(value) || !object(value.data))
    return fail('NATIVE_CUSTOMER_PORTAL_RESPONSE_INVALID');
  return { data: value.data, kind: method };
}

const direct: Send = (url, init, signal) => fetch(url, { ...init, signal });

export function createNativeCustomerPortalClient(raw: string, send: Send = direct) {
  const config = configuration(raw);
  return {
    projection: (incoming: Incoming, slug: string) =>
      call(config, incoming, slug, 'GET', undefined, undefined, send).then((result) =>
        projection(result.data),
      ),
    command: (incoming: Incoming, slug: string, input: unknown, key: string) => {
      if (!object(input)) return Promise.reject(new NativeCustomerPortalError('CUSTOMER_PORTAL_REQUEST_INVALID', 422));
      let body: string;
      try {
        body = JSON.stringify(input);
      } catch {
        return Promise.reject(new NativeCustomerPortalError('CUSTOMER_PORTAL_REQUEST_INVALID', 422));
      }
      return call(config, incoming, slug, 'POST', body, key, send).then((result) =>
        outcome(result.data),
      );
    },
  };
}

export function nativeCustomerPortalConfiguration(raw: string) {
  return configuration(raw);
}
