import 'server-only';
import { directHttp } from './native-iam-assignments';

/**
 * Native People relationship transport. The reviewed preview/apply commands are
 * owned by the Rust Identity Access service; this module never mints a session,
 * Company, assignment or permission. Unconfigured deployments fail closed.
 */
export type NativePeopleRelationshipsContext = Readonly<{
  userId: string;
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  assignmentId: string;
  role: string;
  scopeKind: string;
  authority: { revision: number; assignmentVersion: number };
  iam: { permissions: readonly string[] };
}>;

export type NativePeopleRelationshipsConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

export type NativePeopleRelationshipsTransport = (
  method: 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativePeopleRelationshipsError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native People relationship request could not be completed.');
    this.name = 'NativePeopleRelationshipsError';
    this.code = code;
    this.status = status;
  }
}

type Row = Record<string, unknown>;
const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, maximum = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= maximum;
const identifier = (value: unknown): value is string =>
  text(value, 200) && value.length > 0 && !/[\u0000-\u001f\u007f]/.test(value);
const optionalIdentifier = (value: unknown): value is string | null =>
  value === null || identifier(value);
const integer = (value: unknown, minimum = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= minimum;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativePeopleRelationshipsError(code, status, message);
};

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativePeopleRelationshipsConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_PEOPLE_RELATIONSHIPS_CONFIGURATION_INVALID');
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
    !integer(raw.timeoutMs, 250) ||
    raw.timeoutMs > 30_000 ||
    !Array.isArray(raw.holdings) ||
    raw.holdings.length < 1 ||
    raw.holdings.length > 64
  )
    return failure('NATIVE_PEOPLE_RELATIONSHIPS_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_PEOPLE_RELATIONSHIPS_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      origin.username ||
      origin.password ||
      !identifier(holding.holdingId) ||
      !text(holding.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug) ||
      holdings.has(origin.host)
    )
      return failure('NATIVE_PEOPLE_RELATIONSHIPS_CONFIGURATION_INVALID');
    holdings.set(origin.host, {
      holdingId: holding.holdingId,
      holdingSlug: holding.holdingSlug,
    });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function context(value: unknown, actor: NativePeopleRelationshipsContext): Row {
  if (!record(value)) return failure('NATIVE_PEOPLE_RELATIONSHIPS_RESPONSE_INVALID');
  if (
    value.userId !== actor.userId ||
    value.holdingId !== actor.holdingId ||
    value.companyId !== actor.companyId ||
    optionalIdentifier(value.branchId) === false ||
    value.branchId !== actor.branchId ||
    optionalIdentifier(value.departmentId) === false ||
    value.departmentId !== actor.departmentId ||
    value.assignmentId !== actor.assignmentId ||
    value.assignmentVersion !== actor.authority.assignmentVersion ||
    value.revision !== actor.authority.revision ||
    value.role !== actor.role ||
    !integer(value.expiresAt, 1) ||
    !Array.isArray(value.permissions) ||
    value.permissions.length > 512 ||
    !value.permissions.every((item) => identifier(item)) ||
    value.permissions.length !== actor.iam.permissions.length ||
    !value.permissions.every((item) => actor.iam.permissions.includes(item))
  )
    return failure('NATIVE_PEOPLE_RELATIONSHIPS_CONTEXT_CHANGED');
  return value;
}

function preview(value: unknown) {
  if (
    !record(value) ||
    !UUID.test(String(value.token)) ||
    !record(value.change) ||
    !text(value.summary) ||
    !Array.isArray(value.affected) ||
    !integer(value.expiresAt, 1)
  )
    return failure('NATIVE_PEOPLE_RELATIONSHIPS_RESPONSE_INVALID');
  return value;
}

function applied(value: unknown) {
  if (
    !exact(value, ['id', 'change', 'version']) ||
    !identifier(value.id) ||
    !record(value.change) ||
    !integer(value.version, 1)
  )
    return failure('NATIVE_PEOPLE_RELATIONSHIPS_RESPONSE_INVALID');
  return value;
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
        return failure('NATIVE_PEOPLE_RELATIONSHIPS_RESPONSE_LIMIT');
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
    return failure('NATIVE_PEOPLE_RELATIONSHIPS_UNAVAILABLE', response.status);
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
        // Keep the bounded transport failure when the owner error is malformed.
      }
    }
    return failure(
      code ?? 'NATIVE_PEOPLE_RELATIONSHIPS_UNAVAILABLE',
      response.status,
      message,
    );
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_PEOPLE_RELATIONSHIPS_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_PEOPLE_RELATIONSHIPS_RESPONSE_INVALID');
  }
}

/** Normalized, validated transport configuration for owner clients. */
export type NativePeopleRelationshipsNormalized = ReturnType<typeof configuration>;
export const nativePeopleRelationshipsNormalized = configuration;

export function nativePeopleRelationshipsAvailable(): boolean {
  return process.env.ERPX_NATIVE_IDENTITY_ACCESS_CONFIG !== undefined;
}

export function nativePeopleRelationshipsConfiguration(
  raw: string,
): NativePeopleRelationshipsConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativePeopleRelationshipsError(
      'NATIVE_PEOPLE_RELATIONSHIPS_CONFIGURATION_INVALID',
    );
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    !Array.isArray(value.holdings) ||
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) &&
        text(holding.origin) &&
        text(holding.holdingSlug) &&
        text(holding.holdingId),
    )
  )
    return failure('NATIVE_PEOPLE_RELATIONSHIPS_CONFIGURATION_INVALID');
  return value as unknown as NativePeopleRelationshipsConfiguration;
}

export function nativePeopleRelationshipsFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativePeopleRelationshipsError)) return undefined;
  if (error.code === 'NATIVE_PEOPLE_RELATIONSHIPS_CONTEXT_CHANGED')
    return {
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before retrying.',
    };
  if (error.status === 401)
    return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
  if (error.status === 403)
    return {
      status: 403,
      code: error.code === 'NATIVE_PEOPLE_RELATIONSHIPS_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return {
      status: 404,
      code: error.code === 'NATIVE_PEOPLE_RELATIONSHIPS_UNAVAILABLE' ? 'NOT_FOUND' : error.code,
      message: error.message,
    };
  if (error.status === 409) return { status: 409, code: error.code, message: error.message };
  if (error.status === 422) return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_PEOPLE_RELATIONSHIPS_UNAVAILABLE',
      message: 'The native People relationship service is unavailable.',
    };
  if (error.code === 'NATIVE_PEOPLE_RELATIONSHIPS_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_PEOPLE_RELATIONSHIPS_UNAVAILABLE',
      message: 'The native People relationship service is unavailable.',
    };
  if (error.code === 'NATIVE_PEOPLE_RELATIONSHIPS_REQUEST_INVALID')
    return {
      status: 422,
      code: 'PEOPLE_ACCESS_REQUEST_INVALID',
      message: 'Review the relationship details and try again.',
    };
  if (
    error.code === 'NATIVE_PEOPLE_RELATIONSHIPS_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_PEOPLE_RELATIONSHIPS_COOKIE_LIMIT' ||
    error.code === 'NATIVE_PEOPLE_RELATIONSHIPS_RESPONSE_INVALID' ||
    error.code === 'NATIVE_PEOPLE_RELATIONSHIPS_RESPONSE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_PEOPLE_RELATIONSHIPS_UNAVAILABLE',
      message: 'The native People relationship service is unavailable.',
    };
  return undefined;
}

function transportCall(
  transport: NativePeopleRelationshipsTransport,
  config: ReturnType<typeof configuration>,
  incoming: Pick<Headers, 'get'>,
  actor: NativePeopleRelationshipsContext,
  path: string,
  body: string,
  key: string,
  signal: AbortSignal | undefined,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? config.holdings.get(host) : undefined;
  if (
    !holding ||
    holding.holdingSlug !== actor.holdingSlug ||
    holding.holdingId !== actor.holdingId
  )
    return Promise.reject(
      new NativePeopleRelationshipsError('NATIVE_PEOPLE_RELATIONSHIPS_AUTHORITY_INVALID'),
    );
  if (!text(key, 180) || key.length < 8 || !/^[A-Za-z0-9:_.-]+$/.test(key))
    return Promise.reject(
      new NativePeopleRelationshipsError('NATIVE_PEOPLE_RELATIONSHIPS_REQUEST_INVALID'),
    );
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'content-type': 'application/json',
    'idempotency-key': key,
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(
        new NativePeopleRelationshipsError('NATIVE_PEOPLE_RELATIONSHIPS_COOKIE_LIMIT'),
      );
    headers.set('cookie', cookie);
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/iam/people-relationships${path}`,
    config.base,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, config.timeout);
  return transport('POST', url, headers, body, controller.signal).finally(() => {
    clearTimeout(timer);
    signal?.removeEventListener('abort', cancel);
  });
}

export function createNativePeopleRelationshipsClient(
  raw: NativePeopleRelationshipsConfiguration,
  transport: NativePeopleRelationshipsTransport = directHttp,
) {
  const config = configuration(raw);
  const run = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativePeopleRelationshipsContext,
    path: string,
    body: string,
    key: string,
    signal: AbortSignal | undefined,
    validate: (value: unknown) => unknown,
  ) => {
    if (!actor.authority || !actor.iam)
      return failure('NATIVE_PEOPLE_RELATIONSHIPS_CONTEXT_CHANGED');
    const response = await transportCall(
      transport,
      config,
      incoming,
      actor,
      path,
      body,
      key,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativePeopleRelationshipsError) throw error;
      return failure('NATIVE_PEOPLE_RELATIONSHIPS_UNAVAILABLE');
    });
    const value = await boundedJson(response);
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_PEOPLE_RELATIONSHIPS_RESPONSE_INVALID');
    context(value.context, actor);
    return validate(value.data);
  };
  return {
    preview: (
      incoming: Pick<Headers, 'get'>,
      actor: NativePeopleRelationshipsContext,
      change: unknown,
      signal?: AbortSignal,
    ) =>
      run(
        incoming,
        actor,
        '/preview',
        JSON.stringify({ change }),
        `people-preview-${crypto.randomUUID()}`,
        signal,
        preview,
      ),
    apply: (
      incoming: Pick<Headers, 'get'>,
      actor: NativePeopleRelationshipsContext,
      token: string,
      key: string,
      signal?: AbortSignal,
    ) => {
      if (!UUID.test(token))
        return Promise.reject(
          new NativePeopleRelationshipsError('NATIVE_PEOPLE_RELATIONSHIPS_REQUEST_INVALID'),
        );
      return run(
        incoming,
        actor,
        '/apply',
        JSON.stringify({ previewToken: token }),
        key,
        signal,
        applied,
      );
    },
  };
}
