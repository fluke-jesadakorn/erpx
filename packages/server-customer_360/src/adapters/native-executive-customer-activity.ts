import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { CustomerActivityItem } from '../public/customer-timeline';

type Configuration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Incoming = Pick<Headers, 'get'>;
type Send = (url: URL, headers: Headers, signal: AbortSignal) => Promise<Response>;

export class NativeCustomerActivityError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number) {
    super('The native customer activity request could not be completed.');
    this.name = 'NativeCustomerActivityError';
    this.code = code;
    this.status = status;
  }
}

const fail = (code: string, status?: number): never => {
  throw new NativeCustomerActivityError(code, status);
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, maximum = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= maximum;
const integer = (value: unknown, minimum = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= minimum;
const nullable = (value: unknown) => value === null || text(value);

function configuration(raw: string): Configuration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail('NATIVE_CUSTOMER_ACTIVITY_CONFIGURATION_INVALID');
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
    return fail('NATIVE_CUSTOMER_ACTIVITY_CONFIGURATION_INVALID');
  const base = new URL(value.privateBaseUrl);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    [3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080].includes(
      Number(base.port),
    ) ||
    (Number(base.port) >= 3200 && Number(base.port) <= 3299) ||
    value.privateBaseUrl !== base.origin
  )
    return fail('NATIVE_CUSTOMER_ACTIVITY_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of value.holdings) {
    if (
      !object(holding) ||
      !text(holding.origin) ||
      !text(holding.holdingId) ||
      !text(holding.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug)
    )
      return fail('NATIVE_CUSTOMER_ACTIVITY_CONFIGURATION_INVALID');
    const origin = new URL(holding.origin);
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      holdings.has(origin.host)
    )
      return fail('NATIVE_CUSTOMER_ACTIVITY_CONFIGURATION_INVALID');
    holdings.set(origin.host, {
      holdingId: holding.holdingId,
      holdingSlug: holding.holdingSlug,
    });
  }
  return {
    privateBaseUrl: value.privateBaseUrl,
    holdings: value.holdings as Configuration['holdings'],
    timeoutMs: value.timeoutMs,
  };
}

function target(config: Configuration, incoming: Incoming, actor: WorkContext) {
  const forwarded = incoming.get('x-forwarded-host')?.split(',')[0]?.trim();
  const host = forwarded && forwarded.length > 0 ? forwarded : incoming.get('host');
  const holding = host
    ? config.holdings.find((entry) => new URL(entry.origin).host === host)
    : undefined;
  if (
    !holding ||
    holding.holdingSlug !== actor.holdingSlug ||
    holding.holdingId !== actor.holdingId
  )
    return fail('NATIVE_CUSTOMER_ACTIVITY_AUTHORITY_INVALID');
  return holding;
}

async function boundedJson(response: Response): Promise<unknown> {
  if (
    response.headers.get('content-type')?.split(';')[0]?.trim() !==
      'application/json' ||
    !response.body
  ) {
    void response.body?.cancel().catch(() => undefined);
    return fail('NATIVE_CUSTOMER_ACTIVITY_RESPONSE_INVALID');
  }
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    total += value.byteLength;
    if (total > 512 * 1024) {
      await reader.cancel();
      return fail('NATIVE_CUSTOMER_ACTIVITY_RESPONSE_LIMIT');
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
    return fail('NATIVE_CUSTOMER_ACTIVITY_RESPONSE_INVALID');
  }
}

function row(value: unknown): CustomerActivityItem {
  if (
    !object(value) ||
    !text(value.id) ||
    !text(value.source_id) ||
    !text(value.source_type) ||
    !text(value.title) ||
    !text(value.status) ||
    !integer(value.version, 1) ||
    !integer(value.at) ||
    !text(value.href) ||
    !value.href.startsWith('/') ||
    !text(value.customer_id) ||
    !text(value.customer_name)
  )
    return fail('NATIVE_CUSTOMER_ACTIVITY_RESPONSE_INVALID');
  return value as unknown as CustomerActivityItem;
}

async function request(
  config: Configuration,
  incoming: Incoming,
  actor: WorkContext,
  params: Record<string, string | number | undefined>,
  send: Send,
) {
  const holding = target(config, incoming, actor);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/customers/activity`,
    config.privateBaseUrl,
  );
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined) url.searchParams.set(key, String(value));
  }
  const headers = new Headers({
    host: new URL(holding.origin).host,
    accept: 'application/json',
    'cache-control': 'no-cache',
    origin: holding.origin,
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return fail('NATIVE_CUSTOMER_ACTIVITY_COOKIE_LIMIT');
    headers.set('cookie', cookie);
  }
  const response = await send(
    url,
    headers,
    AbortSignal.timeout(config.timeoutMs),
  ).catch(() => fail('NATIVE_CUSTOMER_ACTIVITY_UNAVAILABLE'));
  const value = await boundedJson(response);
  if (response.status !== 200) {
    if (object(value) && object(value.error) && text(value.error.code))
      return fail(value.error.code, response.status);
    return fail('NATIVE_CUSTOMER_ACTIVITY_UNAVAILABLE', response.status);
  }
  if (!object(value) || !object(value.data) || !Array.isArray(value.data.rows))
    return fail('NATIVE_CUSTOMER_ACTIVITY_RESPONSE_INVALID');
  return value.data.rows.map(row);
}

const direct: Send = (url, headers, signal) =>
  fetch(url, {
    method: 'GET',
    headers: Object.fromEntries(headers),
    signal,
    cache: 'no-store',
  });

export function createNativeCustomerActivityClient(
  raw: string,
  send: Send = direct,
) {
  const config = configuration(raw);
  return {
    search: (
      incoming: Incoming,
      actor: WorkContext,
      input: {
        from: number;
        to: number;
        query: string;
        limit: number;
        cursorAt?: number;
        cursorId?: string;
      },
    ) =>
      request(
        config,
        incoming,
        actor,
        {
          from: input.from,
          to: input.to,
          query: input.query || undefined,
          limit: input.limit,
          cursorAt: input.cursorAt,
          cursorId: input.cursorId,
        },
        send,
      ),
    read: (
      incoming: Incoming,
      actor: WorkContext,
      recordId: string,
      filters: { from: number; to: number },
    ) =>
      request(
        config,
        incoming,
        actor,
        {
          from: filters.from,
          to: filters.to,
          recordId,
          limit: 1,
        },
        send,
      ).then((rows) => rows[0] ?? null),
  };
}

export function nativeCustomerActivityConfiguration(raw: string) {
  return configuration(raw);
}

export { nullable as _nullable };
