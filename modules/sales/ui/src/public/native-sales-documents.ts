import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';

/** Rendering input from retained page authorization, never a grant. */
export type NativeSalesDocumentsPageContext = {
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

export type NativeSalesDocumentsConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;
type Context = Row & { modules: string[]; permissions: string[] };

export type NativeSalesDocumentsKind = 'quote' | 'sales';

export type NativeSalesDocumentTransitionAction =
  | 'submit'
  | 'approve'
  | 'return'
  | 'cancel'
  | 'convert';

const transitionActions = new Set<NativeSalesDocumentTransitionAction>([
  'submit',
  'approve',
  'return',
  'cancel',
  'convert',
]);

/** Sales-owned selector for the existing Commerce public order port.
 * Quotation actions keep Commerce's independent approval and fulfillment
 * guards. Sales approval and confirmed sales cancellation stay retained behind
 * their Finance/Inventory guards, so they are not selected here. */
export function nativeSalesDocumentTransitionEligible(
  kind: string,
  state: string,
  action: NativeSalesDocumentTransitionAction,
): boolean {
  if (!transitionActions.has(action)) return false;
  if (kind === 'quote') return true;
  return (
    kind === 'sales' &&
    (action === 'submit' || action === 'return' || (action === 'cancel' && state === 'draft'))
  );
}

/** Sales-owned selector for the native confirmed-cancellation command. Draft
 * cancellation keeps the Commerce port; confirmed sales cancellation is the
 * one action this Sales ingress owns. */
export function nativeSalesConfirmedCancellationEligible(
  kind: string,
  state: string,
): boolean {
  return kind === 'sales' && state === 'confirmed';
}

export type NativeSalesDocumentsReadInput = Readonly<{
  holdingSlug: string;
  kind: NativeSalesDocumentsKind;
  search?: string;
  status?: string;
  cursor?: string;
  branchSlug?: string;
  departmentSlug?: string;
  expected?: Readonly<{
    companyId: string;
    assignmentId: string;
    revision: number;
    assignmentVersion: number;
  }>;
}>;

export type NativeSalesDocumentRow = Readonly<{
  id: string;
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  kind: NativeSalesDocumentsKind;
  number: string;
  party_id: string;
  party_name: string;
  state:
    | 'draft'
    | 'submitted'
    | 'confirmed'
    | 'partially_fulfilled'
    | 'fulfilled'
    | 'converted'
    | 'cancelled';
  order_date: string;
  due_date: string;
  currency: string;
  subtotal_minor: number;
  tax_minor: number;
  total_minor: number;
  version: number;
  created_at: number;
}>;

export type NativeSalesDocumentsPage = Readonly<{
  items: NativeSalesDocumentRow[];
  nextCursor: string | null;
  previousCursor: null;
  hasMore: boolean;
}>;

export type NativeSalesDocumentsTransport = (
  method: 'GET',
  url: URL,
  headers: Headers,
  body: undefined,
  signal: AbortSignal,
) => Promise<Response>;

export type NativeSalesCancellationTransport = (
  method: 'POST',
  url: URL,
  headers: Headers,
  body: string,
  signal: AbortSignal,
) => Promise<Response>;

export type NativeSalesCancellationInput = Readonly<{
  holdingSlug: string;
  expectedVersion: number;
  branchSlug?: string;
  departmentSlug?: string;
  expected?: Readonly<{
    companyId: string;
    assignmentId: string;
    revision: number;
    assignmentVersion: number;
  }>;
}>;

export type NativeSalesCancellationReceipt = Readonly<{
  id: string;
  state: 'cancelled';
  version: number;
  salesId: null;
}>;

export class NativeSalesDocumentsError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Sales request could not be completed.');
    this.name = 'NativeSalesDocumentsError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeSalesDocumentsError(code, status, message);
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
  text(value, 200) && value.length > 0;
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const optionalText = (value: unknown, max = 4096) =>
  value === null || text(value, max);
const texts = (value: unknown, max = 256): value is string[] =>
  Array.isArray(value) && value.length <= max && value.every((item) => text(item, 256));
const sameSet = (left: readonly string[], right: readonly string[]) => {
  const a = new Set(left);
  const b = new Set(right);
  return a.size === b.size && [...a].every((value) => b.has(value));
};
const states = new Set([
  'draft',
  'submitted',
  'confirmed',
  'partially_fulfilled',
  'fulfilled',
  'converted',
  'cancelled',
]);
const statuses = states;
const kinds = new Set(['quote', 'sales']);

const date = (value: unknown): value is string => {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const [year, month, day] = value.split('-').map(Number) as [number, number, number];
  if (year < 1 || month < 1 || month > 12 || day < 1) return false;
  const leap = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0);
  const days = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][month - 1]!;
  return day <= days;
};

const row = (
  value: unknown,
  input: NativeSalesDocumentsReadInput,
  actor: NativeSalesDocumentsPageContext,
): value is NativeSalesDocumentRow => {
  if (
    !exact(value, [
      'id', 'company_id', 'branch_id', 'department_id', 'kind', 'number',
      'party_id', 'party_name', 'state', 'order_date', 'due_date', 'currency',
      'subtotal_minor', 'tax_minor', 'total_minor', 'version', 'created_at',
    ])
  )
    return false;
  if (
    !identifier(value.id) ||
    !identifier(value.company_id) ||
    !optionalText(value.branch_id, 200) ||
    !optionalText(value.department_id, 200) ||
    typeof value.kind !== 'string' ||
    !kinds.has(value.kind) ||
    !text(value.number, 100) ||
    value.number.length === 0 ||
    !identifier(value.party_id) ||
    !text(value.party_name, 300) ||
    value.party_name.length === 0 ||
    typeof value.state !== 'string' ||
    !states.has(value.state) ||
    !date(value.order_date) ||
    !date(value.due_date) ||
    typeof value.currency !== 'string' ||
    !/^[A-Z]{3}$/.test(value.currency) ||
    !integer(value.subtotal_minor) ||
    !integer(value.tax_minor) ||
    !integer(value.total_minor) ||
    value.total_minor !== value.subtotal_minor + value.tax_minor ||
    !integer(value.version, 1) ||
    !integer(value.created_at)
  )
    return false;
  if (
    value.company_id !== actor.companyId ||
    value.kind !== input.kind ||
    (actor.branchId !== null && value.branch_id !== actor.branchId) ||
    (actor.departmentId !== null && value.department_id !== actor.departmentId)
  )
    return false;
  return true;
};

function pageData(
  value: unknown,
  input: NativeSalesDocumentsReadInput,
  actor: NativeSalesDocumentsPageContext,
): NativeSalesDocumentsPage {
  if (!exact(value, ['items', 'nextCursor', 'hasMore']) || !Array.isArray(value.items))
    return failure('NATIVE_SALES_DOCUMENTS_RESPONSE_INVALID');
  if (typeof value.hasMore !== 'boolean') return failure('NATIVE_SALES_DOCUMENTS_RESPONSE_INVALID');
  if (value.nextCursor !== null && (!text(value.nextCursor, 4096) || value.nextCursor.length === 0))
    return failure('NATIVE_SALES_DOCUMENTS_RESPONSE_INVALID');
  if (value.hasMore !== (value.nextCursor !== null))
    return failure('NATIVE_SALES_DOCUMENTS_RESPONSE_INVALID');
  if (value.items.length > 30) return failure('NATIVE_SALES_DOCUMENTS_RESPONSE_INVALID');
  const seen = new Set<string>();
  for (const item of value.items) {
    if (!row(item, input, actor)) return failure('NATIVE_SALES_DOCUMENTS_RESPONSE_INVALID');
    if (seen.has(item.id)) return failure('NATIVE_SALES_DOCUMENTS_RESPONSE_INVALID');
    seen.add(item.id);
  }
  return {
    items: value.items as NativeSalesDocumentRow[],
    nextCursor: value.nextCursor as string | null,
    previousCursor: null,
    hasMore: value.hasMore,
  };
}

function context(
  value: unknown,
  actor: NativeSalesDocumentsPageContext,
  input: NativeSalesDocumentsReadInput,
): Context {
  if (
    !exact(value, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug',
      'departmentSlug', 'assignmentId', 'assignmentVersion', 'revision', 'role',
      'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ])
  )
    return failure('NATIVE_SALES_DOCUMENTS_RESPONSE_INVALID');
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
    return failure('NATIVE_SALES_DOCUMENTS_CONTEXT_CHANGED');
  if (
    input.expected &&
    (parsed.companyId !== input.expected.companyId ||
      parsed.assignmentId !== input.expected.assignmentId ||
      parsed.revision !== input.expected.revision ||
      parsed.assignmentVersion !== input.expected.assignmentVersion)
  )
    return failure('NATIVE_SALES_DOCUMENTS_CONTEXT_CHANGED');
  return parsed;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativeSalesDocumentsConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_SALES_DOCUMENTS_CONFIGURATION_INVALID');
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
    return failure('NATIVE_SALES_DOCUMENTS_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_SALES_DOCUMENTS_CONFIGURATION_INVALID');
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
      return failure('NATIVE_SALES_DOCUMENTS_CONFIGURATION_INVALID');
    holdings.set(origin.host, { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativeSalesDocumentsTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  input: NativeSalesDocumentsReadInput,
  actor: NativeSalesDocumentsPageContext,
  timeout: number,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== input.holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(new NativeSalesDocumentsError('NATIVE_SALES_DOCUMENTS_AUTHORITY_INVALID'));
  const headers = new Headers({ host: host!, accept: 'application/json', 'cache-control': 'no-cache' });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeSalesDocumentsError('NATIVE_SALES_DOCUMENTS_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  for (const [key, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return Promise.reject(new NativeSalesDocumentsError('NATIVE_SALES_DOCUMENTS_REQUEST_INVALID'));
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
      return Promise.reject(new NativeSalesDocumentsError('NATIVE_SALES_DOCUMENTS_REQUEST_INVALID'));
    headers.set('x-erpx-context-company', input.expected.companyId);
    headers.set('x-erpx-context-assignment', input.expected.assignmentId);
    headers.set('x-erpx-context-revision', String(input.expected.revision));
    headers.set('x-erpx-context-assignment-version', String(input.expected.assignmentVersion));
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/sales/documents`,
    base,
  );
  url.searchParams.set('kind', input.kind);
  if (input.search !== undefined) url.searchParams.set('q', input.search);
  if (input.status !== undefined) url.searchParams.set('status', input.status);
  if (input.cursor !== undefined) url.searchParams.set('cursor', input.cursor);
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
        return failure('NATIVE_SALES_DOCUMENTS_RESPONSE_LIMIT');
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
    return failure('NATIVE_SALES_DOCUMENTS_UNAVAILABLE', response.status);
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
    return failure(code ?? 'NATIVE_SALES_DOCUMENTS_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_SALES_DOCUMENTS_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_SALES_DOCUMENTS_RESPONSE_INVALID');
  }
}

export function nativeSalesDocumentsAvailable(): boolean {
  return process.env.ERPX_NATIVE_SALES_DOCUMENTS_CONFIG !== undefined;
}

export function nativeSalesDocumentsConfiguration(raw: string): NativeSalesDocumentsConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeSalesDocumentsError('NATIVE_SALES_DOCUMENTS_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    !Array.isArray(value.holdings)
  )
    return failure('NATIVE_SALES_DOCUMENTS_CONFIGURATION_INVALID');
  if (
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) &&
        text(holding.origin) &&
        text(holding.holdingSlug) &&
        text(holding.holdingId),
    )
  )
    return failure('NATIVE_SALES_DOCUMENTS_CONFIGURATION_INVALID');
  return value as unknown as NativeSalesDocumentsConfiguration;
}

export function createNativeSalesDocumentsClient(
  raw: NativeSalesDocumentsConfiguration,
  transport: NativeSalesDocumentsTransport = directHttp,
) {
  const config = configuration(raw);
  const documents = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeSalesDocumentsPageContext,
    input: NativeSalesDocumentsReadInput,
    signal?: AbortSignal,
  ): Promise<NativeSalesDocumentsPage> => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return failure('NATIVE_SALES_DOCUMENTS_CONTEXT_CHANGED');
    if (!kinds.has(input.kind))
      return failure('NATIVE_SALES_DOCUMENTS_REQUEST_INVALID');
    if (
      (input.search !== undefined &&
        (input.search.length > 200 || /[\u0000-\u001f\u007f]/.test(input.search))) ||
      (input.status !== undefined &&
        input.status !== '' &&
        !statuses.has(input.status)) ||
      (input.cursor !== undefined &&
        (input.cursor.length > 4096 || input.cursor.length === 0))
    )
      return failure('NATIVE_SALES_DOCUMENTS_REQUEST_INVALID');
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
      if (error instanceof NativeSalesDocumentsError) throw error;
      return failure('NATIVE_SALES_DOCUMENTS_UNAVAILABLE');
    });
    const value = await boundedJson(response);
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_SALES_DOCUMENTS_RESPONSE_INVALID');
    context(value.context, actor, input);
    return pageData(value.data, input, actor);
  };
  return { documents };
}

function cancellationTransportCall(
  transport: NativeSalesCancellationTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  orderId: string,
  input: NativeSalesCancellationInput,
  actor: NativeSalesDocumentsPageContext,
  key: string,
  timeout: number,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== input.holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(new NativeSalesDocumentsError('NATIVE_SALES_DOCUMENTS_AUTHORITY_INVALID'));
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
      return Promise.reject(new NativeSalesDocumentsError('NATIVE_SALES_DOCUMENTS_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  const origin = incoming.get('origin');
  if (origin) {
    if (!text(origin, 2048))
      return Promise.reject(new NativeSalesDocumentsError('NATIVE_SALES_DOCUMENTS_REQUEST_INVALID'));
    headers.set('origin', origin);
  }
  for (const [name, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return Promise.reject(new NativeSalesDocumentsError('NATIVE_SALES_DOCUMENTS_REQUEST_INVALID'));
      headers.set(name, slug);
    }
  }
  if (input.expected) {
    if (
      !identifier(input.expected.companyId) ||
      !identifier(input.expected.assignmentId) ||
      !integer(input.expected.revision) ||
      !integer(input.expected.assignmentVersion, 1)
    )
      return Promise.reject(new NativeSalesDocumentsError('NATIVE_SALES_DOCUMENTS_REQUEST_INVALID'));
    headers.set('x-erpx-context-company', input.expected.companyId);
    headers.set('x-erpx-context-assignment', input.expected.assignmentId);
    headers.set('x-erpx-context-revision', String(input.expected.revision));
    headers.set('x-erpx-context-assignment-version', String(input.expected.assignmentVersion));
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/sales/orders/${encodeURIComponent(orderId)}/cancel`,
    base,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, timeout);
  return transport(
    'POST',
    url,
    headers,
    JSON.stringify({ expectedVersion: input.expectedVersion }),
    controller.signal,
  ).finally(() => {
    clearTimeout(timer);
    signal?.removeEventListener('abort', cancel);
  });
}

function cancellationData(
  value: unknown,
  actor: NativeSalesDocumentsPageContext,
  orderId: string,
  input: NativeSalesCancellationInput,
): NativeSalesCancellationReceipt {
  if (!exact(value, ['id', 'state', 'version', 'salesId']))
    return failure('NATIVE_SALES_DOCUMENTS_RESPONSE_INVALID');
  if (
    value.id !== orderId ||
    !identifier(value.id) ||
    value.state !== 'cancelled' ||
    !integer(value.version, input.expectedVersion + 1) ||
    !integer(value.version, 2) ||
    value.salesId !== null ||
    actor.holdingSlug !== input.holdingSlug
  )
    return failure('NATIVE_SALES_DOCUMENTS_RESPONSE_INVALID');
  return value as NativeSalesCancellationReceipt;
}

export function nativeSalesOrderCancellationAvailable(): boolean {
  return process.env.ERPX_NATIVE_SALES_CANCELLATION_CONFIG !== undefined;
}

export function nativeSalesOrderCancellationConfiguration(
  raw: string,
): NativeSalesDocumentsConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeSalesDocumentsError('NATIVE_SALES_DOCUMENTS_CONFIGURATION_INVALID');
  }
  return nativeSalesDocumentsConfiguration(JSON.stringify(value));
}

export function createNativeSalesCancellationClient(
  raw: NativeSalesDocumentsConfiguration,
  transport: NativeSalesCancellationTransport = directPostHttp,
) {
  const config = configuration(raw);
  const cancelOrder = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeSalesDocumentsPageContext,
    orderId: string,
    input: NativeSalesCancellationInput,
    key: string,
    signal?: AbortSignal,
  ): Promise<NativeSalesCancellationReceipt> => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return failure('NATIVE_SALES_DOCUMENTS_CONTEXT_CHANGED');
    if (
      !identifier(orderId) ||
      !integer(input.expectedVersion, 1) ||
      !integer(input.expectedVersion) ||
      !text(key, 180) ||
      !/^[A-Za-z0-9:_.-]{8,180}$/.test(key)
    )
      return failure('NATIVE_SALES_DOCUMENTS_REQUEST_INVALID');
    const contextInput: NativeSalesDocumentsReadInput = {
      holdingSlug: input.holdingSlug,
      kind: 'sales',
      branchSlug: input.branchSlug,
      departmentSlug: input.departmentSlug,
      expected: input.expected,
    };
    const response = await cancellationTransportCall(
      transport,
      config.base,
      config.holdings,
      incoming,
      orderId,
      input,
      actor,
      key,
      config.timeout,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeSalesDocumentsError) throw error;
      return failure('NATIVE_SALES_DOCUMENTS_UNAVAILABLE');
    });
    const value = await boundedJson(response);
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_SALES_DOCUMENTS_RESPONSE_INVALID');
    context(value.context, actor, contextInput);
    return cancellationData(value.data, actor, orderId, input);
  };
  return { cancel: cancelOrder };
}

/** Fresh bounded socket for one command. Bypasses patched fetch/HMR and
 * ambient proxies. */
export function directPostHttp(
  method: 'POST',
  url: URL,
  headers: Headers,
  body: string,
  signal: AbortSignal,
): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({ keepAlive: false, maxSockets: 1, maxTotalSockets: 1, proxyEnv: {} as import('node:http').ProxyEnv });
    const outgoing = request(
      url,
      { method, agent, headers: { ...Object.fromEntries(headers) }, signal, maxHeaderSize: 8192 },
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
    outgoing.end(body);
  });
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
    const agent = new Agent({ keepAlive: false, maxSockets: 1, maxTotalSockets: 1, proxyEnv: {} as import('node:http').ProxyEnv });
    const outgoing = request(
      url,
      { method, agent, headers: { ...Object.fromEntries(headers) }, signal, maxHeaderSize: 8192 },
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
