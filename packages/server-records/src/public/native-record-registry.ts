import 'server-only';
import {
  directHttp,
  type NativeRecordField,
  type NativeRecordView,
  type NativeRecordsPageContext,
} from './native-record-context';

/** Strict owner transport for the bounded Records registry list/page read. */
export type NativeRecordRegistryConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

export type NativeRecordRegistryInput = Readonly<{
  holdingSlug: string;
  moduleId: string;
  sectionId: string;
  q?: string;
  cursor?: string;
}>;

export type NativeRecordWriteInput = Readonly<{
  holdingSlug: string;
  moduleId: string;
  sectionId: string;
  title: string;
  description: string;
  fields: readonly NativeRecordField[];
}>;

export type NativeRecordUpdateInput = Readonly<{
  holdingSlug: string;
  moduleId: string;
  recordId: string;
  expectedVersion: number;
  fields: Readonly<Record<string, string | number | boolean>>;
}>;

export type NativeRecordArchiveInput = Readonly<{
  holdingSlug: string;
  moduleId: string;
  recordId: string;
  expectedVersion: number;
}>;

export type NativeRecordArchive = Readonly<{
  id: string;
  moduleId: string;
  archived: true;
}>;

export type NativeRecordReferenceKind =
  | 'dependsOn'
  | 'blocks'
  | 'billedBy'
  | 'fulfills'
  | 'authoredWith';

export type NativeRecordReferenceInput = Readonly<{
  holdingSlug: string;
  moduleId: string;
  recordId: string;
  targetModule: string;
  targetId: string;
  kind: NativeRecordReferenceKind;
}>;

export type NativeRecordReference = Readonly<{
  id: string;
  moduleId: string;
}>;

export type NativeRecordRegistryPage = Readonly<{
  items: NativeRecordView[];
  nextCursor: string | null;
  previousCursor: string | null;
  hasMore: boolean;
}>;

export type NativeRecordRegistryTransport = (
  method: 'GET' | 'POST' | 'PATCH' | 'DELETE',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeRecordRegistryError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Records registry request could not be completed.');
    this.name = 'NativeRecordRegistryError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeRecordRegistryError(code, status, message);
};
type Row = Record<string, unknown>;
const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, maximum = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= maximum;
const trimmed = (value: unknown, minimum: number, maximum: number): value is string =>
  text(value, maximum) &&
  [...value].length >= minimum &&
  value.trim() === value &&
  !/[\u0000-\u001f\u007f]/.test(value);
const identifier = (value: unknown): value is string =>
  text(value, 200) && value.length > 0 && !/[\u0000-\u001f\u007f]/.test(value);
const integer = (value: unknown, minimum = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= minimum;
const section = (value: unknown): value is string => trimmed(value, 1, 128);
const search = (value: unknown): value is string =>
  text(value, 800) &&
  [...value].length <= 200 &&
  !/[\u0000-\u001f\u007f]/.test(value);
const boundedId = (value: unknown): value is string =>
  identifier(value) && value.length <= 180;

const statuses = new Set(['draft', 'in_review', 'approved', 'completed', 'rejected']);
const referenceKinds = new Set([
  'dependsOn',
  'blocks',
  'billedBy',
  'fulfills',
  'authoredWith',
]);

const field = (value: unknown): value is NativeRecordField => {
  if (
    !record(value) ||
    !['key', 'label', 'type', 'value'].every((key) => Object.hasOwn(value, key)) ||
    Object.keys(value).some(
      (key) => !['key', 'label', 'type', 'value', 'options'].includes(key),
    )
  )
    return false;
  if (
    !trimmed(value.key, 1, 80) ||
    !trimmed(value.label, 1, 120) ||
    !trimmed(value.type, 1, 30) ||
    !(
      typeof value.value === 'string' ||
      typeof value.value === 'number' ||
      typeof value.value === 'boolean'
    )
  )
    return false;
  if (typeof value.value === 'number' && !Number.isFinite(value.value)) return false;
  if (value.options !== undefined) {
    if (
      !Array.isArray(value.options) ||
      value.options.length > 50 ||
      !value.options.every((option) => trimmed(option, 1, 120))
    )
      return false;
  }
  return true;
};

const view = (
  value: unknown,
  moduleId: string,
  sectionId: string | null,
): value is NativeRecordView =>
  exact(value, [
    'moduleId',
    'id',
    'sectionId',
    'title',
    'description',
    'status',
    'fields',
    'version',
    'createdBy',
    'updatedAt',
  ]) &&
  value.moduleId === moduleId &&
  (sectionId === null || value.sectionId === sectionId) &&
  boundedId(value.id) &&
  trimmed(value.title, 1, 160) &&
  text(value.description, 2_000) &&
  (typeof value.description === 'string' && !value.description.includes('\u0000')) &&
  typeof value.status === 'string' &&
  statuses.has(value.status) &&
  Array.isArray(value.fields) &&
  value.fields.length >= 1 &&
  value.fields.length <= 64 &&
  value.fields.every(field) &&
  new Set((value.fields as NativeRecordField[]).map((item) => item.key)).size ===
    value.fields.length &&
  integer(value.version, 1) &&
  trimmed(value.createdBy, 1, 200) &&
  integer(value.updatedAt);

const archived = (
  value: unknown,
  moduleId: string,
  recordId: string,
): value is NativeRecordArchive =>
  exact(value, ['id', 'moduleId', 'archived']) &&
  boundedId(value.id) &&
  value.id === recordId &&
  value.moduleId === moduleId &&
  value.archived === true;

const referenced = (
  value: unknown,
  moduleId: string,
  recordId: string,
): value is NativeRecordReference =>
  exact(value, ['id', 'moduleId']) &&
  boundedId(value.id) &&
  value.id === recordId &&
  value.moduleId === moduleId;

type ParsedCursor = Readonly<{
  key: number;
  id: string;
  reverse: boolean;
  scope: string;
}>;

const parseCursor = (value: unknown): ParsedCursor | undefined => {
  if (!text(value, 4_096) || value.length === 0) return undefined;
  try {
    const parsed: unknown = JSON.parse(Buffer.from(value, 'base64url').toString('utf8'));
    if (
      !exact(parsed, ['key', 'id', 'reverse', 'scope']) ||
      typeof parsed.key !== 'string' ||
      !/^(?:0|[1-9][0-9]*)$/.test(parsed.key) ||
      !Number.isSafeInteger(Number(parsed.key)) ||
      !boundedId(parsed.id) ||
      typeof parsed.reverse !== 'boolean' ||
      !text(parsed.scope, 2_048) ||
      parsed.scope.length === 0
    )
      return undefined;
    return {
      key: Number(parsed.key),
      id: parsed.id,
      reverse: parsed.reverse,
      scope: parsed.scope,
    };
  } catch {
    return undefined;
  }
};
const cursor = (value: unknown): value is string => parseCursor(value) !== undefined;

function unique(items: readonly NativeRecordView[]): boolean {
  return new Set(items.map((item) => item.id)).size === items.length;
}

function ordered(items: readonly NativeRecordView[]): boolean {
  for (let index = 1; index < items.length; index += 1) {
    const previous = items[index - 1]!;
    const current = items[index]!;
    if (
      previous.updatedAt < current.updatedAt ||
      (previous.updatedAt === current.updatedAt && previous.id < current.id)
    )
      return false;
  }
  return true;
}

function pageData(value: unknown, input: NativeRecordRegistryInput): NativeRecordRegistryPage {
  if (
    !exact(value, ['items', 'nextCursor', 'previousCursor', 'hasMore']) ||
    !Array.isArray(value.items) ||
    value.items.length > 30 ||
    !value.items.every((item) => view(item, input.moduleId, input.sectionId)) ||
    !unique(value.items as NativeRecordView[]) ||
    !ordered(value.items as NativeRecordView[]) ||
    typeof value.hasMore !== 'boolean'
  )
    return failure('NATIVE_RECORD_REGISTRY_RESPONSE_INVALID');
  const items = value.items as NativeRecordView[];
  const next = value.nextCursor === null ? null : parseCursor(value.nextCursor);
  const previous = value.previousCursor === null ? null : parseCursor(value.previousCursor);
  if (
    (value.nextCursor !== null && next === undefined) ||
    (value.previousCursor !== null && previous === undefined) ||
    value.hasMore !== (next !== null && next !== undefined) ||
    (items.length === 0
      ? next !== null || previous !== null
      : (next !== undefined &&
          next !== null &&
          (next.reverse ||
            next.id !== items[items.length - 1]!.id ||
            next.key !== items[items.length - 1]!.updatedAt)) ||
        (previous !== undefined &&
          previous !== null &&
          (!previous.reverse ||
            previous.id !== items[0]!.id ||
            previous.key !== items[0]!.updatedAt)))
  )
    return failure('NATIVE_RECORD_REGISTRY_RESPONSE_INVALID');
  return value as unknown as NativeRecordRegistryPage;
}

function listData(value: unknown, input: NativeRecordRegistryInput): NativeRecordView[] {
  if (
    !Array.isArray(value) ||
    value.length > 500 ||
    !value.every((item) => view(item, input.moduleId, input.sectionId)) ||
    !unique(value as NativeRecordView[]) ||
    !ordered(value as NativeRecordView[])
  )
    return failure('NATIVE_RECORD_REGISTRY_RESPONSE_INVALID');
  return value as NativeRecordView[];
}

function context(value: unknown, actor: NativeRecordsPageContext): void {
  if (
    !exact(value, [
      'userId',
      'holdingId',
      'companyId',
      'branchId',
      'departmentId',
      'assignmentId',
      'assignmentVersion',
      'revision',
      'role',
      'scopeKind',
      'modules',
      'permissions',
      'expiresAt',
    ])
  )
    return failure('NATIVE_RECORD_REGISTRY_RESPONSE_INVALID');
  const parsed = value as Row;
  if (
    parsed.userId !== actor.userId ||
    parsed.holdingId !== actor.holdingId ||
    parsed.companyId !== actor.companyId ||
    parsed.branchId !== actor.branchId ||
    parsed.departmentId !== actor.departmentId ||
    parsed.assignmentId !== actor.assignmentId ||
    parsed.assignmentVersion !== actor.authority.assignmentVersion ||
    parsed.revision !== actor.authority.revision ||
    parsed.role !== actor.role ||
    parsed.scopeKind !== actor.assignmentScope ||
    !integer(parsed.expiresAt, 1) ||
    !Array.isArray(parsed.modules) ||
    parsed.modules.length > 128 ||
    parsed.modules.length !== actor.modules.length ||
    !parsed.modules.every(
      (item) => identifier(item) && actor.modules.includes(item),
    ) ||
    !Array.isArray(parsed.permissions) ||
    parsed.permissions.length > 512 ||
    parsed.permissions.length !== actor.iam.permissions.length ||
    !parsed.permissions.every(
      (item) => identifier(item) && actor.iam.permissions.includes(item),
    )
  )
    return failure('NATIVE_RECORD_REGISTRY_CONTEXT_CHANGED');
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativeRecordRegistryConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_RECORD_REGISTRY_CONFIGURATION_INVALID');
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
    raw.timeoutMs > 30_000 ||
    !Array.isArray(raw.holdings) ||
    raw.holdings.length < 1 ||
    raw.holdings.length > 64
  )
    return failure('NATIVE_RECORD_REGISTRY_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_RECORD_REGISTRY_CONFIGURATION_INVALID');
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
      return failure('NATIVE_RECORD_REGISTRY_CONFIGURATION_INVALID');
    holdings.set(origin.host, {
      holdingId: holding.holdingId,
      holdingSlug: holding.holdingSlug,
    });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativeRecordRegistryTransport,
  config: ReturnType<typeof configuration>,
  incoming: Pick<Headers, 'get'>,
  actor: NativeRecordsPageContext,
  input: NativeRecordRegistryInput,
  paged: boolean,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? config.holdings.get(host) : undefined;
  if (
    !holding ||
    holding.holdingSlug !== input.holdingSlug ||
    holding.holdingId !== actor.holdingId
  )
    return Promise.reject(
      new NativeRecordRegistryError('NATIVE_RECORD_REGISTRY_AUTHORITY_INVALID'),
    );
  if (
    !/^[a-z0-9_-]{1,64}$/.test(input.moduleId) ||
    !section(input.sectionId) ||
    (input.q !== undefined && !search(input.q)) ||
    (input.cursor !== undefined && !cursor(input.cursor)) ||
    (!paged && (input.q !== undefined || input.cursor !== undefined))
  )
    return Promise.reject(
      new NativeRecordRegistryError('NATIVE_RECORD_REGISTRY_REQUEST_INVALID'),
    );
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    'cache-control': 'no-cache',
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeRecordRegistryError('NATIVE_RECORD_REGISTRY_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/records/${encodeURIComponent(input.moduleId)}`,
    config.base,
  );
  url.searchParams.set('section', input.sectionId);
  if (paged) {
    url.searchParams.set('paged', '1');
    if (input.q) url.searchParams.set('q', input.q);
    if (input.cursor) url.searchParams.set('cursor', input.cursor);
  }
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, config.timeout);
  return transport('GET', url, headers, undefined, controller.signal).finally(() => {
    clearTimeout(timer);
    signal?.removeEventListener('abort', cancel);
  });
}

function writeCall(
  transport: NativeRecordRegistryTransport,
  config: ReturnType<typeof configuration>,
  incoming: Pick<Headers, 'get'>,
  actor: NativeRecordsPageContext,
  input: NativeRecordWriteInput,
  key: string,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? config.holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== input.holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(
      new NativeRecordRegistryError('NATIVE_RECORD_REGISTRY_AUTHORITY_INVALID'),
    );
  const origin = incoming.get('origin');
  if (!origin || origin !== `https://${host}`)
    return Promise.reject(new NativeRecordRegistryError('ORIGIN_FORBIDDEN', 403));
  if (
    !/^[a-z0-9_-]{1,64}$/.test(input.moduleId) ||
    !section(input.sectionId) ||
    !trimmed(input.title, 1, 160) ||
    typeof input.description !== 'string' ||
    new TextEncoder().encode(input.description).length > 2_000 ||
    input.description.includes('\u0000') ||
    !Array.isArray(input.fields) ||
    input.fields.length < 1 ||
    input.fields.length > 64 ||
    !input.fields.every(field) ||
    new Set(input.fields.map((entry) => entry.key)).size !== input.fields.length
  )
    return Promise.reject(
      new NativeRecordRegistryError('NATIVE_RECORD_REGISTRY_REQUEST_INVALID'),
    );
  if (!/^[A-Za-z0-9_.:-]{8,180}$/.test(key))
    return Promise.reject(new NativeRecordRegistryError('IDEMPOTENCY_KEY_REQUIRED', 422));
  const body = JSON.stringify({
    sectionId: input.sectionId,
    title: input.title.trim(),
    description: input.description.trim(),
    fields: input.fields,
  });
  if (new TextEncoder().encode(body).length > 64 * 1024)
    return Promise.reject(new NativeRecordRegistryError('NATIVE_RECORD_REGISTRY_REQUEST_INVALID'));
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    'content-type': 'application/json',
    origin,
    'idempotency-key': key,
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority.revision),
    'x-erpx-context-assignment-version': String(actor.authority.assignmentVersion),
  });
  if (actor.branchSlug) headers.set('x-erpx-workspace-branch', actor.branchSlug);
  if (actor.departmentSlug) headers.set('x-erpx-workspace-department', actor.departmentSlug);
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeRecordRegistryError('NATIVE_RECORD_REGISTRY_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/records/${encodeURIComponent(input.moduleId)}`,
    config.base,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, config.timeout);
  return transport('POST', url, headers, body, controller.signal).finally(() => {
    clearTimeout(timer);
    signal?.removeEventListener('abort', cancel);
  });
}

function updateCall(
  transport: NativeRecordRegistryTransport,
  config: ReturnType<typeof configuration>,
  incoming: Pick<Headers, 'get'>,
  actor: NativeRecordsPageContext,
  input: NativeRecordUpdateInput,
  key: string,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? config.holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== input.holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(
      new NativeRecordRegistryError('NATIVE_RECORD_REGISTRY_AUTHORITY_INVALID'),
    );
  const origin = incoming.get('origin');
  if (!origin || origin !== `https://${host}`)
    return Promise.reject(new NativeRecordRegistryError('ORIGIN_FORBIDDEN', 403));
  const value = (entry: unknown): entry is string | number | boolean =>
    typeof entry === 'string'
      ? entry.length <= 4_000
      : typeof entry === 'number'
        ? Number.isFinite(entry)
        : typeof entry === 'boolean';
  if (
    !/^[a-z0-9_-]{1,64}$/.test(input.moduleId) ||
    !boundedId(input.recordId) ||
    !integer(input.expectedVersion, 1) ||
    !record(input.fields) ||
    Object.keys(input.fields).length > 64 ||
    Object.entries(input.fields).some(
      ([name, entry]) =>
        name.length < 1 ||
        name.length > 80 ||
        /[\u0000-\u001f\u007f]/.test(name) ||
        !value(entry),
    )
  )
    return Promise.reject(
      new NativeRecordRegistryError('NATIVE_RECORD_REGISTRY_REQUEST_INVALID'),
    );
  if (!/^[A-Za-z0-9_.:-]{8,180}$/.test(key))
    return Promise.reject(new NativeRecordRegistryError('IDEMPOTENCY_KEY_REQUIRED', 422));
  const body = JSON.stringify({
    expectedVersion: input.expectedVersion,
    fields: input.fields,
  });
  if (new TextEncoder().encode(body).length > 64 * 1024)
    return Promise.reject(new NativeRecordRegistryError('NATIVE_RECORD_REGISTRY_REQUEST_INVALID'));
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    'content-type': 'application/json',
    origin,
    'idempotency-key': key,
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority.revision),
    'x-erpx-context-assignment-version': String(actor.authority.assignmentVersion),
  });
  if (actor.branchSlug) headers.set('x-erpx-workspace-branch', actor.branchSlug);
  if (actor.departmentSlug) headers.set('x-erpx-workspace-department', actor.departmentSlug);
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeRecordRegistryError('NATIVE_RECORD_REGISTRY_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/records/${encodeURIComponent(input.moduleId)}/${encodeURIComponent(input.recordId)}`,
    config.base,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, config.timeout);
  return transport('PATCH', url, headers, body, controller.signal).finally(() => {
    clearTimeout(timer);
    signal?.removeEventListener('abort', cancel);
  });
}

function archiveCall(
  transport: NativeRecordRegistryTransport,
  config: ReturnType<typeof configuration>,
  incoming: Pick<Headers, 'get'>,
  actor: NativeRecordsPageContext,
  input: NativeRecordArchiveInput,
  key: string,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? config.holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== input.holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(
      new NativeRecordRegistryError('NATIVE_RECORD_REGISTRY_AUTHORITY_INVALID'),
    );
  const origin = incoming.get('origin');
  if (!origin || origin !== `https://${host}`)
    return Promise.reject(new NativeRecordRegistryError('ORIGIN_FORBIDDEN', 403));
  if (
    !/^[a-z0-9_-]{1,64}$/.test(input.moduleId) ||
    !boundedId(input.recordId) ||
    !integer(input.expectedVersion, 1)
  )
    return Promise.reject(
      new NativeRecordRegistryError('NATIVE_RECORD_REGISTRY_REQUEST_INVALID'),
    );
  if (!/^[A-Za-z0-9_.:-]{8,180}$/.test(key))
    return Promise.reject(new NativeRecordRegistryError('IDEMPOTENCY_KEY_REQUIRED', 422));
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    origin,
    'idempotency-key': key,
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority.revision),
    'x-erpx-context-assignment-version': String(actor.authority.assignmentVersion),
  });
  if (actor.branchSlug) headers.set('x-erpx-workspace-branch', actor.branchSlug);
  if (actor.departmentSlug) headers.set('x-erpx-workspace-department', actor.departmentSlug);
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeRecordRegistryError('NATIVE_RECORD_REGISTRY_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/records/${encodeURIComponent(input.moduleId)}/${encodeURIComponent(input.recordId)}`,
    config.base,
  );
  url.searchParams.set('expectedVersion', String(input.expectedVersion));
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, config.timeout);
  return transport('DELETE', url, headers, undefined, controller.signal).finally(() => {
    clearTimeout(timer);
    signal?.removeEventListener('abort', cancel);
  });
}

function referenceCall(
  transport: NativeRecordRegistryTransport,
  config: ReturnType<typeof configuration>,
  incoming: Pick<Headers, 'get'>,
  actor: NativeRecordsPageContext,
  input: NativeRecordReferenceInput,
  key: string,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? config.holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== input.holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(
      new NativeRecordRegistryError('NATIVE_RECORD_REGISTRY_AUTHORITY_INVALID'),
    );
  const origin = incoming.get('origin');
  if (!origin || origin !== `https://${host}`)
    return Promise.reject(new NativeRecordRegistryError('ORIGIN_FORBIDDEN', 403));
  if (
    !/^[a-z0-9_-]{1,64}$/.test(input.moduleId) ||
    input.moduleId !== 'support' ||
    input.targetModule !== 'support' ||
    !boundedId(input.recordId) ||
    !boundedId(input.targetId) ||
    input.recordId === input.targetId ||
    !referenceKinds.has(input.kind)
  )
    return Promise.reject(
      new NativeRecordRegistryError('NATIVE_RECORD_REGISTRY_REQUEST_INVALID'),
    );
  if (!/^[A-Za-z0-9_.:-]{8,180}$/.test(key))
    return Promise.reject(new NativeRecordRegistryError('IDEMPOTENCY_KEY_REQUIRED', 422));
  const body = JSON.stringify({
    targetModule: input.targetModule,
    targetId: input.targetId,
    kind: input.kind,
  });
  if (new TextEncoder().encode(body).length > 64 * 1024)
    return Promise.reject(new NativeRecordRegistryError('NATIVE_RECORD_REGISTRY_REQUEST_INVALID'));
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    'content-type': 'application/json',
    origin,
    'idempotency-key': key,
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority.revision),
    'x-erpx-context-assignment-version': String(actor.authority.assignmentVersion),
  });
  if (actor.branchSlug) headers.set('x-erpx-workspace-branch', actor.branchSlug);
  if (actor.departmentSlug) headers.set('x-erpx-workspace-department', actor.departmentSlug);
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeRecordRegistryError('NATIVE_RECORD_REGISTRY_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/records/${encodeURIComponent(input.moduleId)}/${encodeURIComponent(input.recordId)}/references`,
    config.base,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, config.timeout);
  return transport('POST', url, headers, body, controller.signal).finally(() => {
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
        return failure('NATIVE_RECORD_REGISTRY_RESPONSE_LIMIT');
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

async function boundedJson(response: Response, limit: number): Promise<unknown> {
  if (response.redirected) {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_RECORD_REGISTRY_UNAVAILABLE', response.status);
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
    return failure(code ?? 'NATIVE_RECORD_REGISTRY_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_RECORD_REGISTRY_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, limit);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_RECORD_REGISTRY_RESPONSE_INVALID');
  }
}

export function nativeRecordRegistryAvailable(): boolean {
  return process.env.ERPX_NATIVE_RECORDS_CONFIG !== undefined;
}

export function nativeRecordRegistryConfiguration(
  raw: string,
): NativeRecordRegistryConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeRecordRegistryError('NATIVE_RECORD_REGISTRY_CONFIGURATION_INVALID');
  }
  if (
    !exact(value, ['privateBaseUrl', 'timeoutMs', 'holdings']) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    !Array.isArray(value.holdings) ||
    !value.holdings.every(
      (holding: unknown) =>
        exact(holding, ['origin', 'holdingSlug', 'holdingId']) &&
        text(holding.origin) &&
        text(holding.holdingSlug) &&
        text(holding.holdingId),
    )
  )
    return failure('NATIVE_RECORD_REGISTRY_CONFIGURATION_INVALID');
  return value as unknown as NativeRecordRegistryConfiguration;
}

export function nativeRecordRegistryFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeRecordRegistryError)) return undefined;
  if (error.code === 'NATIVE_RECORD_REGISTRY_CONTEXT_CHANGED')
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
      code: error.code === 'NATIVE_RECORD_REGISTRY_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return {
      status: 404,
      code: error.code === 'NATIVE_RECORD_REGISTRY_UNAVAILABLE' ? 'NOT_FOUND' : error.code,
      message: error.message,
    };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_RECORD_REGISTRY_UNAVAILABLE',
      message: 'The native Records registry reader is unavailable.',
    };
  if (error.code === 'NATIVE_RECORD_REGISTRY_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_RECORD_REGISTRY_UNAVAILABLE',
      message: 'The native Records registry reader is unavailable.',
    };
  if (error.code === 'NATIVE_RECORD_REGISTRY_REQUEST_INVALID')
    return {
      status: 422,
      code: 'RECORDS_REGISTRY_REQUEST_INVALID',
      message: 'Choose a valid module section and search.',
    };
  if (
    error.code === 'NATIVE_RECORD_REGISTRY_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_RECORD_REGISTRY_COOKIE_LIMIT' ||
    error.code === 'NATIVE_RECORD_REGISTRY_RESPONSE_INVALID' ||
    error.code === 'NATIVE_RECORD_REGISTRY_RESPONSE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_RECORD_REGISTRY_UNAVAILABLE',
      message: 'The native Records registry reader is unavailable.',
    };
  return undefined;
}

export function createNativeRecordRegistryClient(
  raw: NativeRecordRegistryConfiguration,
  transport: NativeRecordRegistryTransport = directHttp,
) {
  const config = configuration(raw);
  const read = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeRecordsPageContext,
    input: NativeRecordRegistryInput,
    paged: boolean,
    signal?: AbortSignal,
  ) => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return failure('NATIVE_RECORD_REGISTRY_CONTEXT_CHANGED');
    const response = await transportCall(
      transport,
      config,
      incoming,
      actor,
      input,
      paged,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeRecordRegistryError) throw error;
      return failure('NATIVE_RECORD_REGISTRY_UNAVAILABLE');
    });
    return boundedJson(response, paged ? 4 * 1024 * 1024 : 32 * 1024 * 1024);
  };
  const write = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeRecordsPageContext,
    input: NativeRecordWriteInput,
    key: string,
    signal?: AbortSignal,
  ) => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return failure('NATIVE_RECORD_REGISTRY_CONTEXT_CHANGED');
    const response = await writeCall(transport, config, incoming, actor, input, key, signal).catch(
      (error: unknown) => {
        if (error instanceof NativeRecordRegistryError) throw error;
        return failure('NATIVE_RECORD_REGISTRY_UNAVAILABLE');
      },
    );
    return boundedJson(response, 64 * 1024);
  };
  const writeUpdate = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeRecordsPageContext,
    input: NativeRecordUpdateInput,
    key: string,
    signal?: AbortSignal,
  ) => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return failure('NATIVE_RECORD_REGISTRY_CONTEXT_CHANGED');
    const response = await updateCall(
      transport,
      config,
      incoming,
      actor,
      input,
      key,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeRecordRegistryError) throw error;
      return failure('NATIVE_RECORD_REGISTRY_UNAVAILABLE');
    });
    return boundedJson(response, 64 * 1024);
  };
  const writeArchive = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeRecordsPageContext,
    input: NativeRecordArchiveInput,
    key: string,
    signal?: AbortSignal,
  ) => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return failure('NATIVE_RECORD_REGISTRY_CONTEXT_CHANGED');
    const response = await archiveCall(
      transport,
      config,
      incoming,
      actor,
      input,
      key,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeRecordRegistryError) throw error;
      return failure('NATIVE_RECORD_REGISTRY_UNAVAILABLE');
    });
    return boundedJson(response, 64 * 1024);
  };
  const writeReference = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeRecordsPageContext,
    input: NativeRecordReferenceInput,
    key: string,
    signal?: AbortSignal,
  ) => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return failure('NATIVE_RECORD_REGISTRY_CONTEXT_CHANGED');
    const response = await referenceCall(
      transport,
      config,
      incoming,
      actor,
      input,
      key,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeRecordRegistryError) throw error;
      return failure('NATIVE_RECORD_REGISTRY_UNAVAILABLE');
    });
    return boundedJson(response, 64 * 1024);
  };
  return {
    create: async (
      incoming: Pick<Headers, 'get'>,
      actor: NativeRecordsPageContext,
      input: NativeRecordWriteInput,
      key: string = crypto.randomUUID(),
      signal?: AbortSignal,
    ): Promise<NativeRecordView> => {
      const value = await write(incoming, actor, input, key, signal);
      if (!exact(value, ['data', 'context']))
        return failure('NATIVE_RECORD_REGISTRY_RESPONSE_INVALID');
      context(value.context, actor);
      if (!view(value.data, input.moduleId, input.sectionId))
        return failure('NATIVE_RECORD_REGISTRY_RESPONSE_INVALID');
      return value.data;
    },
    update: async (
      incoming: Pick<Headers, 'get'>,
      actor: NativeRecordsPageContext,
      input: NativeRecordUpdateInput,
      key: string = crypto.randomUUID(),
      signal?: AbortSignal,
    ): Promise<NativeRecordView> => {
      const value = await writeUpdate(incoming, actor, input, key, signal);
      if (!exact(value, ['data', 'context']))
        return failure('NATIVE_RECORD_REGISTRY_RESPONSE_INVALID');
      context(value.context, actor);
      if (!view(value.data, input.moduleId, null))
        return failure('NATIVE_RECORD_REGISTRY_RESPONSE_INVALID');
      return value.data;
    },
    archive: async (
      incoming: Pick<Headers, 'get'>,
      actor: NativeRecordsPageContext,
      input: NativeRecordArchiveInput,
      key: string = crypto.randomUUID(),
      signal?: AbortSignal,
    ): Promise<NativeRecordArchive> => {
      const value = await writeArchive(incoming, actor, input, key, signal);
      if (!exact(value, ['data', 'context']))
        return failure('NATIVE_RECORD_REGISTRY_RESPONSE_INVALID');
      context(value.context, actor);
      if (!archived(value.data, input.moduleId, input.recordId))
        return failure('NATIVE_RECORD_REGISTRY_RESPONSE_INVALID');
      return value.data;
    },
    reference: async (
      incoming: Pick<Headers, 'get'>,
      actor: NativeRecordsPageContext,
      input: NativeRecordReferenceInput,
      key: string = crypto.randomUUID(),
      signal?: AbortSignal,
    ): Promise<NativeRecordReference> => {
      const value = await writeReference(incoming, actor, input, key, signal);
      if (!exact(value, ['data', 'context']))
        return failure('NATIVE_RECORD_REGISTRY_RESPONSE_INVALID');
      context(value.context, actor);
      if (!referenced(value.data, input.moduleId, input.recordId))
        return failure('NATIVE_RECORD_REGISTRY_RESPONSE_INVALID');
      return value.data;
    },
    page: async (
      incoming: Pick<Headers, 'get'>,
      actor: NativeRecordsPageContext,
      input: NativeRecordRegistryInput,
      signal?: AbortSignal,
    ): Promise<NativeRecordRegistryPage> => {
      const value = await read(incoming, actor, input, true, signal);
      if (!exact(value, ['data', 'context']))
        return failure('NATIVE_RECORD_REGISTRY_RESPONSE_INVALID');
      context(value.context, actor);
      return pageData(value.data, input);
    },
    list: async (
      incoming: Pick<Headers, 'get'>,
      actor: NativeRecordsPageContext,
      input: NativeRecordRegistryInput,
      signal?: AbortSignal,
    ): Promise<NativeRecordView[]> => {
      const value = await read(incoming, actor, input, false, signal);
      if (!exact(value, ['data', 'context']))
        return failure('NATIVE_RECORD_REGISTRY_RESPONSE_INVALID');
      context(value.context, actor);
      return listData(value.data, input);
    },
  };
}
