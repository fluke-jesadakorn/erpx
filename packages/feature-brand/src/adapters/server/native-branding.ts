import 'server-only';
import {
  NativeBrandIdentityError,
  directHttp,
  nativeBrandIdentityConfiguration,
  type NativeBrandIdentityPageContext,
  type NativeBrandIdentityTransport,
} from './native-brand-identity';

/** Native Branding HTTP transport over the registered Holding origin. */
export type NativeBrandingReadInput = Readonly<{
  holdingSlug: string;
  branchSlug?: string;
  departmentSlug?: string;
  expected?: Readonly<{ companyId: string; assignmentId: string; revision: number; assignmentVersion: number }>;
}>;

export type NativeBrandingCommandInput = Readonly<{
  holdingSlug: string;
  branchSlug?: string;
  departmentSlug?: string;
  expected?: Readonly<{ companyId: string; assignmentId: string; revision: number; assignmentVersion: number }>;
  body: unknown;
  idempotencyKey: string;
  origin?: string;
}>;

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function text(value: unknown, max = 4096): value is string {
  return typeof value === 'string' && new TextEncoder().encode(value).length <= max;
}

function identifier(value: unknown): value is string {
  return text(value, 200) && value.length > 0;
}

function integer(value: unknown, min = 0): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
}

function configuration(raw: string) {
  const parsed = nativeBrandIdentityConfiguration(raw);
  const base = new URL(parsed.privateBaseUrl);
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    reserved.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    parsed.privateBaseUrl !== base.origin ||
    !integer(parsed.timeoutMs, 250) ||
    parsed.timeoutMs > 30000 ||
    parsed.holdings.length < 1 ||
    parsed.holdings.length > 64
  )
    throw new NativeBrandIdentityError('NATIVE_BRANDING_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of parsed.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      throw new NativeBrandIdentityError('NATIVE_BRANDING_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      origin.port ||
      origin.username ||
      origin.password ||
      !identifier(holding.holdingId) ||
      !text(holding.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug) ||
      holdings.has(origin.host)
    )
      throw new NativeBrandIdentityError('NATIVE_BRANDING_CONFIGURATION_INVALID');
    holdings.set(origin.host, { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug });
  }
  return { base, holdings, timeout: parsed.timeoutMs };
}

function headersFor(
  incoming: Pick<Headers, 'get'>,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  holdingSlug: string,
  input: NativeBrandingReadInput | NativeBrandingCommandInput,
): Headers {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== holdingSlug)
    throw new NativeBrandIdentityError('NATIVE_BRANDING_AUTHORITY_INVALID');
  const headers = new Headers({ host: host!, accept: 'application/json', 'cache-control': 'no-cache' });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      throw new NativeBrandIdentityError('NATIVE_BRANDING_COOKIE_LIMIT');
    headers.set('cookie', cookie);
  }
  for (const [key, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        throw new NativeBrandIdentityError('NATIVE_BRANDING_REQUEST_INVALID');
      headers.set(key, slug);
    }
  }
  if (input.expected) {
    const { companyId, assignmentId, revision, assignmentVersion } = input.expected;
    if (!identifier(companyId) || !identifier(assignmentId) || !integer(revision) || !integer(assignmentVersion, 1))
      throw new NativeBrandIdentityError('NATIVE_BRANDING_REQUEST_INVALID');
    headers.set('x-erpx-context-company', companyId);
    headers.set('x-erpx-context-assignment', assignmentId);
    headers.set('x-erpx-context-revision', String(revision));
    headers.set('x-erpx-context-assignment-version', String(assignmentVersion));
  }
  return headers;
}

async function boundedJson(response: Response): Promise<unknown> {
  if (response.redirected || response.status !== 200 || !response.body) {
    void response.body?.cancel().catch(() => undefined);
    throw new NativeBrandIdentityError('NATIVE_BRANDING_UNAVAILABLE', response.status);
  }
  if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    throw new NativeBrandIdentityError('NATIVE_BRANDING_RESPONSE_INVALID');
  }
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.byteLength;
      if (bytes > 2 * 1024 * 1024) {
        await reader.cancel();
        throw new NativeBrandIdentityError('NATIVE_BRANDING_RESPONSE_LIMIT');
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
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    throw new NativeBrandIdentityError('NATIVE_BRANDING_RESPONSE_INVALID');
  }
}

function data(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value) || !('data' in value) || !value.data || typeof value.data !== 'object' || Array.isArray(value.data))
    throw new NativeBrandIdentityError('NATIVE_BRANDING_RESPONSE_INVALID');
  return value.data as Record<string, unknown>;
}

export function createNativeBrandingClient(
  raw: string,
  transport: NativeBrandIdentityTransport = directHttp,
) {
  const config = configuration(raw);
  const request = async (
    method: 'GET' | 'POST',
    incoming: Pick<Headers, 'get'>,
    actor: NativeBrandIdentityPageContext,
    input: NativeBrandingReadInput | NativeBrandingCommandInput,
    body?: string,
    key?: string,
    origin?: string,
  ) => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      throw new NativeBrandIdentityError('NATIVE_BRANDING_CONTEXT_CHANGED');
    const headers = headersFor(incoming, config.holdings, input.holdingSlug, input);
    if (body !== undefined) {
      if (!key || !/^[A-Za-z0-9:_.-]{8,180}$/.test(key))
        throw new NativeBrandIdentityError('NATIVE_BRANDING_COMMAND_KEY_INVALID');
      if (new TextEncoder().encode(body).length > 64 * 1024)
        throw new NativeBrandIdentityError('NATIVE_BRANDING_REQUEST_INVALID');
      headers.set('content-type', 'application/json');
      headers.set('idempotency-key', key);
      if (origin) {
        let parsed: URL;
        try {
          parsed = new URL(origin);
        } catch {
          throw new NativeBrandIdentityError('NATIVE_BRANDING_REQUEST_INVALID');
        }
        if (origin !== parsed.origin) throw new NativeBrandIdentityError('NATIVE_BRANDING_REQUEST_INVALID');
        headers.set('origin', origin);
      }
    }
    const url = new URL(`/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/branding`, config.base);
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), config.timeout);
    try {
      return await transport(method, url, headers, body, controller.signal);
    } finally {
      clearTimeout(timer);
    }
  };
  return {
    async read(
      incoming: Pick<Headers, 'get'>,
      actor: NativeBrandIdentityPageContext,
      input: NativeBrandingReadInput,
    ) {
      const response = await request('GET', incoming, actor, input).catch((error: unknown) => {
        if (error instanceof NativeBrandIdentityError) throw error;
        throw new NativeBrandIdentityError('NATIVE_BRANDING_UNAVAILABLE');
      });
      return data(await boundedJson(response));
    },
    async command(
      incoming: Pick<Headers, 'get'>,
      actor: NativeBrandIdentityPageContext,
      input: NativeBrandingCommandInput,
    ) {
      const response = await request(
        'POST',
        incoming,
        actor,
        input,
        JSON.stringify(input.body),
        input.idempotencyKey,
        input.origin,
      ).catch((error: unknown) => {
        if (error instanceof NativeBrandIdentityError) throw error;
        throw new NativeBrandIdentityError('NATIVE_BRANDING_UNAVAILABLE');
      });
      return data(await boundedJson(response));
    },
  };
}

export function nativeBrandingFailure(error: unknown) {
  if (!(error instanceof NativeBrandIdentityError)) return undefined;
  if (error.code === 'NATIVE_BRANDING_CONTEXT_CHANGED')
    return { status: 409, code: 'WORK_CONTEXT_CHANGED', message: 'Your work role changed. Reload and review before submitting.' };
  if (error.code !== 'NATIVE_BRANDING_UNAVAILABLE') return undefined;
  switch (error.status) {
    case 401:
      return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
    case 403:
      return { status: 403, code: 'FORBIDDEN', message: 'This action is unavailable in your current role.' };
    case 404:
      return { status: 404, code: 'NOT_FOUND', message: 'Branding not found.' };
    case 409:
      return { status: 409, code: 'NATIVE_BRANDING_CONFLICT', message: 'Branding changed. Refresh and review again.' };
    default:
      return undefined;
  }
}
