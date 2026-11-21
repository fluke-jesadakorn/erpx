import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

/** Rendering input from retained page authorization, never a grant. */
export type NativeSalesPricingPageContext = {
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

export type NativeSalesPricingConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;
type Context = Row & { modules: string[]; permissions: string[] };

export type NativeSalesPricingCommand = 'create' | 'update' | 'decide';
export type NativeSalesPricingStatus = 'draft' | 'active' | 'superseded' | 'cancelled';

export type NativeSalesPricingReceipt = Readonly<{
  id: string;
  status: NativeSalesPricingStatus;
  version: number;
}>;

export type NativeSalesPricingTransport = (
  method: 'POST',
  url: URL,
  headers: Headers,
  body: Uint8Array,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeSalesPricingError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Sales pricing request could not be completed.');
    this.name = 'NativeSalesPricingError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeSalesPricingError(code, status, message);
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
const statuses = new Set<NativeSalesPricingStatus>([
  'draft',
  'active',
  'superseded',
  'cancelled',
]);

function context(value: unknown, actor: NativeSalesPricingPageContext): Context {
  if (
    !exact(value, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug',
      'departmentSlug', 'assignmentId', 'assignmentVersion', 'revision', 'role',
      'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ])
  )
    return failure('NATIVE_SALES_PRICING_RESPONSE_INVALID');
  const parsed = value as Context;
  if (
    parsed.userId !== actor.userId ||
    parsed.holdingId !== actor.holdingId ||
    parsed.companyId !== actor.companyId ||
    parsed.branchId !== actor.branchId ||
    parsed.departmentId !== actor.departmentId ||
    parsed.branchSlug !== null ||
    parsed.departmentSlug !== null ||
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
    return failure('NATIVE_SALES_PRICING_CONTEXT_CHANGED');
  return parsed;
}

function receipt(value: unknown): NativeSalesPricingReceipt {
  if (!exact(value, ['id', 'status', 'version'])) return failure('NATIVE_SALES_PRICING_RESPONSE_INVALID');
  if (
    !identifier(value.id) ||
    typeof value.status !== 'string' ||
    !statuses.has(value.status as NativeSalesPricingStatus) ||
    !integer(value.version, 1)
  )
    return failure('NATIVE_SALES_PRICING_RESPONSE_INVALID');
  return value as NativeSalesPricingReceipt;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativeSalesPricingConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_SALES_PRICING_CONFIGURATION_INVALID');
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
    return failure('NATIVE_SALES_PRICING_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_SALES_PRICING_CONFIGURATION_INVALID');
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
      return failure('NATIVE_SALES_PRICING_CONFIGURATION_INVALID');
    holdings.set(origin.host, { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function pathFor(command: NativeSalesPricingCommand, priceListId: string | null): string {
  if (command === 'create') {
    if (priceListId !== null) return failure('NATIVE_SALES_PRICING_REQUEST_INVALID');
    return '';
  }
  if (!identifier(priceListId)) return failure('NATIVE_SALES_PRICING_REQUEST_INVALID');
  return command === 'update'
    ? `/${encodeURIComponent(priceListId)}/edit`
    : `/${encodeURIComponent(priceListId)}`;
}

function transportCall(
  transport: NativeSalesPricingTransport,
  config: ReturnType<typeof configuration>,
  incoming: Pick<Headers, 'get'>,
  actor: NativeSalesPricingPageContext,
  command: NativeSalesPricingCommand,
  priceListId: string | null,
  input: unknown,
  key: string,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? config.holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== actor.holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(new NativeSalesPricingError('NATIVE_SALES_PRICING_AUTHORITY_INVALID'));
  if (!actor.authority || !actor.iam || !texts(actor.modules) || !texts(actor.iam.permissions))
    return Promise.reject(new NativeSalesPricingError('NATIVE_SALES_PRICING_CONTEXT_CHANGED'));
  if (actor.branchId !== null || actor.departmentId !== null)
    return Promise.reject(
      new NativeSalesPricingError(
        'COMPANY_PRICING_SCOPE_REQUIRED',
        403,
        'Use a Company sales assignment to manage price lists.',
      ),
    );
  if (typeof key !== 'string' || key.length < 8 || key.length > 100 || !/^[A-Za-z0-9:_.-]+$/.test(key))
    return Promise.reject(new NativeSalesPricingError('NATIVE_SALES_PRICING_REQUEST_INVALID'));
  let body: string;
  try {
    body = JSON.stringify(input);
    if (body === undefined) throw new Error('missing body');
  } catch {
    return Promise.reject(new NativeSalesPricingError('NATIVE_SALES_PRICING_REQUEST_INVALID'));
  }
  if (new TextEncoder().encode(body).length > 256 * 1024)
    return Promise.reject(new NativeSalesPricingError('NATIVE_SALES_PRICING_REQUEST_INVALID'));

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
      return Promise.reject(new NativeSalesPricingError('NATIVE_SALES_PRICING_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/sales/pricing${pathFor(command, priceListId)}`,
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
        return failure('NATIVE_SALES_PRICING_RESPONSE_LIMIT');
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
    return failure('NATIVE_SALES_PRICING_UNAVAILABLE', response.status);
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
    return failure(code ?? 'NATIVE_SALES_PRICING_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_SALES_PRICING_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 64 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_SALES_PRICING_RESPONSE_INVALID');
  }
}

export function nativeSalesPricingAvailable(): boolean {
  return process.env.ERPX_NATIVE_SALES_PRICING_CONFIG !== undefined;
}

export function nativeSalesPricingConfiguration(raw: string): NativeSalesPricingConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeSalesPricingError('NATIVE_SALES_PRICING_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    !Array.isArray(value.holdings)
  )
    return failure('NATIVE_SALES_PRICING_CONFIGURATION_INVALID');
  if (
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) &&
        text(holding.origin) &&
        text(holding.holdingSlug) &&
        text(holding.holdingId),
    )
  )
    return failure('NATIVE_SALES_PRICING_CONFIGURATION_INVALID');
  const parsed = value as unknown as NativeSalesPricingConfiguration;
  configuration(parsed);
  return parsed;
}

function pageContext(actor: WorkContext): NativeSalesPricingPageContext {
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

export function createNativeSalesPricingClient(
  raw: NativeSalesPricingConfiguration,
  transport: NativeSalesPricingTransport = directHttp,
) {
  const config = configuration(raw);
  return {
    async command(
      incoming: Pick<Headers, 'get'>,
      actor: NativeSalesPricingPageContext,
      command: NativeSalesPricingCommand,
      priceListId: string | null,
      input: unknown,
      key: string,
      signal?: AbortSignal,
    ): Promise<NativeSalesPricingReceipt> {
      if (!['create', 'update', 'decide'].includes(command))
        return failure('NATIVE_SALES_PRICING_REQUEST_INVALID');
      const response = await transportCall(
        transport,
        config,
        incoming,
        actor,
        command,
        priceListId,
        input,
        key,
        signal,
      ).catch((error: unknown) => {
        if (error instanceof NativeSalesPricingError) throw error;
        return failure('NATIVE_SALES_PRICING_UNAVAILABLE');
      });
      const value = await boundedJson(response);
      if (!exact(value, ['data', 'context']))
        return failure('NATIVE_SALES_PRICING_RESPONSE_INVALID');
      context(value.context, actor);
      return receipt(value.data);
    },
  };
}

/**
 * Native command ingress used by the retained Commerce pricing POST branch.
 * `undefined` means the selector is unset and the caller must use the retained
 * TypeScript owner. A configured failure throws a mapped ApiError and never
 * falls back.
 */
export async function nativeSalesPricingCommand(
  actor: WorkContext,
  command: NativeSalesPricingCommand,
  priceListId: string | null,
  input: unknown,
  key: string,
  incoming: Pick<Headers, 'get'>,
  signal?: AbortSignal,
): Promise<NativeSalesPricingReceipt | undefined> {
  const raw = process.env.ERPX_NATIVE_SALES_PRICING_CONFIG;
  if (raw === undefined) return undefined;
  try {
    const client = createNativeSalesPricingClient(nativeSalesPricingConfiguration(raw));
    return await client.command(
      incoming,
      pageContext(actor),
      command,
      priceListId,
      input,
      key,
      signal,
    );
  } catch (error) {
    const mapped = nativeSalesPricingFailure(error);
    if (mapped) throw new ApiError(mapped.status, mapped.code, mapped.message);
    throw error;
  }
}

/** Bounded failure policy for the retained pricing route error handling. */
export function nativeSalesPricingFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeSalesPricingError)) return undefined;
  if (error.code === 'NATIVE_SALES_PRICING_CONTEXT_CHANGED')
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
      code: error.code === 'NATIVE_SALES_PRICING_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return {
      status: 404,
      code: 'NOT_FOUND',
      message: 'Price lists are not available in this scope.',
    };
  if (error.status === 409 || error.status === 422)
    return { status: error.status, code: error.code, message: error.message };
  if (error.status === 503 || (error.status ?? 0) >= 500)
    return {
      status: 503,
      code: 'NATIVE_SALES_PRICING_UNAVAILABLE',
      message: 'The native Sales pricing service is unavailable.',
    };
  if (error.code === 'NATIVE_SALES_PRICING_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_SALES_PRICING_UNAVAILABLE',
      message: 'The native Sales pricing service is unavailable.',
    };
  if (error.code === 'NATIVE_SALES_PRICING_UNAVAILABLE')
    return {
      status: 503,
      code: 'NATIVE_SALES_PRICING_UNAVAILABLE',
      message: 'The native Sales pricing service is unavailable.',
    };
  if (error.code === 'NATIVE_SALES_PRICING_REQUEST_INVALID')
    return {
      status: 422,
      code: 'PRICE_LIST_REQUEST_INVALID',
      message: 'Review the price-list request.',
    };
  if (
    error.code === 'NATIVE_SALES_PRICING_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_SALES_PRICING_COOKIE_LIMIT' ||
    error.code === 'NATIVE_SALES_PRICING_RESPONSE_INVALID' ||
    error.code === 'NATIVE_SALES_PRICING_RESPONSE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_SALES_PRICING_UNAVAILABLE',
      message: 'The native Sales pricing service is unavailable.',
    };
  return undefined;
}

/** Fresh bounded socket; bypasses patched fetch/HMR and ambient proxies. */
export function directHttp(
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
