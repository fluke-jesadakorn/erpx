import 'server-only';
import { headers } from 'next/headers';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import {
  directHttp,
  nativeProcurementAvailable,
  nativeProcurementConfiguration,
  NativeProcurementError,
  type NativeProcurementConfiguration,
  type NativeProcurementPageContext,
  type NativeProcurementTransport,
} from './native-supplier-promise-history';
import { nativeSupplierPromiseHistoryPageContext } from './native-supplier-promise-history-page';

/** Native supplier-bill command receipt as returned by the Axum owner routes. */
export type NativeSupplierBillReceipt = Readonly<{
  id: string;
  version: number;
  receiptCount?: number;
  status?: string;
  voidedInvoiceId?: string;
  matchingMode?: string;
}>;

export type NativeSupplierBillCommandInput = Readonly<{
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

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeProcurementError(code, status, message);
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Record<string, unknown> =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string =>
  text(value, 200) && value.length > 0 && !/[\u0000-\u001f\u007f]/.test(value);
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const commandKey = (value: unknown): value is string =>
  typeof value === 'string' &&
  value.length >= 8 &&
  value.length <= 180 &&
  /^[A-Za-z0-9:_.-]+$/.test(value);
const matchId = (value: unknown): value is string =>
  typeof value === 'string' &&
  value.length > 0 &&
  value.length <= 128 &&
  /^[A-Za-z0-9_-]+$/.test(value) &&
  !value.includes('/') &&
  !value.includes('\\');
const boundedBody = (body: unknown): { body: string } => {
  if (!record(body)) return failure('NATIVE_PROCUREMENT_REQUEST_INVALID');
  let encoded: string;
  try {
    encoded = JSON.stringify(body);
  } catch {
    return failure('NATIVE_PROCUREMENT_REQUEST_INVALID');
  }
  if (new TextEncoder().encode(encoded).length > 64 * 1024)
    return failure('NATIVE_PROCUREMENT_REQUEST_INVALID');
  return { body: encoded };
};

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
  const holdings = new Map<string, { holdingId: string; holdingSlug: string; origin: string }>();
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
      origin.username ||
      origin.password ||
      !identifier(holding.holdingId) ||
      !text(holding.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug) ||
      holdings.has(origin.host)
    )
      return failure('NATIVE_PROCUREMENT_CONFIGURATION_INVALID');
    holdings.set(origin.host, {
      holdingId: holding.holdingId,
      holdingSlug: holding.holdingSlug,
      origin: holding.origin,
    });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function call(
  transport: NativeProcurementTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string; origin: string }>,
  incoming: Pick<Headers, 'get'>,
  path: string,
  holdingSlug: string,
  input: NativeSupplierBillCommandInput,
  timeout: number,
  key: string,
  body: string,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== holdingSlug)
    return Promise.reject(new NativeProcurementError('NATIVE_PROCUREMENT_AUTHORITY_INVALID'));
  const requestHeaders = new Headers({
    host: host!,
    accept: 'application/json',
    'cache-control': 'no-cache',
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeProcurementError('NATIVE_PROCUREMENT_COOKIE_LIMIT'));
    requestHeaders.set('cookie', cookie);
  }
  for (const [name, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return Promise.reject(new NativeProcurementError('NATIVE_PROCUREMENT_REQUEST_INVALID'));
      requestHeaders.set(name, slug);
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
    requestHeaders.set('x-erpx-context-company', input.expected.companyId);
    requestHeaders.set('x-erpx-context-assignment', input.expected.assignmentId);
    requestHeaders.set('x-erpx-context-revision', String(input.expected.revision));
    requestHeaders.set(
      'x-erpx-context-assignment-version',
      String(input.expected.assignmentVersion),
    );
  }
  requestHeaders.set('origin', holding.origin);
  requestHeaders.set('idempotency-key', key);
  requestHeaders.set('content-type', 'application/json');
  const url = new URL(path, base);
  const controller = new AbortController();
  const cancel = () => controller.abort();
  const timer = setTimeout(cancel, timeout);
  return transport('POST', url, requestHeaders, body, controller.signal).finally(() => {
    clearTimeout(timer);
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

function context(
  value: unknown,
  actor: NativeProcurementPageContext,
  input: NativeSupplierBillCommandInput,
): void {
  if (
    !exact(value, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug',
      'departmentSlug', 'assignmentId', 'assignmentVersion', 'revision', 'role',
      'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ])
  )
    return failure('NATIVE_PROCUREMENT_RESPONSE_INVALID');
  const parsed = value as Record<string, unknown> & {
    userId: string; holdingId: string; companyId: string; branchId: string | null;
    departmentId: string | null; branchSlug: string | null; departmentSlug: string | null;
    assignmentId: string; assignmentVersion: number; revision: number; role: string;
    assignmentScopeKind: string; modules: string[]; permissions: string[];
  };
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
    parsed.modules.length !== actor.modules.length ||
    !parsed.modules.every((item) => actor.modules.includes(item)) ||
    !Array.isArray(parsed.permissions) ||
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
}

function captureReceipt(
  value: unknown,
  key: string,
  body: unknown,
): NativeSupplierBillReceipt {
  if (!record(value)) return failure('NATIVE_PROCUREMENT_RESPONSE_INVALID');
  const receipts = record(body) && Array.isArray(body.receipts) ? body.receipts.length : undefined;
  if (
    value.id !== `bill-match-${key}` ||
    value.version !== 1 ||
    typeof value.receiptCount !== 'number' ||
    (receipts !== undefined && value.receiptCount !== receipts)
  )
    return failure('NATIVE_PROCUREMENT_RESPONSE_INVALID');
  return value as NativeSupplierBillReceipt;
}

function decideReceipt(
  value: unknown,
  id: string,
  body: unknown,
): NativeSupplierBillReceipt {
  if (!record(value)) return failure('NATIVE_PROCUREMENT_RESPONSE_INVALID');
  const expectedVersion =
    record(body) && typeof body.expectedVersion === 'number' ? body.expectedVersion + 1 : -1;
  if (
    value.id !== id ||
    value.version !== expectedVersion ||
    (value.status !== 'approved' && value.status !== 'returned')
  )
    return failure('NATIVE_PROCUREMENT_RESPONSE_INVALID');
  return value as NativeSupplierBillReceipt;
}

function policyReceipt(value: unknown, body: unknown): NativeSupplierBillReceipt {
  if (!record(value)) return failure('NATIVE_PROCUREMENT_RESPONSE_INVALID');
  const expectedVersion =
    record(body) && typeof body.expectedVersion === 'number' ? body.expectedVersion + 1 : -1;
  if (
    value.version !== expectedVersion ||
    (value.matchingMode !== 'optional' && value.matchingMode !== 'required') ||
    (record(body) &&
      typeof body.matchingMode === 'string' &&
      value.matchingMode !== body.matchingMode)
  )
    return failure('NATIVE_PROCUREMENT_RESPONSE_INVALID');
  return value as NativeSupplierBillReceipt;
}

export function createNativeSupplierBillCommandClient(
  raw: NativeProcurementConfiguration,
  transport: NativeProcurementTransport = directHttp,
) {
  const config = configuration(raw);
  const request = (
    incoming: Pick<Headers, 'get'>,
    actor: NativeProcurementPageContext,
    id: string,
    path: string,
    input: NativeSupplierBillCommandInput,
    key: string,
    body: unknown,
  ) => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return Promise.reject(new NativeProcurementError('NATIVE_PROCUREMENT_CONTEXT_CHANGED'));
    if (!commandKey(key)) return failure('NATIVE_PROCUREMENT_REQUEST_INVALID');
    const { body: encoded } = boundedBody(body);
    return call(
      transport,
      config.base,
      config.holdings,
      incoming,
      path,
      input.holdingSlug,
      input,
      config.timeout,
      key,
      encoded,
    ).catch((error: unknown) => {
      if (error instanceof NativeProcurementError) throw error;
      return failure('NATIVE_PROCUREMENT_UNAVAILABLE');
    });
  };
  const unpack = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeProcurementPageContext,
    id: string,
    path: string,
    input: NativeSupplierBillCommandInput,
    key: string,
    body: unknown,
    receipt: (data: unknown) => NativeSupplierBillReceipt,
  ): Promise<NativeSupplierBillReceipt> => {
    const value = await boundedJson(await request(incoming, actor, id, path, input, key, body));
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_PROCUREMENT_RESPONSE_INVALID');
    const envelope = value as { data: unknown; context: unknown };
    context(envelope.context, actor, input);
    return receipt(envelope.data);
  };
  const capture = (
    incoming: Pick<Headers, 'get'>,
    actor: NativeProcurementPageContext,
    input: NativeSupplierBillCommandInput,
    key: string,
    body: unknown,
  ): Promise<NativeSupplierBillReceipt> =>
    unpack(
      incoming, actor, '', `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/supplier-bills/capture`,
      input, key, body, (data) => captureReceipt(data, key, body),
    );
  const decide = (
    incoming: Pick<Headers, 'get'>,
    actor: NativeProcurementPageContext,
    id: string,
    input: NativeSupplierBillCommandInput,
    key: string,
    body: unknown,
  ): Promise<NativeSupplierBillReceipt> => {
    if (!matchId(id)) return failure('NATIVE_PROCUREMENT_REQUEST_INVALID');
    return unpack(
      incoming, actor, id,
      `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/supplier-bills/${encodeURIComponent(id)}/decision`,
      input, key, body, (data) => decideReceipt(data, id, body),
    );
  };
  const savePolicy = (
    incoming: Pick<Headers, 'get'>,
    actor: NativeProcurementPageContext,
    input: NativeSupplierBillCommandInput,
    key: string,
    body: unknown,
  ): Promise<NativeSupplierBillReceipt> =>
    unpack(
      incoming, actor, '', `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/supplier-bill-policy`,
      input, key, body, (data) => policyReceipt(data, body),
    );
  return { capture, decide, savePolicy };
}

function expected(actor: NativeProcurementPageContext) {
  const authority = actor.authority;
  if (!authority || !actor.iam)
    throw new NativeProcurementError('NATIVE_PROCUREMENT_CONTEXT_CHANGED');
  return {
    companyId: actor.companyId,
    assignmentId: actor.assignmentId,
    revision: authority.revision,
    assignmentVersion: authority.assignmentVersion,
  } as const;
}

function scope(actor: NativeProcurementPageContext) {
  return {
    branchSlug: actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug,
    departmentSlug: actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined,
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_PROCUREMENT_CONFIG;
  if (raw === undefined) return undefined;
  return createNativeSupplierBillCommandClient(nativeProcurementConfiguration(raw));
}

function pageContext(actor: WorkContext): NativeProcurementPageContext {
  return nativeSupplierPromiseHistoryPageContext(actor);
}

export async function captureNativeSupplierBill(
  actor: WorkContext,
  holdingSlug: string,
  key: string,
  body: unknown,
): Promise<NativeSupplierBillReceipt | undefined> {
  const native = client();
  if (!native) return undefined;
  const context = pageContext(actor);
  if (context.holdingSlug !== holdingSlug)
    throw new NativeProcurementError('NATIVE_PROCUREMENT_CONTEXT_CHANGED');
  return native.capture(await headers(), context, { holdingSlug, ...scope(context), expected: expected(context) }, key, body);
}

export async function decideNativeSupplierBill(
  actor: WorkContext,
  holdingSlug: string,
  id: string,
  key: string,
  body: unknown,
): Promise<NativeSupplierBillReceipt | undefined> {
  const native = client();
  if (!native) return undefined;
  const context = pageContext(actor);
  if (context.holdingSlug !== holdingSlug)
    throw new NativeProcurementError('NATIVE_PROCUREMENT_CONTEXT_CHANGED');
  return native.decide(await headers(), context, id, { holdingSlug, ...scope(context), expected: expected(context) }, key, body);
}

export async function saveNativeSupplierBillPolicy(
  actor: WorkContext,
  holdingSlug: string,
  key: string,
  body: unknown,
): Promise<NativeSupplierBillReceipt | undefined> {
  const native = client();
  if (!native) return undefined;
  const context = pageContext(actor);
  if (context.holdingSlug !== holdingSlug)
    throw new NativeProcurementError('NATIVE_PROCUREMENT_CONTEXT_CHANGED');
  return native.savePolicy(await headers(), context, { holdingSlug, ...scope(context), expected: expected(context) }, key, body);
}

export { nativeProcurementAvailable };
