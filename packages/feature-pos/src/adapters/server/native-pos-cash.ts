import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

/** Rendering input from retained page authorization, never a grant. */
export type NativePosCashActor = Readonly<{
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
}>;

export type NativePosCashConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

export type NativePosCashBody = Readonly<{
  id: string;
  expectedVersion: number;
  amountMinor: number;
  reason: string;
}>;

export type NativePosCashReceipt = Readonly<{ id: string }>;
export type NativePosCommandReceipt = Readonly<{ id: string; version?: number }>;

/** Closed native command set. Operations outside this set remain on the
 * retained TypeScript POS application until their public ports exist. */
export const NATIVE_POS_COMMANDS = new Set([
  'shift.open', 'shift.close', 'shift.review', 'cash.approve',
  'sale.create', 'sale.add', 'sale.update_line', 'sale.customer', 'sale.move_table', 'sale.remove',
  'sale.discount', 'sale.merge', 'sale.split', 'payment.create', 'sale.complete',
  'sale.hold', 'sale.resume', 'sale.cancel',
  'refund.request', 'refund.reject',
]);
/** Operations in the five POS owner workflows. Once the native transport is
 * configured, an operation here without a native implementation must close
 * rather than silently use the retained TypeScript writer. */
export const POS_NATIVE_WORKFLOW_OPERATIONS = new Set([
  ...NATIVE_POS_COMMANDS,
  'cash.create',
  'sale.add', 'sale.update_line', 'sale.discount', 'sale.remove',
  'sale.customer', 'sale.move_table', 'sale.complete', 'sale.approve_discount',
  'payment.create', 'payment.verify_transfer', 'payment.cancel_transfer',
  'payment.retry', 'payment.cancel',
  'refund.approve',
]);
export function nativePosWorkflowOperation(operation: string): boolean {
  return POS_NATIVE_WORKFLOW_OPERATIONS.has(operation);
}
export function nativePosCommandAvailable(): boolean {
  return process.env.ERPX_NATIVE_POS_CASH_CONFIG !== undefined;
}
export function nativePosCommandCapability(operation: string): {
  capability: 'pos.shifts' | 'pos.sales' | 'pos.refunds';
  purpose: 'new-work' | 'settle-existing';
} | undefined {
  if (!NATIVE_POS_COMMANDS.has(operation)) return undefined;
  if (operation.startsWith('shift.') || operation.startsWith('cash.'))
    return { capability: 'pos.shifts', purpose: operation === 'shift.open' ? 'new-work' : 'settle-existing' };
  if (operation.startsWith('refund.')) return { capability: 'pos.refunds', purpose: 'settle-existing' };
  if (operation === 'sale.cancel' || operation === 'sale.complete')
    return { capability: 'pos.sales', purpose: 'settle-existing' };
  return { capability: 'pos.sales', purpose: 'new-work' };
}
/** Selected only when the native configuration is present and the operation is
 * in the closed native set. Unset configuration keeps the retained application;
 * a configured native failure is surfaced by the caller and never falls back. */
export function nativePosCommandScope(operation: string) {
  return nativePosCommandAvailable() ? nativePosCommandCapability(operation) : undefined;
}

export type NativePosCashTransport = (
  method: 'POST',
  url: URL,
  headers: Headers,
  body: string,
  signal: AbortSignal,
) => Promise<Response>;

export class NativePosCashError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native POS cash request could not be completed.');
    this.name = 'NativePosCashError';
    this.code = code;
    this.status = status;
  }
}

type Row = Record<string, unknown>;

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativePosCashError(code, status, message);
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
const texts = (value: unknown, max = 256): value is string[] =>
  Array.isArray(value) && value.length <= max && value.every((item) => text(item, 256));
const sameSet = (left: readonly string[], right: readonly string[]) => {
  const a = new Set(left);
  const b = new Set(right);
  return a.size === b.size && [...a].every((value) => b.has(value));
};
const keyPattern = /^[a-zA-Z0-9:_.-]{8,180}$/;
const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativePosCashConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_POS_CASH_CONFIGURATION_INVALID');
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
    return failure('NATIVE_POS_CASH_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string; origin: string }>();
  const slugs = new Set<string>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_POS_CASH_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      origin.username ||
      origin.password ||
      !identifier(holding.holdingId) ||
      !text(holding.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug) ||
      holdings.has(origin.host) ||
      slugs.has(holding.holdingSlug)
    )
      return failure('NATIVE_POS_CASH_CONFIGURATION_INVALID');
    holdings.set(origin.host, {
      holdingId: holding.holdingId,
      holdingSlug: holding.holdingSlug,
      origin: holding.origin,
    });
    slugs.add(holding.holdingSlug);
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function body(value: string): NativePosCashBody {
  if (new TextEncoder().encode(value).length > 8 * 1024)
    return failure('NATIVE_POS_CASH_REQUEST_INVALID');
  let parsed: unknown;
  try {
    parsed = JSON.parse(value);
  } catch {
    return failure('NATIVE_POS_CASH_REQUEST_INVALID');
  }
  if (!exact(parsed, ['id', 'expectedVersion', 'amountMinor', 'reason']))
    return failure('NATIVE_POS_CASH_REQUEST_INVALID');
  const id = parsed.id;
  const expectedVersion = parsed.expectedVersion;
  const amountMinor = parsed.amountMinor;
  const reason = parsed.reason;
  if (
    !identifier(id) ||
    !integer(expectedVersion, 1) ||
    typeof amountMinor !== 'number' ||
    !Number.isSafeInteger(amountMinor) ||
    amountMinor === 0 ||
    Math.abs(amountMinor) > 99_999_999_999 ||
    !text(reason, 1_000) ||
    reason.trim().length < 3
  )
    return failure('NATIVE_POS_CASH_REQUEST_INVALID');
  return parsed as NativePosCashBody;
}

function transportCall(
  transport: NativePosCashTransport,
  config: ReturnType<typeof configuration>,
  incoming: Pick<Headers, 'get'>,
  actor: NativePosCashActor,
  key: string,
  input: NativePosCashBody,
  timeout: number,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const target = host ? config.holdings.get(host) : undefined;
  if (
    !target ||
    target.holdingSlug !== actor.holdingSlug ||
    target.holdingId !== actor.holdingId ||
    actor.holdingSlug.length === 0
  )
    return Promise.reject(new NativePosCashError('NATIVE_POS_CASH_AUTHORITY_INVALID'));
  const origin = incoming.get('origin');
  if (origin && origin !== target.origin)
    return Promise.reject(new NativePosCashError('ORIGIN_FORBIDDEN', 403));
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'content-type': 'application/json',
    'idempotency-key': key,
    'x-erpx-context-user': actor.userId,
    'x-erpx-context-holding': actor.holdingId,
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority?.revision ?? ''),
    'x-erpx-context-assignment-version': String(actor.authority?.assignmentVersion ?? ''),
  });
  if (origin) headers.set('origin', origin);
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativePosCashError('NATIVE_POS_CASH_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  if (actor.scope.kind === 'branch' || actor.scope.kind === 'department') {
    const branch = actor.scope.branchSlug;
    if (!branch || !/^[a-z0-9_-]+$/.test(branch))
      return Promise.reject(new NativePosCashError('NATIVE_POS_CASH_REQUEST_INVALID'));
    headers.set('x-erpx-workspace-branch', branch);
  }
  if (actor.scope.kind === 'department') {
    const department = actor.scope.departmentSlug;
    if (!department || !/^[a-z0-9_-]+$/.test(department))
      return Promise.reject(new NativePosCashError('NATIVE_POS_CASH_REQUEST_INVALID'));
    headers.set('x-erpx-workspace-department', department);
  }
  if (actor.branchId && actor.scope.kind === 'holding')
    return Promise.reject(new NativePosCashError('NATIVE_POS_CASH_AUTHORITY_INVALID'));
  if (actor.departmentId && actor.scope.kind !== 'department')
    return Promise.reject(new NativePosCashError('NATIVE_POS_CASH_AUTHORITY_INVALID'));
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/pos/cash/create`,
    config.base,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, timeout);
  return transport('POST', url, headers, JSON.stringify(input), controller.signal).finally(() => {
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
        return failure('NATIVE_POS_CASH_RESPONSE_LIMIT');
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
    return failure('NATIVE_POS_CASH_UNAVAILABLE', response.status);
  }
  const contentType = response.headers.get('content-type')?.split(';')[0]?.trim();
  if (response.status !== 200) {
    const raw = await boundedBytes(response, 64 * 1024);
    let code: string | undefined;
    let message: string | undefined;
    if (contentType === 'application/json') {
      try {
        const value: unknown = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(raw));
        if (record(value) && record(value.error)) {
          if (text(value.error.code, 200)) code = value.error.code;
          if (text(value.error.message, 2000)) message = value.error.message;
        }
      } catch {
        // A malformed owner error keeps the bounded transport failure.
      }
    }
    return failure(code ?? 'NATIVE_POS_CASH_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_POS_CASH_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 256 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_POS_CASH_RESPONSE_INVALID');
  }
}

function context(value: unknown, actor: NativePosCashActor) {
  if (
    !exact(value, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug',
      'departmentSlug', 'assignmentId', 'assignmentVersion', 'revision', 'role',
      'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ])
  )
    return failure('NATIVE_POS_CASH_RESPONSE_INVALID');
  const parsed = value as Row & {
    userId: string;
    holdingId: string;
    companyId: string;
    branchId: string | null;
    departmentId: string | null;
    branchSlug: string | null;
    departmentSlug: string | null;
    assignmentId: string;
    assignmentVersion: number;
    revision: number;
    role: string;
    assignmentScopeKind: string;
    modules: string[];
    permissions: string[];
    expiresAt: number;
  };
  const branchSlug =
    actor.scope.kind === 'holding' ? null : actor.scope.branchSlug ?? null;
  const departmentSlug =
    actor.scope.kind === 'department' ? actor.scope.departmentSlug : null;
  if (
    parsed.userId !== actor.userId ||
    parsed.holdingId !== actor.holdingId ||
    parsed.companyId !== actor.companyId ||
    parsed.branchId !== actor.branchId ||
    parsed.departmentId !== actor.departmentId ||
    parsed.branchSlug !== branchSlug ||
    parsed.departmentSlug !== departmentSlug ||
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
    return failure('NATIVE_POS_CASH_CONTEXT_CHANGED');
}

function receipt(value: unknown): NativePosCashReceipt {
  if (
    !exact(value, ['id']) ||
    typeof value.id !== 'string' ||
    !uuidPattern.test(value.id)
  )
    return failure('NATIVE_POS_CASH_RESPONSE_INVALID');
  return { id: value.id };
}

export function nativePosCashAvailable(): boolean {
  return process.env.ERPX_NATIVE_POS_CASH_CONFIG !== undefined;
}

export function nativePosCashConfiguration(raw: string): NativePosCashConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativePosCashError('NATIVE_POS_CASH_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    !Array.isArray(value.holdings)
  )
    return failure('NATIVE_POS_CASH_CONFIGURATION_INVALID');
  if (
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) &&
        text(holding.origin) &&
        text(holding.holdingSlug) &&
        text(holding.holdingId),
    )
  )
    return failure('NATIVE_POS_CASH_CONFIGURATION_INVALID');
  return value as unknown as NativePosCashConfiguration;
}

function commandTransportCall(
  transport: NativePosCashTransport,
  config: ReturnType<typeof configuration>,
  incoming: Pick<Headers, 'get'>,
  actor: NativePosCashActor,
  operation: string,
  key: string,
  rawBody: string,
  timeout: number,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const target = host ? config.holdings.get(host) : undefined;
  if (
    !target ||
    target.holdingSlug !== actor.holdingSlug ||
    target.holdingId !== actor.holdingId
  )
    return Promise.reject(new NativePosCashError('NATIVE_POS_CASH_AUTHORITY_INVALID'));
  const origin = incoming.get('origin');
  if (origin && origin !== target.origin)
    return Promise.reject(new NativePosCashError('ORIGIN_FORBIDDEN', 403));
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'content-type': 'application/json',
    'idempotency-key': key,
    'x-erpx-context-user': actor.userId,
    'x-erpx-context-holding': actor.holdingId,
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority?.revision ?? ''),
    'x-erpx-context-assignment-version': String(actor.authority?.assignmentVersion ?? ''),
  });
  if (origin) headers.set('origin', origin);
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativePosCashError('NATIVE_POS_CASH_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  if (actor.scope.kind === 'branch' || actor.scope.kind === 'department') {
    const branch = actor.scope.branchSlug;
    if (!branch || !/^[a-z0-9_-]+$/.test(branch))
      return Promise.reject(new NativePosCashError('NATIVE_POS_CASH_REQUEST_INVALID'));
    headers.set('x-erpx-workspace-branch', branch);
  }
  if (actor.scope.kind === 'department') {
    const department = actor.scope.departmentSlug;
    if (!department || !/^[a-z0-9_-]+$/.test(department))
      return Promise.reject(new NativePosCashError('NATIVE_POS_CASH_REQUEST_INVALID'));
    headers.set('x-erpx-workspace-department', department);
  }
  if (actor.branchId && actor.scope.kind === 'holding')
    return Promise.reject(new NativePosCashError('NATIVE_POS_CASH_AUTHORITY_INVALID'));
  if (actor.departmentId && actor.scope.kind !== 'department')
    return Promise.reject(new NativePosCashError('NATIVE_POS_CASH_AUTHORITY_INVALID'));
  if (new TextEncoder().encode(rawBody).length > 64 * 1024)
    return Promise.reject(new NativePosCashError('NATIVE_POS_CASH_REQUEST_INVALID'));
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/pos/commands/${encodeURIComponent(operation)}`,
    config.base,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, timeout);
  return transport('POST', url, headers, rawBody, controller.signal).finally(() => {
    clearTimeout(timer);
    signal?.removeEventListener('abort', cancel);
  });
}

function commandReceipt(operation: string, value: unknown): NativePosCommandReceipt {
  if (!record(value) || typeof value.id !== 'string' || !value.id || value.id.length > 200)
    return failure('NATIVE_POS_CASH_RESPONSE_INVALID');
  if (Object.keys(value).some((key) => key !== 'id' && key !== 'version'))
    return failure('NATIVE_POS_CASH_RESPONSE_INVALID');
  const generated = ['shift.open', 'sale.create', 'refund.request'].includes(operation);
  if (generated && !uuidPattern.test(value.id))
    return failure('NATIVE_POS_CASH_RESPONSE_INVALID');
  if (value.version !== undefined && !integer(value.version, 1))
    return failure('NATIVE_POS_CASH_RESPONSE_INVALID');
  if (
    ['sale.hold', 'sale.resume', 'sale.cancel'].includes(operation) &&
    value.version === undefined
  )
    return failure('NATIVE_POS_CASH_RESPONSE_INVALID');
  return value.version === undefined
    ? { id: value.id }
    : { id: value.id, version: value.version };
}

export function createNativePosCashClient(
  raw: NativePosCashConfiguration,
  transport: NativePosCashTransport = directHttp,
) {
  const config = configuration(raw);
  const cash = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativePosCashActor,
    key: string,
    rawBody: string,
    signal?: AbortSignal,
  ): Promise<NativePosCashReceipt> => {
    if (!actor.authority || !actor.iam || !keyPattern.test(key))
      return failure('NATIVE_POS_CASH_REQUEST_INVALID');
    if (!(actor.iam.permissions ?? []).includes('pos.sell'))
      return failure(
        'POS_PERMISSION_REQUIRED',
        403,
        'Your selected role does not grant POS selling access.',
      );
    if (!actor.modules.includes('pos'))
      return failure('POS_MODULE_FORBIDDEN', 403, 'Your current role cannot access POS.');
    if (actor.departmentId)
      return failure('POS_BRANCH_REQUIRED', 403, 'Select a Company or branch for POS.');
    const input = body(rawBody);
    const response = await transportCall(
      transport,
      config,
      incoming,
      actor,
      key,
      input,
      config.timeout,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativePosCashError) throw error;
      return failure('NATIVE_POS_CASH_UNAVAILABLE');
    });
    const value = await boundedJson(response);
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_POS_CASH_RESPONSE_INVALID');
    context(value.context, actor);
    return receipt(value.data);
  };
  const command = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativePosCashActor,
    operation: string,
    key: string,
    rawBody: string,
    signal?: AbortSignal,
  ): Promise<NativePosCommandReceipt> => {
    if (
      !NATIVE_POS_COMMANDS.has(operation) ||
      !actor.authority ||
      !actor.iam ||
      !keyPattern.test(key)
    )
      return failure('NATIVE_POS_CASH_REQUEST_INVALID');
    const permission = ['shift.review', 'cash.approve', 'refund.reject'].includes(operation)
      ? 'pos.approve'
      : 'pos.sell';
    if (!(actor.iam.permissions ?? []).includes(permission))
      return failure(
        'POS_PERMISSION_REQUIRED',
        403,
        'Your selected role does not grant this POS command.',
      );
    if (!actor.modules.includes('pos'))
      return failure('POS_MODULE_FORBIDDEN', 403, 'Your current role cannot access POS.');
    if (actor.departmentId)
      return failure('POS_BRANCH_REQUIRED', 403, 'Select a Company or branch for POS.');
    try {
      JSON.parse(rawBody);
    } catch {
      return failure('NATIVE_POS_CASH_REQUEST_INVALID');
    }
    const response = await commandTransportCall(
      transport,
      config,
      incoming,
      actor,
      operation,
      key,
      rawBody,
      config.timeout,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativePosCashError) throw error;
      return failure('NATIVE_POS_CASH_UNAVAILABLE');
    });
    const value = await boundedJson(response);
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_POS_CASH_RESPONSE_INVALID');
    context(value.context, actor);
    return commandReceipt(operation, value.data);
  };
  return { cash, command };
}

/** Retained WorkContext render input; the native owner still re-checks scope. */
export function nativePosCashActor(actor: WorkContext): NativePosCashActor {
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
    scope: actor.scope,
    authority: actor.authority,
    modules: actor.modules,
    iam: actor.iam,
  };
}

/** Bounded failure policy for the retained POS route error mapping. */
export function nativePosCashFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativePosCashError)) return undefined;
  if (error.code === 'NATIVE_POS_CASH_CONTEXT_CHANGED')
    return {
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before retrying.',
    };
  if (error.code === 'WORK_CONTEXT_CHANGED')
    return { status: 409, code: 'WORK_CONTEXT_CHANGED', message: error.message };
  if (error.status === 401)
    return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
  if (error.status === 403)
    return { status: 403, code: error.code, message: error.message };
  if (error.status === 404)
    return { status: 404, code: 'NOT_FOUND', message: 'This POS session is not available in this scope.' };
  if (error.status === 409) return { status: 409, code: error.code, message: error.message };
  if (error.status === 422) return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_POS_CASH_UNAVAILABLE',
      message: 'The native POS cash command is unavailable.',
    };
  if (error.code === 'NATIVE_POS_CASH_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_POS_CASH_UNAVAILABLE',
      message: 'The native POS cash command is unavailable.',
    };
  if (error.code === 'NATIVE_POS_CASH_REQUEST_INVALID')
    return {
      status: 422,
      code: 'POS_CASH_REQUEST_INVALID',
      message: 'Review the cash amount and reason.',
    };
  if (
    error.code === 'NATIVE_POS_CASH_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_POS_CASH_COOKIE_LIMIT' ||
    error.code === 'NATIVE_POS_CASH_RESPONSE_INVALID' ||
    error.code === 'NATIVE_POS_CASH_RESPONSE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_POS_CASH_UNAVAILABLE',
      message: 'The native POS cash command is unavailable.',
    };
  return undefined;
}

/** Fresh bounded socket; bypasses patched fetch/HMR and ambient proxies. */
export function directHttp(
  method: 'POST',
  url: URL,
  headers: Headers,
  body: string,
  signal: AbortSignal,
): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import('node:http').ProxyEnv,
    });
    const payload = Buffer.from(body, 'utf8');
    const outgoing = request(
      url,
      {
        method,
        agent,
        headers: {
          ...Object.fromEntries(headers),
          'content-length': String(payload.byteLength),
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
    outgoing.end(payload);
  });
}
