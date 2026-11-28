import 'server-only';
import { directHttp } from './native-sales-documents';

/** Rendering input from retained page authorization, never a grant. */
export type NativeEcommerceCatalogPageContext = {
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

export type NativeEcommerceCatalogConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type UnknownRow = Record<string, unknown>;
export type NativeEcommerceCatalogRow = Record<string, string | number | null>;
export type NativeEcommerceCatalog = Readonly<{
  channels: NativeEcommerceCatalogRow[];
  listings: NativeEcommerceCatalogRow[];
}>;

export type NativeEcommerceCatalogReadInput = Readonly<{
  holdingSlug: string;
  branchSlug?: string;
  departmentSlug?: string;
  expected: Readonly<{
    companyId: string;
    assignmentId: string;
    revision: number;
    assignmentVersion: number;
  }>;
}>;

export type NativeEcommerceCatalogTransport = (
  method: 'GET',
  url: URL,
  headers: Headers,
  body: undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeEcommerceCatalogError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Ecommerce catalog request could not be completed.');
    this.name = 'NativeEcommerceCatalogError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeEcommerceCatalogError(code, status, message);
};
const record = (value: unknown): value is UnknownRow =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is UnknownRow =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string =>
  text(value, 200) && value.length > 0 && !/[\u0000-\u001f\u007f]/.test(value);
const optionalText = (value: unknown, max = 4096) =>
  value === null || text(value, max);
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const texts = (value: unknown, max = 256): value is string[] =>
  Array.isArray(value) && value.length <= max && value.every((item) => text(item, 256));
const sameSet = (left: readonly string[], right: readonly string[]) => {
  const a = new Set(left);
  const b = new Set(right);
  return a.size === b.size && [...a].every((value) => b.has(value));
};
const channelTypes = new Set(['web', 'marketplace', 'social']);
const channelStatuses = new Set(['draft', 'active', 'rejected', 'retired']);
const listingStatuses = new Set(['draft', 'published', 'rejected', 'retired']);
const CHANNEL_KEYS = [
  'id', 'company_id', 'name', 'channel_type', 'configuration_json', 'status',
  'created_by', 'reviewed_by', 'version', 'created_at', 'updated_at',
] as const;
const LISTING_KEYS = [
  'id', 'channel_id', 'product_id', 'title', 'currency', 'price_minor',
  'status', 'created_by', 'reviewed_by', 'version', 'created_at', 'updated_at',
] as const;

const configurationObject = (value: unknown) => {
  if (typeof value !== 'string' || new TextEncoder().encode(value).length > 16 * 1024)
    return false;
  try {
    const parsed: unknown = JSON.parse(value);
    return record(parsed);
  } catch {
    return false;
  }
};

const timestamps = (row: UnknownRow) =>
  integer(row.created_at) &&
  integer(row.updated_at) &&
  Number(row.updated_at) >= Number(row.created_at);

const channelRow = (value: unknown, actor: NativeEcommerceCatalogPageContext): value is NativeEcommerceCatalogRow => {
  if (!exact(value, CHANNEL_KEYS)) return false;
  const row = value;
  if (
    !identifier(row.id) ||
    !identifier(row.company_id) ||
    row.company_id !== actor.companyId ||
    !text(row.name, 160) ||
    row.name.trim().length === 0 ||
    typeof row.channel_type !== 'string' ||
    !channelTypes.has(row.channel_type) ||
    !configurationObject(row.configuration_json) ||
    typeof row.status !== 'string' ||
    !channelStatuses.has(row.status) ||
    !identifier(row.created_by) ||
    !optionalText(row.reviewed_by, 200) ||
    !integer(row.version, 1) ||
    !timestamps(row)
  )
    return false;
  return true;
};

const listingRow = (value: unknown, channelIds: ReadonlySet<string>): value is NativeEcommerceCatalogRow => {
  if (!exact(value, LISTING_KEYS)) return false;
  const row = value;
  if (
    !identifier(row.id) ||
    !identifier(row.channel_id) ||
    !channelIds.has(row.channel_id) ||
    !identifier(row.product_id) ||
    !text(row.title, 200) ||
    row.title.trim().length === 0 ||
    typeof row.currency !== 'string' ||
    !/^[A-Z]{3}$/.test(row.currency) ||
    !integer(row.price_minor) ||
    typeof row.status !== 'string' ||
    !listingStatuses.has(row.status) ||
    !identifier(row.created_by) ||
    !optionalText(row.reviewed_by, 200) ||
    !integer(row.version, 1) ||
    !timestamps(row)
  )
    return false;
  return true;
};

const ordered = (rows: readonly NativeEcommerceCatalogRow[]) =>
  rows.every((row, index) => {
    if (index === 0) return true;
    const previous = rows[index - 1]!;
    const previousKey = [Number(previous.updated_at), String(previous.id)] as const;
    const currentKey = [Number(row.updated_at), String(row.id)] as const;
    return (
      previousKey[0] > currentKey[0] ||
      (previousKey[0] === currentKey[0] && previousKey[1] > currentKey[1])
    );
  });

function catalogData(value: unknown, actor: NativeEcommerceCatalogPageContext): NativeEcommerceCatalog {
  if (!exact(value, ['channels', 'listings'])) return failure('NATIVE_ECOMMERCE_CATALOG_RESPONSE_INVALID');
  if (!Array.isArray(value.channels) || value.channels.length > 500) return failure('NATIVE_ECOMMERCE_CATALOG_RESPONSE_INVALID');
  if (!Array.isArray(value.listings) || value.listings.length > 500) return failure('NATIVE_ECOMMERCE_CATALOG_RESPONSE_INVALID');
  const channelIds = new Set<string>();
  for (const channel of value.channels) {
    if (!channelRow(channel, actor) || channelIds.has(channel.id as string)) return failure('NATIVE_ECOMMERCE_CATALOG_RESPONSE_INVALID');
    channelIds.add(channel.id as string);
  }
  const listingIds = new Set<string>();
  const products = new Set<string>();
  for (const listing of value.listings) {
    if (!listingRow(listing, channelIds) || listingIds.has(listing.id as string)) return failure('NATIVE_ECOMMERCE_CATALOG_RESPONSE_INVALID');
    const pair = `${listing.channel_id!}\u0000${listing.product_id!}`;
    if (products.has(pair)) return failure('NATIVE_ECOMMERCE_CATALOG_RESPONSE_INVALID');
    listingIds.add(listing.id as string);
    products.add(pair);
  }
  const channels = value.channels as NativeEcommerceCatalogRow[];
  const listings = value.listings as NativeEcommerceCatalogRow[];
  if (!ordered(channels) || !ordered(listings)) return failure('NATIVE_ECOMMERCE_CATALOG_RESPONSE_INVALID');
  return { channels, listings };
}

function context(
  value: unknown,
  actor: NativeEcommerceCatalogPageContext,
  input: NativeEcommerceCatalogReadInput,
): UnknownRow {
  if (
    !exact(value, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug',
      'departmentSlug', 'assignmentId', 'assignmentVersion', 'revision', 'role',
      'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ])
  )
    return failure('NATIVE_ECOMMERCE_CATALOG_RESPONSE_INVALID');
  const parsed = value;
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
    return failure('NATIVE_ECOMMERCE_CATALOG_CONTEXT_CHANGED');
  if (
    parsed.companyId !== input.expected.companyId ||
    parsed.assignmentId !== input.expected.assignmentId ||
    parsed.revision !== input.expected.revision ||
    parsed.assignmentVersion !== input.expected.assignmentVersion
  )
    return failure('NATIVE_ECOMMERCE_CATALOG_CONTEXT_CHANGED');
  return parsed;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativeEcommerceCatalogConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_ECOMMERCE_CATALOG_CONFIGURATION_INVALID');
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
    return failure('NATIVE_ECOMMERCE_CATALOG_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_ECOMMERCE_CATALOG_CONFIGURATION_INVALID');
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
      return failure('NATIVE_ECOMMERCE_CATALOG_CONFIGURATION_INVALID');
    holdings.set(origin.host, { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativeEcommerceCatalogTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  input: NativeEcommerceCatalogReadInput,
  actor: NativeEcommerceCatalogPageContext,
  timeout: number,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== input.holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(new NativeEcommerceCatalogError('NATIVE_ECOMMERCE_CATALOG_AUTHORITY_INVALID'));
  const headers = new Headers({ host: host!, accept: 'application/json', 'cache-control': 'no-cache' });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeEcommerceCatalogError('NATIVE_ECOMMERCE_CATALOG_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  for (const [key, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return Promise.reject(new NativeEcommerceCatalogError('NATIVE_ECOMMERCE_CATALOG_REQUEST_INVALID'));
      headers.set(key, slug);
    }
  }
  if (
    !identifier(input.expected.companyId) ||
    !identifier(input.expected.assignmentId) ||
    !integer(input.expected.revision) ||
    !integer(input.expected.assignmentVersion, 1)
  )
    return Promise.reject(new NativeEcommerceCatalogError('NATIVE_ECOMMERCE_CATALOG_REQUEST_INVALID'));
  headers.set('x-erpx-context-company', input.expected.companyId);
  headers.set('x-erpx-context-assignment', input.expected.assignmentId);
  headers.set('x-erpx-context-revision', String(input.expected.revision));
  headers.set('x-erpx-context-assignment-version', String(input.expected.assignmentVersion));
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/ecommerce/catalog`, base);
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
        return failure('NATIVE_ECOMMERCE_CATALOG_RESPONSE_LIMIT');
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
    return failure('NATIVE_ECOMMERCE_CATALOG_UNAVAILABLE', response.status);
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
    return failure(code ?? 'NATIVE_ECOMMERCE_CATALOG_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_ECOMMERCE_CATALOG_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_ECOMMERCE_CATALOG_RESPONSE_INVALID');
  }
}

export function nativeEcommerceCatalogAvailable(): boolean {
  return process.env.ERPX_NATIVE_ECOMMERCE_CATALOG_CONFIG !== undefined;
}

export function nativeEcommerceCatalogConfiguration(raw: string): NativeEcommerceCatalogConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeEcommerceCatalogError('NATIVE_ECOMMERCE_CATALOG_CONFIGURATION_INVALID');
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
    return failure('NATIVE_ECOMMERCE_CATALOG_CONFIGURATION_INVALID');
  return value as unknown as NativeEcommerceCatalogConfiguration;
}

export function createNativeEcommerceCatalogClient(
  raw: NativeEcommerceCatalogConfiguration,
  transport: NativeEcommerceCatalogTransport = directHttp,
) {
  const config = configuration(raw);
  const catalog = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeEcommerceCatalogPageContext,
    input: NativeEcommerceCatalogReadInput,
    signal?: AbortSignal,
  ): Promise<NativeEcommerceCatalog> => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return failure('NATIVE_ECOMMERCE_CATALOG_CONTEXT_CHANGED');
    if (!identifier(input.holdingSlug))
      return failure('NATIVE_ECOMMERCE_CATALOG_REQUEST_INVALID');
    const response = await transportCall(
      transport,
      config.base,
      config.holdings,
      incoming,
      input,
      actor,
      config.timeout,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeEcommerceCatalogError) throw error;
      return failure('NATIVE_ECOMMERCE_CATALOG_UNAVAILABLE');
    });
    const value = await boundedJson(response);
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_ECOMMERCE_CATALOG_RESPONSE_INVALID');
    context(value.context, actor, input);
    return catalogData(value.data, actor);
  };
  return { catalog };
}
