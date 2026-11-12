import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

export type NativeFollowupPageContext = {
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

export type NativeFollowupConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;
type Context = Row & { modules: string[]; permissions: string[] };

export type NativeFollowupReceipt = Readonly<{
  id: string;
  version: number;
  status: 'open' | 'completed' | 'cancelled';
}>;

export type NativeFollowupTransport = (
  method: 'POST',
  url: URL,
  headers: Headers,
  body: Uint8Array,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeFollowupError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Sales follow-up request could not be completed.');
    this.name = 'NativeFollowupError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeFollowupError(code, status, message);
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
const date = (value: unknown): value is string => {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const parsed = new Date(`${value}T00:00:00Z`);
  return !Number.isNaN(parsed.valueOf()) && parsed.toISOString().slice(0, 10) === value;
};
const boundedText = (value: unknown): value is string =>
  typeof value === 'string' &&
  value.trim() === value &&
  value.trim().length >= 3 &&
  value.trim().length <= 2000;

/** Strict create body: one source-linked contact with an eligible owner. */
export function customerFollowupCreateInput(raw: unknown): {
  orderId: string;
  expectedVersion: number;
  ownerId: string;
  dueDate: string;
  nextAction: string;
} {
  if (
    !record(raw) ||
    Object.keys(raw).length !== 5 ||
    !['orderId', 'expectedVersion', 'ownerId', 'dueDate', 'nextAction'].every((key) =>
      Object.hasOwn(raw, key),
    ) ||
    !identifier(raw.orderId) ||
    raw.expectedVersion !== 1 ||
    !identifier(raw.ownerId) ||
    !date(raw.dueDate) ||
    !boundedText(raw.nextAction)
  )
    return failure('CUSTOMER_FOLLOWUP_REQUEST_INVALID', 422);
  return {
    orderId: raw.orderId,
    expectedVersion: 1,
    ownerId: raw.ownerId,
    dueDate: raw.dueDate,
    nextAction: raw.nextAction,
  };
}

/** Strict change body: reassign carries owner/due, outcome actions carry reason. */
export function customerFollowupChangeInput(raw: unknown): {
  action: 'reassign' | 'complete' | 'cancel';
  expectedVersion: number;
  ownerId?: string;
  dueDate?: string;
  reason: string;
} {
  if (
    !record(raw) ||
    !['reassign', 'complete', 'cancel'].includes(String(raw.action)) ||
    !integer(raw.expectedVersion, 1) ||
    !boundedText(raw.reason)
  )
    return failure('CUSTOMER_FOLLOWUP_REQUEST_INVALID', 422);
  if (raw.action === 'reassign') {
    if (
      Object.keys(raw).length !== 5 ||
      !identifier(raw.ownerId) ||
      !date(raw.dueDate)
    )
      return failure('CUSTOMER_FOLLOWUP_REQUEST_INVALID', 422);
    return {
      action: 'reassign',
      expectedVersion: raw.expectedVersion,
      ownerId: raw.ownerId,
      dueDate: raw.dueDate,
      reason: raw.reason,
    };
  }
  if (Object.keys(raw).length !== 3 || Object.hasOwn(raw, 'ownerId') || Object.hasOwn(raw, 'dueDate'))
    return failure('CUSTOMER_FOLLOWUP_REQUEST_INVALID', 422);
  return {
    action: raw.action as 'complete' | 'cancel',
    expectedVersion: raw.expectedVersion,
    reason: raw.reason,
  };
}

function context(value: unknown, actor: NativeFollowupPageContext): Context {
  if (
    !exact(value, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug',
      'departmentSlug', 'assignmentId', 'assignmentVersion', 'revision', 'role',
      'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ])
  )
    return failure('NATIVE_SALES_FOLLOWUPS_RESPONSE_INVALID');
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
    return failure('NATIVE_SALES_FOLLOWUPS_CONTEXT_CHANGED', 409);
  return parsed;
}

function receipt(value: unknown): NativeFollowupReceipt {
  if (
    !exact(value, ['id', 'version', 'status']) ||
    !identifier(value.id) ||
    !integer(value.version, 1) ||
    !['open', 'completed', 'cancelled'].includes(String(value.status))
  )
    return failure('NATIVE_SALES_FOLLOWUPS_RESPONSE_INVALID');
  return value as NativeFollowupReceipt;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativeFollowupConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_SALES_FOLLOWUPS_CONFIGURATION_INVALID');
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
    return failure('NATIVE_SALES_FOLLOWUPS_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_SALES_FOLLOWUPS_CONFIGURATION_INVALID');
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
      return failure('NATIVE_SALES_FOLLOWUPS_CONFIGURATION_INVALID');
    holdings.set(origin.host, { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativeFollowupTransport,
  config: ReturnType<typeof configuration>,
  incoming: Pick<Headers, 'get'>,
  actor: NativeFollowupPageContext,
  path: string,
  input: unknown,
  key: string,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? config.holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== actor.holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(new NativeFollowupError('NATIVE_SALES_FOLLOWUPS_AUTHORITY_INVALID'));
  if (!actor.authority || !actor.iam || !texts(actor.modules) || !texts(actor.iam.permissions))
    return Promise.reject(new NativeFollowupError('NATIVE_SALES_FOLLOWUPS_CONTEXT_CHANGED'));
  if (typeof key !== 'string' || key.length < 8 || key.length > 200 || !/^[A-Za-z0-9:_.-]+$/.test(key))
    return Promise.reject(new NativeFollowupError('CUSTOMER_FOLLOWUP_REQUEST_INVALID'));
  let body: string;
  try {
    body = JSON.stringify(input);
    if (body === undefined) throw new Error('missing body');
  } catch {
    return Promise.reject(new NativeFollowupError('CUSTOMER_FOLLOWUP_REQUEST_INVALID'));
  }
  if (new TextEncoder().encode(body).length > 16 * 1024)
    return Promise.reject(new NativeFollowupError('CUSTOMER_FOLLOWUP_REQUEST_INVALID'));

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
      return Promise.reject(new NativeFollowupError('NATIVE_SALES_FOLLOWUPS_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/customer-followups${path}`,
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
        return failure('NATIVE_SALES_FOLLOWUPS_RESPONSE_LIMIT');
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
    return failure('NATIVE_SALES_FOLLOWUPS_UNAVAILABLE', response.status);
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
    return failure(code ?? 'NATIVE_SALES_FOLLOWUPS_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_SALES_FOLLOWUPS_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 64 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_SALES_FOLLOWUPS_RESPONSE_INVALID');
  }
}

function pageContext(actor: WorkContext): NativeFollowupPageContext {
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

export function createNativeFollowupClient(
  raw: NativeFollowupConfiguration,
  transport: NativeFollowupTransport,
) {
  const config = configuration(raw);
  async function send(
    incoming: Pick<Headers, 'get'>,
    actor: NativeFollowupPageContext,
    path: string,
    input: unknown,
    key: string,
    signal?: AbortSignal,
  ): Promise<NativeFollowupReceipt> {
    const response = await transportCall(
      transport,
      config,
      incoming,
      actor,
      path,
      input,
      key,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeFollowupError) throw error;
      return failure('NATIVE_SALES_FOLLOWUPS_UNAVAILABLE');
    });
    const value = await boundedJson(response);
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_SALES_FOLLOWUPS_RESPONSE_INVALID');
    context(value.context, actor);
    return receipt(value.data);
  }
  return {
    create(
      incoming: Pick<Headers, 'get'>,
      actor: NativeFollowupPageContext,
      input: unknown,
      key: string,
      signal?: AbortSignal,
    ) {
      return send(incoming, actor, '', customerFollowupCreateInput(input), key, signal);
    },
    change(
      incoming: Pick<Headers, 'get'>,
      actor: NativeFollowupPageContext,
      id: string,
      input: unknown,
      key: string,
      signal?: AbortSignal,
    ) {
      if (!identifier(id)) return Promise.reject(new NativeFollowupError('CUSTOMER_FOLLOWUP_REQUEST_INVALID'));
      return send(
        incoming,
        actor,
        `/${encodeURIComponent(id)}`,
        customerFollowupChangeInput(input),
        key,
        signal,
      );
    },
  };
}

/** Native create ingress; unset configuration fails closed with no TS fallback. */
export async function nativeCreateCustomerFollowup(
  actor: WorkContext,
  input: unknown,
  key: string,
  incoming: Pick<Headers, 'get'>,
  signal?: AbortSignal,
): Promise<NativeFollowupReceipt> {
  return run((client) => client.create(incoming, pageContext(actor), input, key, signal));
}

/** Native change ingress; unset configuration fails closed with no TS fallback. */
export async function nativeChangeCustomerFollowup(
  actor: WorkContext,
  id: string,
  input: unknown,
  key: string,
  incoming: Pick<Headers, 'get'>,
  signal?: AbortSignal,
): Promise<NativeFollowupReceipt> {
  return run((client) =>
    client.change(incoming, pageContext(actor), id, input, key, signal),
  );
}

async function run(
  call: (client: ReturnType<typeof createNativeFollowupClient>) => Promise<NativeFollowupReceipt>,
): Promise<NativeFollowupReceipt> {
  const raw = process.env.ERPX_NATIVE_SALES_FOLLOWUPS_CONFIG;
  if (raw === undefined)
    throw new ApiError(
      503,
      'NATIVE_SALES_FOLLOWUPS_UNAVAILABLE',
      'The native Sales follow-up service is unavailable.',
    );
  try {
    let parsed: unknown;
    try {
      parsed = JSON.parse(raw);
    } catch {
      return failure('NATIVE_SALES_FOLLOWUPS_CONFIGURATION_INVALID');
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
      return failure('NATIVE_SALES_FOLLOWUPS_CONFIGURATION_INVALID');
    return await call(
      createNativeFollowupClient(
        parsed as unknown as NativeFollowupConfiguration,
        directHttp,
      ),
    );
  } catch (error) {
    const mapped = nativeFollowupFailure(error);
    if (mapped) throw new ApiError(mapped.status, mapped.code, mapped.message);
    throw error;
  }
}

export function nativeFollowupFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeFollowupError)) return undefined;
  if (error.code === 'NATIVE_SALES_FOLLOWUPS_CONTEXT_CHANGED')
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
      code: error.code === 'NATIVE_SALES_FOLLOWUPS_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return {
      status: 404,
      code: 'NOT_FOUND',
      message: 'Customer follow-ups are not available in this scope.',
    };
  if (error.status === 409 || error.status === 422)
    return { status: error.status, code: error.code, message: error.message };
  if (
    error.code === 'CUSTOMER_FOLLOWUP_REQUEST_INVALID'
  )
    return { status: 422, code: error.code, message: 'Review the customer follow-up request.' };
  if (
    error.status === 503 ||
    (error.status ?? 0) >= 500 ||
    [
      'NATIVE_SALES_FOLLOWUPS_CONFIGURATION_INVALID',
      'NATIVE_SALES_FOLLOWUPS_AUTHORITY_INVALID',
      'NATIVE_SALES_FOLLOWUPS_COOKIE_LIMIT',
      'NATIVE_SALES_FOLLOWUPS_RESPONSE_INVALID',
      'NATIVE_SALES_FOLLOWUPS_RESPONSE_LIMIT',
      'NATIVE_SALES_FOLLOWUPS_UNAVAILABLE',
    ].includes(error.code)
  )
    return {
      status: 503,
      code: 'NATIVE_SALES_FOLLOWUPS_UNAVAILABLE',
      message: 'The native Sales follow-up service is unavailable.',
    };
  return undefined;
}

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
