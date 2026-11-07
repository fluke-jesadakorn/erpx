import 'server-only';

/**
 * Server-only transport for the native Manage ERPX company-onboarding read.
 *
 * The configuration is trusted deployment input. It never carries a session,
 * actor, Company or permission. The incoming Account cookie is forwarded
 * unchanged to the private native service, which must resolve it again.
 */
export type NativeManageErpxConfiguration = Readonly<{
  privateBaseUrl: string;
  /** Exact registered front origins, for example `https://acme.example.test`. */
  hosts: readonly string[];
  timeoutMs: number;
}>;

export type NativeManageErpxOnboardingSlot = Readonly<{
  slug: string;
  company_name: string;
  branch_name: string;
  state: 'available' | 'provisioning' | 'active';
}>;

export type NativeManageErpxOnboardingView = Readonly<{
  enabled: boolean;
  slot: NativeManageErpxOnboardingSlot | null;
}>;

export type NativeManageErpxTransport = (
  method: 'GET',
  url: URL,
  headers: Headers,
  body: undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeManageErpxError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Manage ERPX request could not be completed.');
    this.name = 'NativeManageErpxError';
    this.code = code;
    this.status = status;
  }
}

type Row = Record<string, unknown>;

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeManageErpxError(code, status, message);
};

const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));

const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;

const identifier = (value: unknown): value is string => text(value, 200) && value.length > 0;

const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const loopbackHosts = new Set(['127.0.0.1', '[::1]', 'localhost']);
const onboardingStates = new Set(['available', 'provisioning', 'active']);

function configuration(raw: NativeManageErpxConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_MANAGE_ERPX_CONFIGURATION_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    reservedPorts.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    raw.privateBaseUrl !== base.origin ||
    !Number.isSafeInteger(raw.timeoutMs) ||
    raw.timeoutMs < 250 ||
    raw.timeoutMs > 30_000 ||
    !Array.isArray(raw.hosts) ||
    raw.hosts.length < 1 ||
    raw.hosts.length > 64
  ) {
    return failure('NATIVE_MANAGE_ERPX_CONFIGURATION_INVALID');
  }
  const hosts = new Map<string, string>();
  for (const rawHost of raw.hosts) {
    if (typeof rawHost !== 'string') return failure('NATIVE_MANAGE_ERPX_CONFIGURATION_INVALID');
    let origin: URL;
    try {
      origin = new URL(rawHost);
    } catch {
      return failure('NATIVE_MANAGE_ERPX_CONFIGURATION_INVALID');
    }
    const secure = origin.protocol === 'https:';
    const local = origin.protocol === 'http:' && loopbackHosts.has(origin.hostname);
    if (
      (!secure && !local) ||
      rawHost !== origin.origin ||
      origin.username ||
      origin.password ||
      (!secure && origin.port && reservedPorts.has(Number(origin.port))) ||
      hosts.has(origin.host)
    ) {
      return failure('NATIVE_MANAGE_ERPX_CONFIGURATION_INVALID');
    }
    hosts.set(origin.host, rawHost);
  }
  return { base, hosts, timeout: raw.timeoutMs };
}

export function nativeManageErpxConfiguration(raw: string): NativeManageErpxConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return failure('NATIVE_MANAGE_ERPX_CONFIGURATION_INVALID');
  }
  if (
    !exact(value, ['privateBaseUrl', 'hosts', 'timeoutMs']) ||
    !Array.isArray(value.hosts) ||
    typeof value.privateBaseUrl !== 'string' ||
    typeof value.timeoutMs !== 'number'
  ) {
    return failure('NATIVE_MANAGE_ERPX_CONFIGURATION_INVALID');
  }
  const config = {
    privateBaseUrl: value.privateBaseUrl,
    hosts: value.hosts as string[],
    timeoutMs: value.timeoutMs,
  };
  configuration(config);
  return config;
}

async function boundedBytes(response: Response, limit: number): Promise<Uint8Array> {
  if (!response.body) return new Uint8Array();
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.byteLength;
      if (bytes > limit) {
        await reader.cancel();
        return failure('NATIVE_MANAGE_ERPX_RESPONSE_LIMIT');
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
    offset += chunk.length;
  }
  return data;
}

async function boundedJson(response: Response): Promise<unknown> {
  if (response.redirected) {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_MANAGE_ERPX_UNAVAILABLE', response.status);
  }
  const contentType = response.headers.get('content-type')?.split(';')[0]?.trim();
  if (response.status !== 200) {
    const body = await boundedBytes(response, 64 * 1024);
    let code: string | undefined;
    let message: string | undefined;
    if (contentType === 'application/json') {
      try {
        const value: unknown = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(body));
        if (record(value) && record(value.error)) {
          if (text(value.error.code, 200)) code = value.error.code;
          if (text(value.error.message, 2000)) message = value.error.message;
        }
      } catch {
        // A malformed owner error keeps the bounded transport failure.
      }
    }
    return failure(code ?? 'NATIVE_MANAGE_ERPX_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_MANAGE_ERPX_RESPONSE_INVALID');
  }
  const body = await boundedBytes(response, 64 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(body));
  } catch {
    return failure('NATIVE_MANAGE_ERPX_RESPONSE_INVALID');
  }
}

function slot(value: unknown): NativeManageErpxOnboardingSlot {
  if (
    !exact(value, ['slug', 'company_name', 'branch_name', 'state']) ||
    !text(value.slug, 64) ||
    !/^[a-z][a-z0-9-]*$/.test(value.slug) ||
    !text(value.company_name, 200) ||
    value.company_name.trim().length === 0 ||
    !text(value.branch_name, 200) ||
    value.branch_name.trim().length === 0 ||
    !onboardingStates.has(String(value.state))
  ) {
    return failure('NATIVE_MANAGE_ERPX_RESPONSE_INVALID');
  }
  return value as unknown as NativeManageErpxOnboardingSlot;
}

function onboardingView(
  value: unknown,
  expectedUserId: string,
): NativeManageErpxOnboardingView {
  if (!exact(value, ['data', 'context']) || !exact(value.context, ['userId']))
    return failure('NATIVE_MANAGE_ERPX_RESPONSE_INVALID');
  if (!identifier(value.context.userId) || value.context.userId !== expectedUserId)
    return failure('NATIVE_MANAGE_ERPX_RESPONSE_INVALID');
  const data = value.data;
  if (!exact(data, ['enabled', 'slot']) || typeof data.enabled !== 'boolean')
    return failure('NATIVE_MANAGE_ERPX_RESPONSE_INVALID');
  return {
    enabled: data.enabled,
    slot: data.slot === null ? null : slot(data.slot),
  };
}

export function createNativeManageErpxClient(
  raw: NativeManageErpxConfiguration,
  transport: NativeManageErpxTransport = (method, url, headers, body, signal) =>
    fetch(url, { method, headers, body, signal, redirect: 'error' }),
) {
  const config = configuration(raw);
  return {
    async onboardingView(
      incoming: Pick<Headers, 'get'>,
      expectedUserId: string,
      signal?: AbortSignal,
    ): Promise<NativeManageErpxOnboardingView> {
      if (!identifier(expectedUserId)) return failure('NATIVE_MANAGE_ERPX_REQUEST_INVALID');
      const host = incoming.get('host');
      if (!host || !config.hosts.has(host))
        return failure('NATIVE_MANAGE_ERPX_AUTHORITY_INVALID');
      const headers = new Headers({
        host,
        accept: 'application/json',
        'cache-control': 'no-cache',
      });
      const cookie = incoming.get('cookie');
      if (cookie) {
        if (new TextEncoder().encode(cookie).length > 8192)
          return failure('NATIVE_MANAGE_ERPX_COOKIE_LIMIT');
        headers.set('cookie', cookie);
      }
      const controller = new AbortController();
      const cancel = () => controller.abort();
      signal?.addEventListener('abort', cancel, { once: true });
      if (signal?.aborted) controller.abort();
      const timer = setTimeout(cancel, config.timeout);
      try {
        const response = await transport(
          'GET',
          new URL('/api/v1/onboarding', config.base),
          headers,
          undefined,
          controller.signal,
        );
        return onboardingView(await boundedJson(response), expectedUserId);
      } finally {
        clearTimeout(timer);
        signal?.removeEventListener('abort', cancel);
      }
    },
  };
}

/** Bounded error mapping for the retained Next route and page shells. */
export function nativeManageErpxFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeManageErpxError)) return undefined;
  if (error.code === 'NATIVE_MANAGE_ERPX_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_MANAGE_ERPX_UNAVAILABLE',
      message: 'The native Manage ERPX service is unavailable.',
    };
  if (error.code === 'NATIVE_MANAGE_ERPX_AUTHORITY_INVALID')
    return {
      status: 403,
      code: 'FORBIDDEN',
      message: 'Use the registered application host.',
    };
  if (error.code === 'NATIVE_MANAGE_ERPX_REQUEST_INVALID')
    return { status: 422, code: error.code, message: error.message };
  if (
    error.code === 'NATIVE_MANAGE_ERPX_RESPONSE_INVALID' ||
    error.code === 'NATIVE_MANAGE_ERPX_RESPONSE_LIMIT' ||
    error.code === 'NATIVE_MANAGE_ERPX_COOKIE_LIMIT' ||
    error.code === 'NATIVE_MANAGE_ERPX_UNAVAILABLE'
  )
    return {
      status: 503,
      code: 'NATIVE_MANAGE_ERPX_UNAVAILABLE',
      message: 'The native Manage ERPX service is unavailable.',
    };
  if (error.status === 401)
    return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
  if (error.status === 403)
    return { status: 403, code: error.code, message: error.message };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_MANAGE_ERPX_UNAVAILABLE',
      message: 'The native Manage ERPX service is unavailable.',
    };
  return undefined;
}
