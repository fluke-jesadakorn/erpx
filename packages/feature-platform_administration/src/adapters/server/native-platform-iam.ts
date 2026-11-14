import 'server-only';
import { Agent, request } from 'node:http';
import { allowedPrivateLoopbackPort } from '@erpx/contracts/private-loopback';

/**
 * Strict private-loopback transport for the Platform IAM command slice.
 *
 * It forwards the retained Account session cookie, the exact Platform Host,
 * browser Origin, current work selectors and `x-erpx-platform-assignment`. It
 * never mints a session, Platform assignment or permission; the native owner
 * re-resolves the current Platform actor and executes one serializable unit.
 */

/** Rendering input from the retained Manage ERPX authorization, never a grant. */
export type NativePlatformIamActor = {
  userId: string;
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  workAssignmentId: string;
  platformAssignmentId: string;
  platformRole: string;
  authority?: { revision: number; assignmentVersion: number };
  modules: readonly string[];
  iam?: { permissions: readonly string[] };
};

export type NativePlatformIamCommand =
  | { kind: 'role-save' }
  | { kind: 'review-decide'; reviewId: string }
  | { kind: 'access-review-start' }
  | { kind: 'access-review-item-decide'; itemId: string }
  | { kind: 'access-review-cancel'; campaignId: string };

export type NativePlatformIamConfiguration = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  production?: boolean;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
}>;

type Row = Record<string, unknown>;

export class NativePlatformIamError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Platform IAM request could not be completed.');
    this.name = 'NativePlatformIamError';
    this.code = code;
    this.status = status;
  }
}

const fail = (code: string, status?: number, message?: string): never => {
  throw new NativePlatformIamError(code, status, message);
};
const object = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string =>
  text(value, 200) && value.length > 0 && !/[\u0000-\u001f\u007f]/.test(value);
const integer = (value: unknown, minimum = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= minimum;

function configuration(raw: NativePlatformIamConfiguration): NativePlatformIamConfiguration {
  if (
    typeof raw?.privateBaseUrl !== 'string' ||
    (raw.production !== undefined && typeof raw.production !== 'boolean') ||
    !integer(raw.timeoutMs, 250) ||
    raw.timeoutMs > 30000 ||
    !Array.isArray(raw.holdings) ||
    raw.holdings.length < 1 ||
    raw.holdings.length > 64
  )
    throw new NativePlatformIamError('NATIVE_PLATFORM_IAM_CONFIGURATION_INVALID');
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    throw new NativePlatformIamError('NATIVE_PLATFORM_IAM_CONFIGURATION_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    !allowedPrivateLoopbackPort(port, raw.production === true) ||
    base.username ||
    base.password ||
    raw.privateBaseUrl !== base.origin
  )
    throw new NativePlatformIamError('NATIVE_PLATFORM_IAM_CONFIGURATION_INVALID');
  const hosts = new Set<string>();
  const slugs = new Set<string>();
  const ids = new Set<string>();
  const holdings = raw.holdings.map((holding) => {
    if (
      !object(holding) ||
      typeof holding.origin !== 'string' ||
      !identifier(holding.holdingSlug) ||
      !/^[a-z0-9_-]{1,128}$/.test(holding.holdingSlug) ||
      !identifier(holding.holdingId)
    )
      throw new NativePlatformIamError('NATIVE_PLATFORM_IAM_CONFIGURATION_INVALID');
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      throw new NativePlatformIamError('NATIVE_PLATFORM_IAM_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== holding.origin ||
      hosts.has(origin.host) ||
      slugs.has(holding.holdingSlug) ||
      ids.has(holding.holdingId)
    )
      throw new NativePlatformIamError('NATIVE_PLATFORM_IAM_CONFIGURATION_INVALID');
    hosts.add(origin.host);
    slugs.add(holding.holdingSlug);
    ids.add(holding.holdingId);
    return {
      origin: holding.origin,
      holdingSlug: holding.holdingSlug,
      holdingId: holding.holdingId,
    };
  });
  return {
    privateBaseUrl: raw.privateBaseUrl,
    timeoutMs: raw.timeoutMs,
    production: raw.production === true,
    holdings,
  };
}

function directHttp(
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import('node:http').ProxyEnv,
    });
    const outgoing = request(
      url,
      { method, agent, headers: Object.fromEntries(headers), signal, maxHeaderSize: 8192 },
      (response) => {
        const chunks: Buffer[] = [];
        response.on('data', (chunk) => chunks.push(Buffer.from(chunk)));
        response.on('end', () => {
          agent.destroy();
          resolve(
            new Response(Buffer.concat(chunks), {
              status: response.statusCode ?? 500,
              headers: response.headers as HeadersInit,
            }),
          );
        });
      },
    );
    outgoing.once('error', (error) => {
      agent.destroy();
      reject(error);
    });
    outgoing.end(body);
  });
}
type Transport = typeof directHttp;

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
        return fail('NATIVE_PLATFORM_IAM_RESPONSE_LIMIT');
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
    return fail('NATIVE_PLATFORM_IAM_UNAVAILABLE', response.status);
  }
  const contentType = response.headers.get('content-type')?.split(';')[0]?.trim();
  if (!response.ok) {
    const body = await boundedBytes(response, 64 * 1024);
    let code = 'NATIVE_PLATFORM_IAM_UNAVAILABLE';
    let message: string | undefined;
    if (contentType === 'application/json') {
      try {
        const value: unknown = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(body));
        if (object(value) && object(value.error)) {
          if (text(value.error.code, 200) && /^[A-Z][A-Z0-9_]{2,80}$/.test(value.error.code))
            code = value.error.code;
          if (text(value.error.message, 2000)) message = value.error.message;
        }
      } catch {
        /* Retain the bounded status failure. */
      }
    }
    return fail(code, response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return fail('NATIVE_PLATFORM_IAM_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return fail('NATIVE_PLATFORM_IAM_RESPONSE_INVALID');
  }
}

function headers(
  incoming: Pick<Headers, 'get'>,
  actor: NativePlatformIamActor,
  holdingOrigin: string,
  body: boolean,
  key: string | undefined,
): Headers {
  const host = incoming.get('host');
  if (!host) throw new NativePlatformIamError('NATIVE_PLATFORM_IAM_AUTHORITY_INVALID');
  const origin = incoming.get('origin');
  if (origin !== holdingOrigin)
    throw new NativePlatformIamError('NATIVE_PLATFORM_IAM_ORIGIN_INVALID');
  const values = new Headers({
    host,
    accept: 'application/json',
    'cache-control': 'no-cache',
    origin,
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      throw new NativePlatformIamError('NATIVE_PLATFORM_IAM_COOKIE_LIMIT');
    values.set('cookie', cookie);
  }
  if (!actor.authority || !actor.iam)
    throw new NativePlatformIamError('NATIVE_PLATFORM_IAM_CONTEXT_CHANGED');
  values.set('x-erpx-context-company', actor.companyId);
  values.set('x-erpx-context-assignment', actor.workAssignmentId);
  values.set('x-erpx-context-revision', String(actor.authority.revision));
  values.set(
    'x-erpx-context-assignment-version',
    String(actor.authority.assignmentVersion),
  );
  values.set('x-erpx-platform-assignment', actor.platformAssignmentId);
  if (body) {
    if (!key || !/^[A-Za-z0-9:_.-]{8,180}$/.test(key))
      throw new NativePlatformIamError('NATIVE_PLATFORM_IAM_COMMAND_KEY_INVALID');
    values.set('content-type', 'application/json');
    values.set('idempotency-key', key);
    values.set('x-erpx-method', 'POST');
    values.set('x-erpx-request-method', 'POST');
  }
  return values;
}

function path(command: NativePlatformIamCommand): string {
  switch (command.kind) {
    case 'role-save':
      return '/roles';
    case 'review-decide':
      if (!identifier(command.reviewId) || /[/\\]/.test(command.reviewId))
        return fail('NATIVE_PLATFORM_IAM_REQUEST_INVALID');
      return `/reviews/${encodeURIComponent(command.reviewId)}/decide`;
    case 'access-review-start':
      return '/access-reviews';
    case 'access-review-item-decide':
      if (!identifier(command.itemId) || /[/\\]/.test(command.itemId))
        return fail('NATIVE_PLATFORM_IAM_REQUEST_INVALID');
      return `/access-review-items/${encodeURIComponent(command.itemId)}/decide`;
    case 'access-review-cancel':
      if (!identifier(command.campaignId) || /[/\\]/.test(command.campaignId))
        return fail('NATIVE_PLATFORM_IAM_REQUEST_INVALID');
      return `/access-reviews/${encodeURIComponent(command.campaignId)}/cancel`;
  }
}

function context(value: unknown, actor: NativePlatformIamActor): Row {
  if (!object(value)) return fail('NATIVE_PLATFORM_IAM_CONTEXT_CHANGED');
  if (
    value.userId !== actor.userId ||
    value.holdingId !== actor.holdingId ||
    value.companyId !== actor.companyId ||
    value.workAssignmentId !== actor.workAssignmentId ||
    value.platformAssignmentId !== actor.platformAssignmentId ||
    value.platformRole !== actor.platformRole ||
    !integer(value.assignmentVersion, 1) ||
    !integer(value.expiresAt, 1) ||
    !Array.isArray(value.permissions) ||
    !value.permissions.every((permission) => identifier(permission))
  )
    return fail('NATIVE_PLATFORM_IAM_CONTEXT_CHANGED');
  if (
    actor.authority &&
    (value.revision !== actor.authority.revision ||
      value.assignmentVersion !== actor.authority.assignmentVersion)
  )
    return fail('NATIVE_PLATFORM_IAM_CONTEXT_CHANGED');
  return value;
}

function receipt(value: unknown): Row {
  if (!object(value) || !identifier(value.id))
    return fail('NATIVE_PLATFORM_IAM_RESPONSE_INVALID');
  return value;
}

export function nativePlatformIamConfigured(): boolean {
  return process.env.ERPX_NATIVE_PLATFORM_IAM_CONFIG !== undefined;
}

export function nativePlatformIamConfiguration(raw: string): NativePlatformIamConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativePlatformIamError('NATIVE_PLATFORM_IAM_CONFIGURATION_INVALID');
  }
  if (!object(value)) throw new NativePlatformIamError('NATIVE_PLATFORM_IAM_CONFIGURATION_INVALID');
  return configuration(value as unknown as NativePlatformIamConfiguration);
}

export function nativePlatformIamFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativePlatformIamError)) return undefined;
  if (error.code === 'NATIVE_PLATFORM_IAM_CONTEXT_CHANGED')
    return {
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before retrying.',
    };
  if (error.code === 'NATIVE_PLATFORM_IAM_ORIGIN_INVALID')
    return {
      status: 403,
      code: 'ORIGIN_FORBIDDEN',
      message: 'Use the ERPX application to submit this request.',
    };
  if (error.status === 401)
    return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
  if (error.status === 403)
    return { status: 403, code: error.code, message: error.message };
  if (error.status === 404)
    return { status: 404, code: error.code, message: error.message };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_PLATFORM_IAM_UNAVAILABLE',
      message: 'The native Platform IAM service is unavailable.',
    };
  if (error.code === 'NATIVE_PLATFORM_IAM_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_PLATFORM_IAM_UNAVAILABLE',
      message: 'The native Platform IAM service is unavailable.',
    };
  if (error.code === 'NATIVE_PLATFORM_IAM_REQUEST_INVALID')
    return {
      status: 422,
      code: 'PLATFORM_IAM_REQUEST_INVALID',
      message: 'Review the Platform identity request.',
    };
  if (error.code === 'NATIVE_PLATFORM_IAM_COMMAND_KEY_INVALID')
    return {
      status: 422,
      code: 'COMMAND_KEY_REQUIRED',
      message: 'A valid Idempotency-Key is required.',
    };
  if (
    error.code === 'NATIVE_PLATFORM_IAM_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_PLATFORM_IAM_COOKIE_LIMIT' ||
    error.code === 'NATIVE_PLATFORM_IAM_RESPONSE_INVALID' ||
    error.code === 'NATIVE_PLATFORM_IAM_RESPONSE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_PLATFORM_IAM_UNAVAILABLE',
      message: 'The native Platform IAM service is unavailable.',
    };
  return undefined;
}

export function createNativePlatformIamClient(
  raw: NativePlatformIamConfiguration,
  transport: Transport = directHttp,
) {
  const config = configuration(raw);
  return {
    async command(
      incoming: Pick<Headers, 'get'>,
      actor: NativePlatformIamActor,
      command: NativePlatformIamCommand,
      body: string | undefined,
      key: string | undefined,
      signal?: AbortSignal,
    ): Promise<Row> {
      const host = incoming.get('host');
      const holding = config.holdings.find(
        (candidate) =>
          candidate.holdingSlug === actor.holdingSlug &&
          candidate.holdingId === actor.holdingId &&
          candidate.origin === incoming.get('origin'),
      );
      if (!holding || new URL(holding.origin).host !== host)
        throw new NativePlatformIamError('NATIVE_PLATFORM_IAM_AUTHORITY_INVALID');
      const payload = body === undefined ? undefined : body;
      const values = headers(incoming, actor, holding.origin, payload !== undefined, key);
      if (payload !== undefined && new TextEncoder().encode(payload).length > 256 * 1024)
        throw new NativePlatformIamError('NATIVE_PLATFORM_IAM_REQUEST_INVALID');
      const url = new URL(`/api/v1/platform/iam${path(command)}`, config.privateBaseUrl);
      const controller = new AbortController();
      const cancel = () => controller.abort();
      signal?.addEventListener('abort', cancel, { once: true });
      if (signal?.aborted) controller.abort();
      const timer = setTimeout(cancel, config.timeoutMs);
      try {
        const response = await transport(
          payload === undefined ? 'GET' : 'POST',
          url,
          values,
          payload,
          controller.signal,
        );
        const value = await boundedJson(response);
        if (!object(value) || !object(value.data) || !object(value.context))
          return fail('NATIVE_PLATFORM_IAM_RESPONSE_INVALID');
        context(value.context, actor);
        return receipt(value.data);
      } catch (error) {
        if (error instanceof NativePlatformIamError) throw error;
        throw new NativePlatformIamError('NATIVE_PLATFORM_IAM_UNAVAILABLE');
      } finally {
        clearTimeout(timer);
        signal?.removeEventListener('abort', cancel);
      }
    },
  };
}
