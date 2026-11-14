import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';

/** Rendering input from retained page authorization, never a grant. */
export type NativeCustomerTimelinePageContext = {
  userId: string;
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  assignmentId: string;
  role: string;
  assignmentScope: string;
  scope:
    | { kind: 'holding' }
    | { kind: 'branch'; branchSlug: string }
    | { kind: 'department'; branchSlug?: string; departmentSlug: string };
  authority?: { revision: number; assignmentVersion: number };
  modules: readonly string[];
  iam?: { permissions: readonly string[] };
};

export type NativeCustomerTimelineConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;
type Context = Row & { modules: string[]; permissions: string[] };

export type NativeCustomerTimelineReadInput = Readonly<{
  holdingSlug: string;
  customerId: string;
  cursor?: string;
  limit?: number;
  branchSlug?: string;
  departmentSlug?: string;
  expected?: Readonly<{
    companyId: string;
    assignmentId: string;
    revision: number;
    assignmentVersion: number;
  }>;
}>;

export type NativeCustomerTimelineCustomer = Readonly<{
  id: string;
  name: string;
  code: string;
}>;

export type NativeCustomerTimelineItem = Readonly<{
  id: string;
  source_id: string;
  source_type: string;
  title: string;
  status: string;
  version: number;
  at: number;
  href: string;
}>;

export type NativeCustomerTimelinePage = Readonly<{
  customer: NativeCustomerTimelineCustomer;
  items: NativeCustomerTimelineItem[];
  hasMore: boolean;
  nextCursor: string | null;
  coverage: string[];
  limitations: string;
}>;

export type NativeCustomerTimelineTransport = (
  method: 'GET',
  url: URL,
  headers: Headers,
  body: undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeCustomerTimelineError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native customer timeline request could not be completed.');
    this.name = 'NativeCustomerTimelineError';
    this.code = code;
    this.status = status;
  }
}

export const NATIVE_CUSTOMER_TIMELINE_LIMITATIONS =
  'Internal staff projection. Source records remain authoritative. Order/return entries show their latest state, not a complete transition log. Field Service, Finance and Legal entries show scoped source state; Portal and customer-authority records are not included.';

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeCustomerTimelineError(code, status, message);
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
  text(value, 200) && value.length > 0 && !/[\u0000-\u001f\u007f]/.test(value);
const customerIdentifier = (value: unknown): value is string =>
  identifier(value) && /^[A-Za-z0-9._:-]+$/.test(value);
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const texts = (value: unknown, max = 256): value is string[] =>
  Array.isArray(value) && value.length <= max && value.every((item) => text(item, 256));
const sameSet = (left: readonly string[], right: readonly string[]) => {
  const a = new Set(left);
  const b = new Set(right);
  return a.size === b.size && [...a].every((value) => b.has(value));
};

const sourceTypes = new Set([
  'order',
  'fulfillment',
  'return',
  'contact',
  'followup',
  'support',
  'field_service',
  'legal',
  'invoice',
  'crm',
  'crm_activity',
]);
const sourcePrefixes: Record<string, readonly string[]> = {
  order: ['order'],
  fulfillment: ['fulfillment'],
  return: ['return'],
  contact: ['contact'],
  followup: ['followup'],
  support: ['support'],
  field_service: ['service-agreement', 'service-order'],
  legal: ['legal'],
  invoice: ['invoice'],
  crm: ['crm'],
  crm_activity: ['crm-activity'],
};
const coverageValues = new Set([
  'Sales quotations/orders',
  'Fulfillments and customer returns',
  'Internal contact history',
  'Customer follow-up history',
  'Qualified CRM lead/opportunity history',
  'CRM follow-up activities',
  'Support case history',
  'Field Service agreements and visits',
  'Finance invoice status',
  'Legal records linked to orders',
]);

const row = (value: unknown): value is NativeCustomerTimelineItem => {
  if (
    !exact(value, [
      'id',
      'source_id',
      'source_type',
      'title',
      'status',
      'version',
      'at',
      'href',
    ])
  )
    return false;
  if (
    typeof value.source_type !== 'string' ||
    !sourceTypes.has(value.source_type) ||
    !identifier(value.id) ||
    !identifier(value.source_id) ||
    !text(value.title, 500) ||
    value.title.length === 0 ||
    /[\u0000-\u001f\u007f]/.test(value.title) ||
    !text(value.status, 200) ||
    value.status.length === 0 ||
    /[\u0000-\u001f\u007f]/.test(value.status) ||
    !integer(value.version, 1) ||
    !integer(value.at) ||
    !text(value.href, 500) ||
    !value.href.startsWith('/') ||
    value.href.startsWith('//') ||
    /[\u0000-\u001f\u007f]/.test(value.href)
  )
    return false;
  const [prefix, suffix, ...rest] = value.id.split(':');
  const allowed = sourcePrefixes[value.source_type] ?? [];
  return (
    Boolean(prefix) &&
    Boolean(suffix) &&
    rest.length === 0 &&
    allowed.includes(prefix!) &&
    text(suffix, 200) &&
    !/[\u0000-\u001f\u007f]/.test(suffix!)
  );
};

const customer = (
  value: unknown,
  input: NativeCustomerTimelineReadInput,
): NativeCustomerTimelineCustomer => {
  if (
    !exact(value, ['id', 'name', 'code']) ||
    !identifier(value.id) ||
    value.id !== input.customerId ||
    !text(value.name, 300) ||
    value.name.length === 0 ||
    !text(value.code, 200) ||
    value.code.length === 0 ||
    /[\u0000-\u001f\u007f]/.test(value.name) ||
    /[\u0000-\u001f\u007f]/.test(value.code)
  )
    return failure('NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID');
  return { id: value.id, name: value.name, code: value.code };
};

function coverage(value: unknown): string[] {
  if (
    !Array.isArray(value) ||
    value.length > 12 ||
    !value.every((item) => typeof item === 'string' && coverageValues.has(item))
  )
    return failure('NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID');
  const unique = new Set(value);
  if (unique.size !== value.length) return failure('NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID');
  return value as string[];
}

function pageData(
  value: unknown,
  input: NativeCustomerTimelineReadInput,
): NativeCustomerTimelinePage {
  const limit = input.limit ?? 30;
  if (
    !exact(value, ['customer', 'items', 'hasMore', 'nextCursor', 'coverage', 'limitations']) ||
    !Array.isArray(value.items) ||
    typeof value.hasMore !== 'boolean' ||
    value.coverage === null
  )
    return failure('NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID');
  if (
    value.nextCursor !== null &&
    (!text(value.nextCursor, 1000) ||
      value.nextCursor.length === 0 ||
      /[\u0000-\u001f\u007f]/.test(value.nextCursor))
  )
    return failure('NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID');
  if (value.hasMore !== (value.nextCursor !== null))
    return failure('NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID');
  if (value.items.length > limit) return failure('NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID');
  if (value.limitations !== NATIVE_CUSTOMER_TIMELINE_LIMITATIONS)
    return failure('NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID');
  const seen = new Set<string>();
  let previous: { at: number; id: string } | null = null;
  for (const item of value.items) {
    if (!row(item)) return failure('NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID');
    if (seen.has(item.id)) return failure('NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID');
    if (
      previous &&
      (item.at > previous.at ||
        (item.at === previous.at && item.id >= previous.id))
    )
      return failure('NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID');
    previous = { at: item.at, id: item.id };
    seen.add(item.id);
  }
  return {
    customer: customer(value.customer, input),
    items: value.items as NativeCustomerTimelineItem[],
    hasMore: value.hasMore,
    nextCursor: value.nextCursor as string | null,
    coverage: coverage(value.coverage),
    limitations: value.limitations,
  };
}

function context(
  value: unknown,
  actor: NativeCustomerTimelinePageContext,
  input: NativeCustomerTimelineReadInput,
): Context {
  if (
    !exact(value, [
      'userId',
      'holdingId',
      'companyId',
      'branchId',
      'departmentId',
      'branchSlug',
      'departmentSlug',
      'assignmentId',
      'assignmentVersion',
      'revision',
      'role',
      'assignmentScopeKind',
      'modules',
      'permissions',
      'expiresAt',
    ])
  )
    return failure('NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID');
  const parsed = value as Context;
  if (
    parsed.userId !== actor.userId ||
    parsed.holdingId !== actor.holdingId ||
    parsed.companyId !== actor.companyId ||
    parsed.branchId !== actor.branchId ||
    parsed.departmentId !== actor.departmentId ||
    parsed.branchSlug !== (input.branchSlug ?? null) ||
    parsed.departmentSlug !== (input.departmentSlug ?? null) ||
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
    return failure('NATIVE_CUSTOMER_TIMELINE_CONTEXT_CHANGED');
  if (
    input.expected &&
    (parsed.companyId !== input.expected.companyId ||
      parsed.assignmentId !== input.expected.assignmentId ||
      parsed.revision !== input.expected.revision ||
      parsed.assignmentVersion !== input.expected.assignmentVersion)
  )
    return failure('NATIVE_CUSTOMER_TIMELINE_CONTEXT_CHANGED');
  return parsed;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativeCustomerTimelineConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_CUSTOMER_TIMELINE_CONFIGURATION_INVALID');
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
    !Array.isArray(raw.holdings) ||
    raw.holdings.length < 1 ||
    raw.holdings.length > 64
  )
    return failure('NATIVE_CUSTOMER_TIMELINE_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_CUSTOMER_TIMELINE_CONFIGURATION_INVALID');
    }
    const originPort = origin.port;
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      (originPort !== '' &&
        (!/^[1-9]\d{0,4}$/.test(originPort) ||
          Number(originPort) < 1 ||
          Number(originPort) > 65535)) ||
      origin.username ||
      origin.password ||
      !identifier(holding.holdingId) ||
      !text(holding.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug) ||
      holdings.has(origin.host)
    )
      return failure('NATIVE_CUSTOMER_TIMELINE_CONFIGURATION_INVALID');
    holdings.set(origin.host, {
      holdingId: holding.holdingId,
      holdingSlug: holding.holdingSlug,
    });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativeCustomerTimelineTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  input: NativeCustomerTimelineReadInput,
  actor: NativeCustomerTimelinePageContext,
  timeout: number,
  path: string,
  signal?: AbortSignal,
): Promise<Response> {
  const forwarded = incoming
    .get('x-forwarded-host')
    ?.split(',')[0]
    ?.trim();
  const host = forwarded && forwarded.length > 0 ? forwarded : incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== input.holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(new NativeCustomerTimelineError(
      'NATIVE_CUSTOMER_TIMELINE_AUTHORITY_INVALID',
    ));
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    'cache-control': 'no-cache',
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeCustomerTimelineError(
        'NATIVE_CUSTOMER_TIMELINE_COOKIE_LIMIT',
      ));
    headers.set('cookie', cookie);
  }
  for (const [key, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return Promise.reject(new NativeCustomerTimelineError(
          'NATIVE_CUSTOMER_TIMELINE_REQUEST_INVALID',
        ));
      headers.set(key, slug);
    }
  }
  if (input.expected) {
    if (
      !identifier(input.expected.companyId) ||
      !identifier(input.expected.assignmentId) ||
      !integer(input.expected.revision) ||
      !integer(input.expected.assignmentVersion, 1)
    )
      return Promise.reject(new NativeCustomerTimelineError(
        'NATIVE_CUSTOMER_TIMELINE_REQUEST_INVALID',
      ));
    headers.set('x-erpx-context-company', input.expected.companyId);
    headers.set('x-erpx-context-assignment', input.expected.assignmentId);
    headers.set('x-erpx-context-revision', String(input.expected.revision));
    headers.set('x-erpx-context-assignment-version', String(input.expected.assignmentVersion));
  }
  const url = new URL(path, base);
  if (input.cursor !== undefined) url.searchParams.set('cursor', input.cursor);
  if (input.limit !== undefined) url.searchParams.set('limit', String(input.limit));
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, timeout);
  return transport('GET', url, headers, undefined, controller.signal).finally(() => {
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
        return failure('NATIVE_CUSTOMER_TIMELINE_RESPONSE_LIMIT');
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
    return failure('NATIVE_CUSTOMER_TIMELINE_UNAVAILABLE', response.status);
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
    return failure(code ?? 'NATIVE_CUSTOMER_TIMELINE_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID');
  }
}

export function nativeCustomerTimelineAvailable(): boolean {
  return process.env.ERPX_NATIVE_CUSTOMER_TIMELINE_CONFIG !== undefined;
}

export function nativeCustomerTimelineConfiguration(
  raw: string,
): NativeCustomerTimelineConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeCustomerTimelineError('NATIVE_CUSTOMER_TIMELINE_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    !Array.isArray(value.holdings)
  )
    return failure('NATIVE_CUSTOMER_TIMELINE_CONFIGURATION_INVALID');
  if (
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) &&
        text(holding.origin) &&
        text(holding.holdingSlug) &&
        text(holding.holdingId),
    )
  )
    return failure('NATIVE_CUSTOMER_TIMELINE_CONFIGURATION_INVALID');
  return value as unknown as NativeCustomerTimelineConfiguration;
}

export function createNativeCustomerTimelineClient(
  raw: NativeCustomerTimelineConfiguration,
  transport: NativeCustomerTimelineTransport = directHttp,
) {
  const config = configuration(raw);
  const timeline = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeCustomerTimelinePageContext,
    input: NativeCustomerTimelineReadInput,
    signal?: AbortSignal,
  ): Promise<NativeCustomerTimelinePage> => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return failure('NATIVE_CUSTOMER_TIMELINE_CONTEXT_CHANGED');
    if (!customerIdentifier(input.customerId))
      return failure('NATIVE_CUSTOMER_TIMELINE_REQUEST_INVALID');
    if (
      (input.cursor !== undefined &&
        (input.cursor.length > 1000 ||
          input.cursor.length === 0 ||
          /[\u0000-\u001f\u007f]/.test(input.cursor))) ||
      (input.limit !== undefined && (!integer(input.limit, 1) || input.limit > 100))
    )
      return failure('NATIVE_CUSTOMER_TIMELINE_REQUEST_INVALID');
    const response = await transportCall(
      transport,
      config.base,
      config.holdings,
      incoming,
      input,
      actor,
      config.timeout,
      `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/customers/${encodeURIComponent(input.customerId)}/timeline`,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeCustomerTimelineError) throw error;
      return failure('NATIVE_CUSTOMER_TIMELINE_UNAVAILABLE');
    });
    const value = await boundedJson(response);
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID');
    context(value.context, actor, input);
    return pageData(value.data, input);
  };

  const directory = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeCustomerTimelinePageContext,
    input: NativeCustomerTimelineReadInput,
    signal?: AbortSignal,
  ) => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return failure('NATIVE_CUSTOMER_TIMELINE_CONTEXT_CHANGED');
    if (
      (input.cursor !== undefined &&
        (input.cursor.length > 1000 ||
          input.cursor.length === 0 ||
          /[\u0000-\u001f\u007f]/.test(input.cursor))) ||
      (input.limit !== undefined && (!integer(input.limit, 1) || input.limit > 200))
    )
      return failure('NATIVE_CUSTOMER_TIMELINE_REQUEST_INVALID');
    const response = await transportCall(
      transport,
      config.base,
      config.holdings,
      incoming,
      { ...input, customerId: '' },
      actor,
      config.timeout,
      `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/customers`,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeCustomerTimelineError) throw error;
      return failure('NATIVE_CUSTOMER_TIMELINE_UNAVAILABLE');
    });
    const value = await boundedJson(response);
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID');
    context(value.context, actor, input);
    const data = value.data as { items?: unknown; hasMore?: unknown; nextCursor?: unknown };
    if (!record(data) || !Array.isArray(data.items)) return failure('NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID');
    return data as { items: Array<{ id: string; name: string; code?: string | null }>; hasMore: boolean; nextCursor: string | null };
  };

  return { timeline, directory };
}

/** Fresh bounded socket; bypasses patched fetch/HMR and ambient proxies. */
export function directHttp(
  method: 'GET',
  url: URL,
  headers: Headers,
  body: undefined,
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
        method,
        agent,
        headers: { ...Object.fromEntries(headers) },
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
    outgoing.end();
  });
}
