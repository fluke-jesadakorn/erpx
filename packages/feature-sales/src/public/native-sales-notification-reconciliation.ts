import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

/** Current selected reviewer input from the retained page authorization. */
export type NativeSalesNotificationPageContext = {
  userId: string;
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  assignmentId: string;
  role: string;
  assignmentScope: string;
  authority?: { revision: number; assignmentVersion: number };
  modules: readonly string[];
  iam?: { permissions: readonly string[] };
};

export type NativeSalesNotificationConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;
type Context = Row & { modules: string[]; permissions: string[] };

export type NativeSalesNotificationReceipt = Readonly<{
  id: string;
  orderId: string;
  status: 'sent' | 'cancelled';
  version: number;
  reconciliationId: string;
}>;

export type NativeSalesNotificationTransport = (
  method: 'POST',
  url: URL,
  headers: Headers,
  body: Uint8Array,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeSalesNotificationError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Sales notification request could not be completed.');
    this.name = 'NativeSalesNotificationError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeSalesNotificationError(code, status, message);
};
const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string =>
  text(value, 200) &&
  value.length > 0 &&
  !/[\u0000-\u001f\u007f/\\?#]/.test(value);
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const texts = (value: unknown, max = 256): value is string[] =>
  Array.isArray(value) && value.length <= max && value.every((item) => text(item, 256));
const sameSet = (left: readonly string[], right: readonly string[]) => {
  const a = new Set(left);
  const b = new Set(right);
  return a.size === b.size && [...a].every((value) => b.has(value));
};

/** Strict reconciliation body: version, exact attempt and reviewer reason. */
export function salesNotificationReconciliationInput(raw: unknown): {
  expectedVersion: number;
  attemptId: string;
  reason: string;
} {
  if (
    !record(raw) ||
    Object.keys(raw).length !== 3 ||
    !Object.hasOwn(raw, 'expectedVersion') ||
    !Object.hasOwn(raw, 'attemptId') ||
    !Object.hasOwn(raw, 'reason')
  )
    return failure('SALES_NOTIFICATION_RECONCILIATION_REQUEST_INVALID', 422);
  if (!integer(raw.expectedVersion, 1)) {
    return failure('SALES_NOTIFICATION_RECONCILIATION_REQUEST_INVALID', 422);
  }
  if (
    typeof raw.attemptId !== 'string' ||
    raw.attemptId.trim() !== raw.attemptId ||
    raw.attemptId.length < 1 ||
    raw.attemptId.length > 256 ||
    /[\u0000-\u001f\u007f]/.test(raw.attemptId)
  )
    return failure('SALES_NOTIFICATION_RECONCILIATION_REQUEST_INVALID', 422);
  if (
    typeof raw.reason !== 'string' ||
    raw.reason.trim() !== raw.reason ||
    raw.reason.length < 5 ||
    raw.reason.length > 1000
  )
    return failure('SALES_NOTIFICATION_RECONCILIATION_REQUEST_INVALID', 422);
  return { expectedVersion: raw.expectedVersion, attemptId: raw.attemptId, reason: raw.reason };
}

function context(value: unknown, actor: NativeSalesNotificationPageContext): Context {
  if (
    !exact(value, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug',
      'departmentSlug', 'assignmentId', 'assignmentVersion', 'revision', 'role',
      'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ])
  )
    return failure('NATIVE_SALES_NOTIFICATION_RECONCILIATION_RESPONSE_INVALID');
  const parsed = value as Context;
  if (
    parsed.userId !== actor.userId ||
    parsed.holdingId !== actor.holdingId ||
    parsed.companyId !== actor.companyId ||
    parsed.branchId !== actor.branchId ||
    parsed.departmentId !== actor.departmentId ||
    parsed.assignmentId !== actor.assignmentId ||
    parsed.assignmentVersion !== actor.authority?.assignmentVersion ||
    parsed.revision !== actor.authority?.revision ||
    parsed.role !== actor.role ||
    parsed.assignmentScopeKind !== actor.assignmentScope ||
    !integer(parsed.expiresAt, 1) ||
    !texts(parsed.modules) ||
    !sameSet(actor.modules, parsed.modules) ||
    !texts(parsed.permissions) ||
    !sameSet(actor.iam?.permissions ?? [], parsed.permissions)
  )
    return failure('NATIVE_SALES_NOTIFICATION_RECONCILIATION_CONTEXT_CHANGED', 409);
  return parsed;
}

function receipt(value: unknown): NativeSalesNotificationReceipt {
  if (
    !exact(value, ['id', 'orderId', 'status', 'version', 'reconciliationId']) ||
    !identifier(value.id) ||
    !identifier(value.orderId) ||
    !(value.status === 'sent' || value.status === 'cancelled') ||
    !integer(value.version, 2) ||
    !identifier(value.reconciliationId)
  )
    return failure('NATIVE_SALES_NOTIFICATION_RECONCILIATION_RESPONSE_INVALID');
  return value as NativeSalesNotificationReceipt;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativeSalesNotificationConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_SALES_NOTIFICATION_RECONCILIATION_CONFIGURATION_INVALID');
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
    raw.timeoutMs > 30000 ||
    raw.holdings.length < 1 ||
    raw.holdings.length > 64
  )
    return failure('NATIVE_SALES_NOTIFICATION_RECONCILIATION_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_SALES_NOTIFICATION_RECONCILIATION_CONFIGURATION_INVALID');
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
      return failure('NATIVE_SALES_NOTIFICATION_RECONCILIATION_CONFIGURATION_INVALID');
    holdings.set(origin.host, { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativeSalesNotificationTransport,
  config: ReturnType<typeof configuration>,
  incoming: Pick<Headers, 'get'>,
  actor: NativeSalesNotificationPageContext,
  notificationId: string,
  input: unknown,
  key: string,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? config.holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== actor.holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(
      new NativeSalesNotificationError('NATIVE_SALES_NOTIFICATION_RECONCILIATION_AUTHORITY_INVALID'),
    );
  if (!actor.authority || !actor.iam || !texts(actor.modules) || !texts(actor.iam.permissions))
    return Promise.reject(
      new NativeSalesNotificationError('NATIVE_SALES_NOTIFICATION_RECONCILIATION_CONTEXT_CHANGED'),
    );
  if (typeof key !== 'string' || key.length < 8 || key.length > 200 || !/^[A-Za-z0-9:_.-]+$/.test(key))
    return Promise.reject(
      new NativeSalesNotificationError('SALES_NOTIFICATION_RECONCILIATION_REQUEST_INVALID'),
    );
  let body: string;
  try {
    body = JSON.stringify(input);
    if (body === undefined) throw new Error('missing body');
  } catch {
    return Promise.reject(
      new NativeSalesNotificationError('SALES_NOTIFICATION_RECONCILIATION_REQUEST_INVALID'),
    );
  }
  if (new TextEncoder().encode(body).length > 16 * 1024)
    return Promise.reject(
      new NativeSalesNotificationError('SALES_NOTIFICATION_RECONCILIATION_REQUEST_INVALID'),
    );

  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    'content-type': 'application/json',
    'cache-control': 'no-cache',
    'idempotency-key': key,
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority.revision),
    'x-erpx-context-assignment-version': String(actor.authority.assignmentVersion),
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(
        new NativeSalesNotificationError('NATIVE_SALES_NOTIFICATION_RECONCILIATION_COOKIE_LIMIT'),
      );
    headers.set('cookie', cookie);
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/customer-notifications/${encodeURIComponent(notificationId)}/reconcile`,
    config.base,
  );
  const payload = new TextEncoder().encode(body);
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, config.timeout);
  return transport('POST', url, headers, payload, controller.signal).finally(() => {
    clearTimeout(timer);
    signal?.removeEventListener('abort', cancel);
  });
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
        return failure('NATIVE_SALES_NOTIFICATION_RECONCILIATION_RESPONSE_LIMIT');
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
    return failure('NATIVE_SALES_NOTIFICATION_RECONCILIATION_UNAVAILABLE', response.status);
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
    return failure(code ?? 'NATIVE_SALES_NOTIFICATION_RECONCILIATION_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_SALES_NOTIFICATION_RECONCILIATION_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 64 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_SALES_NOTIFICATION_RECONCILIATION_RESPONSE_INVALID');
  }
}

function pageContext(actor: WorkContext): NativeSalesNotificationPageContext {
  return {
    userId: actor.userId,
    holdingId: actor.holdingId,
    holdingSlug: actor.holdingSlug,
    companyId: actor.companyId,
    branchId: actor.branchId,
    departmentId: actor.departmentId,
    assignmentId: actor.assignmentId,
    role: actor.role,
    assignmentScope: actor.assignmentScope,
    authority: actor.authority,
    modules: actor.modules,
    iam: actor.iam,
  };
}

export function createNativeSalesNotificationClient(
  raw: NativeSalesNotificationConfiguration,
  transport: NativeSalesNotificationTransport,
) {
  const config = configuration(raw);
  return {
    async reconcile(
      incoming: Pick<Headers, 'get'>,
      actor: NativeSalesNotificationPageContext,
      notificationId: string,
      input: unknown,
      key: string,
      signal?: AbortSignal,
    ): Promise<NativeSalesNotificationReceipt> {
      if (!identifier(notificationId))
        return failure('SALES_NOTIFICATION_RECONCILIATION_REQUEST_INVALID');
      const response = await transportCall(
        transport,
        config,
        incoming,
        actor,
        notificationId,
        input,
        key,
        signal,
      ).catch((error: unknown) => {
        if (error instanceof NativeSalesNotificationError) throw error;
        return failure('NATIVE_SALES_NOTIFICATION_RECONCILIATION_UNAVAILABLE');
      });
      const value = await boundedJson(response);
      if (!exact(value, ['data', 'context']))
        return failure('NATIVE_SALES_NOTIFICATION_RECONCILIATION_RESPONSE_INVALID');
      context(value.context, actor);
      return receipt(value.data);
    },
  };
}

/**
 * Native reconciliation ingress for the retained Commerce POST branch. The
 * caller never falls back to a TypeScript provider read or writer.
 */
export async function nativeSalesNotificationReconcile(
  actor: WorkContext,
  notificationId: string,
  input: unknown,
  key: string,
  incoming: Pick<Headers, 'get'>,
  signal?: AbortSignal,
): Promise<NativeSalesNotificationReceipt> {
  const raw = process.env.ERPX_NATIVE_SALES_NOTIFICATIONS_CONFIG;
  if (raw === undefined)
    throw new ApiError(
      503,
      'NATIVE_SALES_NOTIFICATION_RECONCILIATION_UNAVAILABLE',
      'The native Sales notification reconciliation service is unavailable.',
    );
  try {
    let parsed: unknown;
    try {
      parsed = JSON.parse(raw);
    } catch {
      return failure('NATIVE_SALES_NOTIFICATION_RECONCILIATION_CONFIGURATION_INVALID');
    }
    if (
      !record(parsed) ||
      !text(parsed.privateBaseUrl) ||
      !integer(parsed.timeoutMs) ||
      !Array.isArray(parsed.holdings) ||
      !parsed.holdings.every(
        (holding: unknown) =>
          record(holding) &&
          text(holding.origin) &&
          text(holding.holdingSlug) &&
          text(holding.holdingId),
      )
    )
      return failure('NATIVE_SALES_NOTIFICATION_RECONCILIATION_CONFIGURATION_INVALID');
    const client = createNativeSalesNotificationClient(
      parsed as unknown as NativeSalesNotificationConfiguration,
      directHttp,
    );
    return await client.reconcile(
      incoming,
      pageContext(actor),
      notificationId,
      input,
      key,
      signal,
    );
  } catch (error) {
    const mapped = nativeSalesNotificationFailure(error);
    if (mapped) throw new ApiError(mapped.status, mapped.code, mapped.message);
    throw error;
  }
}

export function nativeSalesNotificationFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeSalesNotificationError)) return undefined;
  if (error.code === 'NATIVE_SALES_NOTIFICATION_RECONCILIATION_CONTEXT_CHANGED')
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
      code: error.code === 'NATIVE_SALES_NOTIFICATION_RECONCILIATION_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return {
      status: 404,
      code: 'NOT_FOUND',
      message: 'Customer notices are not available in this scope.',
    };
  if (error.status === 409 || error.status === 422)
    return { status: error.status, code: error.code, message: error.message };
  if (error.code === 'SALES_NOTIFICATION_RECONCILIATION_REQUEST_INVALID')
    return {
      status: 422,
      code: error.code,
      message: 'Review the reconciliation request.',
    };
  if (
    error.status === 503 ||
    (error.status ?? 0) >= 500 ||
    error.code === 'NATIVE_SALES_NOTIFICATION_RECONCILIATION_CONFIGURATION_INVALID' ||
    error.code === 'NATIVE_SALES_NOTIFICATION_RECONCILIATION_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_SALES_NOTIFICATION_RECONCILIATION_COOKIE_LIMIT' ||
    error.code === 'NATIVE_SALES_NOTIFICATION_RECONCILIATION_RESPONSE_INVALID' ||
    error.code === 'NATIVE_SALES_NOTIFICATION_RECONCILIATION_RESPONSE_LIMIT' ||
    error.code === 'NATIVE_SALES_NOTIFICATION_RECONCILIATION_UNAVAILABLE'
  )
    return {
      status: 503,
      code: 'NATIVE_SALES_NOTIFICATION_RECONCILIATION_UNAVAILABLE',
      message: 'The native Sales notification reconciliation service is unavailable.',
    };
  return undefined;
}

/** Fresh bounded socket; bypasses patched fetch/HMR and ambient proxies. */
function directHttp(
  _method: 'POST',
  url: URL,
  headers: Headers,
  body: Uint8Array,
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
      {
        method: 'POST',
        agent,
        headers: {
          ...Object.fromEntries(headers),
          'content-length': String(body.byteLength),
        },
        signal,
        maxHeaderSize: 8192,
      },
      (incoming) => {
        incoming.once('close', () => agent.destroy());
        try {
          const status = incoming.statusCode ?? 502;
          const values = new Headers();
          for (let index = 0; index < incoming.rawHeaders.length; index += 2)
            values.append(incoming.rawHeaders[index]!, incoming.rawHeaders[index + 1]!);
          resolve(
            new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, {
              status: status >= 200 && status <= 599 ? status : 502,
              headers: values,
            }),
          );
        } catch (error) {
          incoming.destroy();
          agent.destroy();
          reject(error);
        }
      },
    );
    outgoing.once('error', (error) => {
      agent.destroy();
      reject(error);
    });
    outgoing.end(Buffer.from(body));
  });
}
