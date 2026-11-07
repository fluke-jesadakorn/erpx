import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';
import type {
  NativeOpenPromiseRow,
  NativeSupplierPromiseHistory,
  NativeSupplierPromiseHistoryRow,
  NativeSupplierPromiseLineRow,
} from '../lib/native-supplier-promise-history-types';

/** Rendering input from retained page authorization, never a grant. */
export type NativeProcurementPageContext = {
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

export type NativeProcurementConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;
type Context = Row & { modules: string[]; permissions: string[] };

export type NativeSupplierPromiseHistoryReadInput = Readonly<{
  holdingSlug: string;
  branchSlug?: string;
  departmentSlug?: string;
  expected?: Readonly<{
    companyId: string;
    assignmentId: string;
    revision: number;
    assignmentVersion: number;
  }>;
}>;

export type NativeProcurementTransport = (
  method: 'GET',
  url: URL,
  headers: Headers,
  body: undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeProcurementError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Procurement request could not be completed.');
    this.name = 'NativeProcurementError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeProcurementError(code, status, message);
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
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const optionalIdentifier = (value: unknown) =>
  value === null || (identifier(value) && !/[\u0000-\u001f\u007f]/.test(value));
const date = (value: unknown): value is string => {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const month = Number(value.slice(5, 7));
  const day = Number(value.slice(8, 10));
  return month >= 1 && month <= 12 && day >= 1 && day <= 31;
};
const currency = (value: unknown): value is string =>
  typeof value === 'string' && /^[A-Z]{3}$/.test(value);
const trimmed = (value: unknown, minimum: number, maximum: number): value is string =>
  text(value, maximum) &&
  [...value].length >= minimum &&
  value.trim() === value;
const kinds = new Set(['confirmed', 'withdrawn']);
const orderStates = new Set([
  'draft',
  'submitted',
  'confirmed',
  'partially_fulfilled',
  'fulfilled',
  'converted',
  'cancelled',
]);

const historyRow = (value: unknown): value is NativeSupplierPromiseHistoryRow =>
  exact(value, [
    'id', 'order_id', 'company_id', 'order_version', 'supplier_id', 'currency',
    'required_date', 'kind', 'confirmation_date', 'supplier_reference', 'evidence',
    'recorded_by', 'created_at',
  ]) &&
  identifier(value.id) &&
  identifier(value.order_id) &&
  identifier(value.company_id) &&
  integer(value.order_version, 1) &&
  identifier(value.supplier_id) &&
  currency(value.currency) &&
  date(value.required_date) &&
  typeof value.kind === 'string' &&
  kinds.has(value.kind) &&
  date(value.confirmation_date) &&
  trimmed(value.supplier_reference, 3, 160) &&
  trimmed(value.evidence, 5, 2_000) &&
  identifier(value.recorded_by) &&
  integer(value.created_at);

const lineRow = (value: unknown): value is NativeSupplierPromiseLineRow =>
  exact(value, [
    'id', 'promise_id', 'order_line_id', 'ordinal', 'quantity_milli',
    'fulfilled_snapshot_milli', 'promised_date',
  ]) &&
  identifier(value.id) &&
  identifier(value.promise_id) &&
  identifier(value.order_line_id) &&
  integer(value.ordinal) &&
  value.ordinal <= 1_000_000 &&
  integer(value.quantity_milli, 1) &&
  integer(value.fulfilled_snapshot_milli) &&
  date(value.promised_date);

const openRow = (value: unknown): value is NativeOpenPromiseRow =>
  exact(value, [
    'id', 'promise_id', 'order_id', 'company_id', 'branch_id', 'department_id',
    'order_number', 'order_state', 'order_line_id', 'ordinal', 'quantity_milli',
    'fulfilled_snapshot_milli', 'promised_date', 'product_id', 'description',
    'fulfilled_milli', 'prior_quantity', 'remaining_milli',
  ]) &&
  identifier(value.id) &&
  identifier(value.promise_id) &&
  identifier(value.order_id) &&
  identifier(value.company_id) &&
  identifier(value.branch_id) &&
  optionalIdentifier(value.department_id) &&
  trimmed(value.order_number, 1, 200) &&
  typeof value.order_state === 'string' &&
  orderStates.has(value.order_state) &&
  identifier(value.order_line_id) &&
  integer(value.ordinal) &&
  value.ordinal <= 1_000_000 &&
  integer(value.quantity_milli, 1) &&
  integer(value.fulfilled_snapshot_milli) &&
  date(value.promised_date) &&
  identifier(value.product_id) &&
  trimmed(value.description, 1, 2_000) &&
  integer(value.fulfilled_milli) &&
  integer(value.prior_quantity) &&
  integer(value.remaining_milli) &&
  value.remaining_milli <= value.quantity_milli;

function historyData(value: unknown, orderId: string, companyId: string): NativeSupplierPromiseHistory {
  if (!exact(value, ['order_id', 'history', 'lines', 'open']) || value.order_id !== orderId)
    return failure('NATIVE_PROCUREMENT_RESPONSE_INVALID');
  if (
    !Array.isArray(value.history) ||
    value.history.length > 1_000 ||
    !value.history.every(historyRow) ||
    !Array.isArray(value.lines) ||
    value.lines.length > 10_000 ||
    !value.lines.every(lineRow) ||
    !Array.isArray(value.open) ||
    value.open.length > 10_000 ||
    !value.open.every(openRow)
  )
    return failure('NATIVE_PROCUREMENT_RESPONSE_INVALID');

  const promises = new Map<string, NativeSupplierPromiseHistoryRow>();
  for (const row of value.history) {
    if (row.order_id !== orderId || row.company_id !== companyId || promises.has(row.id))
      return failure('NATIVE_PROCUREMENT_RESPONSE_INVALID');
    promises.set(row.id, row);
  }
  const lines = new Map<string, NativeSupplierPromiseLineRow>();
  for (const row of value.lines) {
    const promise = promises.get(row.promise_id);
    if (
      !promise ||
      promise.kind !== 'confirmed' ||
      lines.has(row.id) ||
      row.promised_date < promise.confirmation_date
    )
      return failure('NATIVE_PROCUREMENT_RESPONSE_INVALID');
    lines.set(row.id, row);
  }
  const open = new Set<string>();
  for (const row of value.open) {
    const promise = promises.get(row.promise_id);
    const line = lines.get(row.id);
    if (
      !promise ||
      !line ||
      promise.kind !== 'confirmed' ||
      row.order_id !== orderId ||
      row.company_id !== companyId ||
      row.promise_id !== line.promise_id ||
      row.order_line_id !== line.order_line_id ||
      row.ordinal !== line.ordinal ||
      row.quantity_milli !== line.quantity_milli ||
      row.fulfilled_snapshot_milli !== line.fulfilled_snapshot_milli ||
      row.promised_date !== line.promised_date ||
      open.has(row.id)
    )
      return failure('NATIVE_PROCUREMENT_RESPONSE_INVALID');
    open.add(row.id);
  }
  return {
    order_id: orderId,
    history: value.history as NativeSupplierPromiseHistoryRow[],
    lines: value.lines as NativeSupplierPromiseLineRow[],
    open: value.open as NativeOpenPromiseRow[],
  };
}

function context(value: unknown, actor: NativeProcurementPageContext, input: NativeSupplierPromiseHistoryReadInput): Context {
  if (
    !exact(value, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug',
      'departmentSlug', 'assignmentId', 'assignmentVersion', 'revision', 'role',
      'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ])
  )
    return failure('NATIVE_PROCUREMENT_RESPONSE_INVALID');
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
    !Array.isArray(parsed.modules) ||
    parsed.modules.length > 128 ||
    !parsed.modules.every((item) => identifier(item)) ||
    parsed.modules.length !== actor.modules.length ||
    !parsed.modules.every((item) => actor.modules.includes(item)) ||
    !Array.isArray(parsed.permissions) ||
    parsed.permissions.length > 512 ||
    !parsed.permissions.every((item) => identifier(item)) ||
    parsed.permissions.length !== (actor.iam?.permissions.length ?? 0) ||
    !parsed.permissions.every((item) => (actor.iam?.permissions ?? []).includes(item))
  )
    return failure('NATIVE_PROCUREMENT_CONTEXT_CHANGED');
  if (
    input.expected &&
    (parsed.companyId !== input.expected.companyId ||
      parsed.assignmentId !== input.expected.assignmentId ||
      parsed.revision !== input.expected.revision ||
      parsed.assignmentVersion !== input.expected.assignmentVersion)
  )
    return failure('NATIVE_PROCUREMENT_CONTEXT_CHANGED');
  return parsed;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativeProcurementConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_PROCUREMENT_CONFIGURATION_INVALID');
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
    return failure('NATIVE_PROCUREMENT_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_PROCUREMENT_CONFIGURATION_INVALID');
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
      return failure('NATIVE_PROCUREMENT_CONFIGURATION_INVALID');
    holdings.set(origin.host, { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativeProcurementTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  orderId: string,
  holdingSlug: string,
  input: NativeSupplierPromiseHistoryReadInput,
  timeout: number,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== holdingSlug)
    return Promise.reject(new NativeProcurementError('NATIVE_PROCUREMENT_AUTHORITY_INVALID'));
  const headers = new Headers({ host: host!, accept: 'application/json', 'cache-control': 'no-cache' });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeProcurementError('NATIVE_PROCUREMENT_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  for (const [key, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return Promise.reject(new NativeProcurementError('NATIVE_PROCUREMENT_REQUEST_INVALID'));
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
      return Promise.reject(new NativeProcurementError('NATIVE_PROCUREMENT_REQUEST_INVALID'));
    headers.set('x-erpx-context-company', input.expected.companyId);
    headers.set('x-erpx-context-assignment', input.expected.assignmentId);
    headers.set('x-erpx-context-revision', String(input.expected.revision));
    headers.set('x-erpx-context-assignment-version', String(input.expected.assignmentVersion));
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/procurement/purchase-orders/${encodeURIComponent(orderId)}/supplier-promises`,
    base,
  );
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
        return failure('NATIVE_PROCUREMENT_RESPONSE_LIMIT');
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
    return failure('NATIVE_PROCUREMENT_UNAVAILABLE', response.status);
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
    return failure(code ?? 'NATIVE_PROCUREMENT_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_PROCUREMENT_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_PROCUREMENT_RESPONSE_INVALID');
  }
}

export function nativeProcurementAvailable(): boolean {
  return process.env.ERPX_NATIVE_PROCUREMENT_CONFIG !== undefined;
}

export function nativeProcurementConfiguration(raw: string): NativeProcurementConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeProcurementError('NATIVE_PROCUREMENT_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    !Array.isArray(value.holdings)
  )
    return failure('NATIVE_PROCUREMENT_CONFIGURATION_INVALID');
  if (
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) &&
        text(holding.origin) &&
        text(holding.holdingSlug) &&
        text(holding.holdingId),
    )
  )
    return failure('NATIVE_PROCUREMENT_CONFIGURATION_INVALID');
  return value as unknown as NativeProcurementConfiguration;
}

export function createNativeSupplierPromiseHistoryClient(
  raw: NativeProcurementConfiguration,
  transport: NativeProcurementTransport = directHttp,
) {
  const config = configuration(raw);
  const request = (
    incoming: Pick<Headers, 'get'>,
    actor: NativeProcurementPageContext,
    orderId: string,
    input: NativeSupplierPromiseHistoryReadInput,
    signal?: AbortSignal,
  ) => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return Promise.reject(new NativeProcurementError('NATIVE_PROCUREMENT_CONTEXT_CHANGED'));
    if (!identifier(orderId) || orderId.includes('/') || orderId.includes('\\'))
      return Promise.reject(new NativeProcurementError('NATIVE_PROCUREMENT_REQUEST_INVALID'));
    return transportCall(
      transport,
      config.base,
      config.holdings,
      incoming,
      orderId,
      input.holdingSlug,
      input,
      config.timeout,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeProcurementError) throw error;
      return failure('NATIVE_PROCUREMENT_UNAVAILABLE');
    });
  };
  const historyDataFor = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeProcurementPageContext,
    orderId: string,
    input: NativeSupplierPromiseHistoryReadInput,
    signal?: AbortSignal,
  ): Promise<NativeSupplierPromiseHistory> => {
    const value = await boundedJson(await request(incoming, actor, orderId, input, signal));
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_PROCUREMENT_RESPONSE_INVALID');
    context(value.context, actor, input);
    return historyData(value.data, orderId, actor.companyId);
  };
  return { history: historyDataFor };
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
