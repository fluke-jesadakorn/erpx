import type {
  OperationsDetail,
  OperationsException,
  OperationsWorkspace,
} from '../../lib/operations-work-model';

/** Rendering input from the retained page authorization, never a grant. */
export type NativeOperationsPageContext = {
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

export type NativeOperationsConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;
type Context = Row & { modules: string[]; permissions: string[] };
export type NativeOperationsReadInput = Readonly<{
  holdingSlug: string;
  id?: string;
  branchSlug?: string;
  departmentSlug?: string;
  expected?: Readonly<{
    companyId: string;
    assignmentId: string;
    revision: number;
    assignmentVersion: number;
  }>;
}>;
export type NativeOperationsTransport = (
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
) => Promise<Response>;

export type NativeOperationsReceipt = Readonly<{
  id: string;
  status: string;
  version: number;
}>;

export class NativeOperationsError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Operations request could not be completed.');
    this.name = 'NativeOperationsError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeOperationsError(code, status, message);
};
const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string => text(value, 200) && value.length > 0;
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const optionalText = (value: unknown, max = 4096) => value === null || text(value, max);
const texts = (value: unknown, max = 256): value is string[] =>
  Array.isArray(value) && value.length <= max && value.every((item) => text(item, 256));
const sameSet = (left: readonly string[], right: readonly string[]) => {
  const a = new Set(left);
  const b = new Set(right);
  return a.size === b.size && [...a].every((value) => b.has(value));
};
const priorities = new Set(['normal', 'high', 'critical']);
const statuses = new Set([
  'open',
  'acknowledged',
  'in_progress',
  'escalated',
  'awaiting_review',
  'resolved',
  'closed',
  'cancelled',
]);
const sources = new Set([
  'commerce_order',
  'support_ticket',
  'staff_task',
  'pos_sale',
  'pms_record',
  'asset',
]);
const policyStatuses = new Set(['draft', 'published', 'rejected', 'retired']);
const policyChangeStatuses: Record<string, string> = {
  publish: 'published',
  reject: 'rejected',
  retire: 'retired',
};
const exceptionChangeStatuses: Record<string, readonly string[]> = {
  acknowledge: ['acknowledged'],
  start: ['in_progress'],
  resolve: ['awaiting_review'],
  approve: ['resolved'],
  reject: ['in_progress'],
  close: ['closed'],
  reopen: ['in_progress'],
  assign: ['open', 'acknowledged', 'in_progress', 'escalated'],
  prioritize: ['open', 'acknowledged', 'in_progress', 'escalated'],
  escalate: ['escalated'],
  cancel: ['cancelled'],
};
const commandKey = (value: unknown): value is string =>
  typeof value === 'string' &&
  value.length >= 8 &&
  value.length <= 180 &&
  /^[A-Za-z0-9:_.-]+$/.test(value);
const nonEmptyReason = (value: unknown) =>
  typeof value === 'string' && value.trim().length >= 3 && value.length <= 4000;

type CommandExpectation = Readonly<{
  id: string;
  statuses: readonly string[];
  version: number;
}>;

function commandExpectation(body: string, key: string): CommandExpectation {
  if (new TextEncoder().encode(body).length > 64 * 1024)
    return failure('NATIVE_OPERATIONS_REQUEST_INVALID');
  let value: unknown;
  try {
    value = JSON.parse(body);
  } catch {
    return failure('NATIVE_OPERATIONS_REQUEST_INVALID');
  }
  if (!record(value)) return failure('NATIVE_OPERATIONS_REQUEST_INVALID');
  const entity = value.entity;
  const id = value.id;
  const action = value.action;
  const expected = value.expectedVersion;
  if (entity === 'policy') {
    if (id === undefined) {
      if (
        typeof value.area !== 'string' ||
        typeof value.name !== 'string' ||
        !integer(value.targetMinutes, 1) ||
        !Array.isArray(value.sourceKinds) ||
        value.sourceKinds.length < 1 ||
        value.sourceKinds.length > 7 ||
        !value.sourceKinds.every((kind) => typeof kind === 'string')
      )
        return failure('NATIVE_OPERATIONS_REQUEST_INVALID');
      return { id: `queue-policy-${key}`, statuses: ['draft'], version: 1 };
    }
    if (
      !identifier(id) ||
      typeof action !== 'string' ||
      !Object.hasOwn(policyChangeStatuses, action) ||
      !integer(expected, 1) ||
      !nonEmptyReason(value.reason)
    )
      return failure('NATIVE_OPERATIONS_REQUEST_INVALID');
    return {
      id,
      statuses: [policyChangeStatuses[action]!],
      version: expected + 1,
    };
  }
  if (entity === 'exception') {
    if (id === undefined) {
      const source = value.source;
      if (
        !record(source) ||
        !sources.has(String(source.kind)) ||
        !identifier(source.id) ||
        !integer(source.version, 1) ||
        !identifier(value.policyId) ||
        !integer(value.policyVersion, 1) ||
        !identifier(value.ownerId) ||
        typeof value.title !== 'string' ||
        value.title.trim().length < 3 ||
        value.title.length > 160 ||
        typeof value.issue !== 'string' ||
        value.issue.trim().length < 3 ||
        value.issue.length > 4000
      )
        return failure('NATIVE_OPERATIONS_REQUEST_INVALID');
      return {
        id: `operations-exception-${key}`,
        statuses: ['open'],
        version: 1,
      };
    }
    if (
      !identifier(id) ||
      typeof action !== 'string' ||
      !Object.hasOwn(exceptionChangeStatuses, action) ||
      !integer(expected, 1) ||
      !nonEmptyReason(value.reason)
    )
      return failure('NATIVE_OPERATIONS_REQUEST_INVALID');
    return {
      id,
      statuses: exceptionChangeStatuses[action]!,
      version: expected + 1,
    };
  }
  return failure('NATIVE_OPERATIONS_REQUEST_INVALID');
}

function commandReceipt(value: unknown, expected: CommandExpectation): NativeOperationsReceipt {
  if (
    !exact(value, ['id', 'status', 'version']) ||
    value.id !== expected.id ||
    typeof value.status !== 'string' ||
    !expected.statuses.includes(value.status) ||
    value.version !== expected.version
  )
    return failure('NATIVE_OPERATIONS_RESPONSE_INVALID');
  return value as unknown as NativeOperationsReceipt;
}

const sourceView = (value: unknown) =>
  exact(value, [
    'kind',
    'id',
    'version',
    'title',
    'href',
    'label',
    'companyId',
    'branchId',
    'departmentId',
    'status',
  ]) &&
  sources.has(String(value.kind)) &&
  identifier(value.id) &&
  integer(value.version, 1) &&
  text(value.title, 400) &&
  text(value.href, 2048) &&
  text(value.label, 600) &&
  identifier(value.companyId) &&
  optionalText(value.branchId, 200) &&
  optionalText(value.departmentId, 200) &&
  text(value.status, 200);

const exceptionRow = (value: unknown): value is OperationsException =>
  exact(value, [
    'id',
    'company_id',
    'branch_id',
    'department_id',
    'area',
    'source_kind',
    'source_id',
    'source_version',
    'source_reference_json',
    'policy_id',
    'policy_snapshot_json',
    'title',
    'issue',
    'priority',
    'status',
    'owner_id',
    'escalation_owner_id',
    'resolution',
    'resolved_by',
    'reviewed_by',
    'version',
    'created_by',
    'created_at',
    'updated_at',
    'due_at',
    'source',
  ]) &&
  identifier(value.id) &&
  identifier(value.company_id) &&
  optionalText(value.branch_id, 200) &&
  optionalText(value.department_id, 200) &&
  identifier(value.area) &&
  sources.has(String(value.source_kind)) &&
  identifier(value.source_id) &&
  integer(value.source_version, 1) &&
  text(value.source_reference_json, 2_097_152) &&
  identifier(value.policy_id) &&
  text(value.policy_snapshot_json, 2_097_152) &&
  text(value.title, 160) &&
  text(value.issue, 4000) &&
  priorities.has(String(value.priority)) &&
  statuses.has(String(value.status)) &&
  identifier(value.owner_id) &&
  optionalText(value.escalation_owner_id, 200) &&
  optionalText(value.resolution, 4000) &&
  optionalText(value.resolved_by, 200) &&
  optionalText(value.reviewed_by, 200) &&
  integer(value.version, 1) &&
  identifier(value.created_by) &&
  integer(value.created_at) &&
  integer(value.updated_at) &&
  integer(value.due_at) &&
  sourceView(value.source) &&
  (value.source as Row).kind === value.source_kind &&
  (value.source as Row).id === value.source_id &&
  (value.source as Row).companyId === value.company_id;

const policyRow = (value: unknown) =>
  exact(value, [
    'id',
    'company_id',
    'area',
    'name',
    'revision',
    'source_kinds_json',
    'target_minutes',
    'status',
    'created_by',
    'reviewed_by',
    'version',
    'created_at',
    'updated_at',
  ]) &&
  identifier(value.id) &&
  identifier(value.company_id) &&
  identifier(value.area) &&
  text(value.name, 160) &&
  integer(value.revision, 1) &&
  text(value.source_kinds_json, 4096) &&
  integer(value.target_minutes, 1) &&
  policyStatuses.has(String(value.status)) &&
  identifier(value.created_by) &&
  optionalText(value.reviewed_by, 200) &&
  integer(value.version, 1) &&
  integer(value.created_at) &&
  integer(value.updated_at);

const personRow = (value: unknown) =>
  exact(value, ['id', 'name']) && identifier(value.id) && text(value.name, 400);

const eventRow = (value: unknown) =>
  exact(value, [
    'id',
    'exception_id',
    'version',
    'action',
    'actor_id',
    'reason',
    'status',
    'details_json',
    'created_at',
  ]) &&
  identifier(value.id) &&
  identifier(value.exception_id) &&
  integer(value.version, 1) &&
  identifier(value.action) &&
  identifier(value.actor_id) &&
  text(value.reason, 4000) &&
  text(value.status, 200) &&
  text(value.details_json, 2_097_152) &&
  integer(value.created_at);

function summary(value: unknown) {
  if (
    !exact(value, ['active', 'awaitingReview', 'escalated', 'overdue', 'byArea']) ||
    !['active', 'awaitingReview', 'escalated', 'overdue'].every((key) => integer(value[key], 0)) ||
    !record(value.byArea) ||
    Object.keys(value.byArea).length > 64 ||
    !Object.entries(value.byArea).every(([key, count]) => text(key, 80) && integer(count, 0))
  )
    return failure('NATIVE_OPERATIONS_RESPONSE_INVALID');
  return value;
}

function workspaceData(value: unknown, companyId: string): OperationsWorkspace {
  if (!record(value) || !exact(value, ['exceptions', 'policies', 'people', 'actorId', 'canWrite', 'canReview', 'summary']))
    return failure('NATIVE_OPERATIONS_RESPONSE_INVALID');
  if (!Array.isArray(value.exceptions) || value.exceptions.length > 200)
    return failure('NATIVE_OPERATIONS_RESPONSE_INVALID');
  if (!value.exceptions.every((row) => record(row) && exceptionRow(row)))
    return failure('NATIVE_OPERATIONS_RESPONSE_INVALID');
  if (!Array.isArray(value.policies) || value.policies.length > 500 || !value.policies.every((row) => record(row) && policyRow(row)))
    return failure('NATIVE_OPERATIONS_RESPONSE_INVALID');
  if (!Array.isArray(value.people) || value.people.length > 1000 || !value.people.every((row) => record(row) && personRow(row)))
    return failure('NATIVE_OPERATIONS_RESPONSE_INVALID');
  if (!identifier(value.actorId) || typeof value.canWrite !== 'boolean' || typeof value.canReview !== 'boolean')
    return failure('NATIVE_OPERATIONS_RESPONSE_INVALID');
  summary(value.summary);
  if (value.exceptions.some((row) => (row as OperationsException).company_id !== companyId))
    return failure('NATIVE_OPERATIONS_RESPONSE_INVALID');
  if (value.policies.some((row) => String((row as Row).company_id) !== companyId))
    return failure('NATIVE_OPERATIONS_RESPONSE_INVALID');
  return value as unknown as OperationsWorkspace;
}

function detailData(value: unknown, input: NativeOperationsReadInput, companyId: string): OperationsDetail {
  if (!record(value)) return failure('NATIVE_OPERATIONS_RESPONSE_INVALID');
  const { row, history, ...workspaceFields } = value;
  const workspace = workspaceData(workspaceFields, companyId);
  if (!record(row) || !exceptionRow(row)) return failure('NATIVE_OPERATIONS_RESPONSE_INVALID');
  if (!input.id || row.id !== input.id) return failure('NATIVE_OPERATIONS_RESPONSE_INVALID');
  if (!Array.isArray(history) || history.length > 500)
    return failure('NATIVE_OPERATIONS_RESPONSE_INVALID');
  if (!history.every((event) => record(event) && eventRow(event)))
    return failure('NATIVE_OPERATIONS_RESPONSE_INVALID');
  if (history.some((event) => (event as Row).exception_id !== input.id))
    return failure('NATIVE_OPERATIONS_RESPONSE_INVALID');
  return { ...workspace, row, history } as unknown as OperationsDetail;
}

function context(value: unknown, actor: NativeOperationsPageContext, input: NativeOperationsReadInput): Context {
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
    return failure('NATIVE_OPERATIONS_RESPONSE_INVALID');
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
    return failure('NATIVE_OPERATIONS_CONTEXT_CHANGED');
  return parsed;
}

function envelope(value: unknown, actor: NativeOperationsPageContext, input: NativeOperationsReadInput) {
  if (!exact(value, ['data', 'context'])) return failure('NATIVE_OPERATIONS_RESPONSE_INVALID');
  context(value.context, actor, input);
  return value.data;
}

function workspace(value: unknown, actor: NativeOperationsPageContext, input: NativeOperationsReadInput): OperationsWorkspace {
  return workspaceData(envelope(value, actor, input), actor.companyId);
}
function detail(value: unknown, actor: NativeOperationsPageContext, input: NativeOperationsReadInput): OperationsDetail {
  return detailData(envelope(value, actor, input), input, actor.companyId);
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativeOperationsConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_OPERATIONS_CONFIGURATION_INVALID');
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
    return failure('NATIVE_OPERATIONS_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_OPERATIONS_CONFIGURATION_INVALID');
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
      return failure('NATIVE_OPERATIONS_CONFIGURATION_INVALID');
    holdings.set(origin.host, { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativeOperationsTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  input: NativeOperationsReadInput,
  method: 'GET' | 'POST',
  body: string | undefined,
  key: string | undefined,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== input.holdingSlug)
    return Promise.reject(new NativeOperationsError('NATIVE_OPERATIONS_AUTHORITY_INVALID'));
  const headers = new Headers({ host: host!, accept: 'application/json', 'cache-control': 'no-cache' });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeOperationsError('NATIVE_OPERATIONS_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  for (const [name, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return Promise.reject(new NativeOperationsError('NATIVE_OPERATIONS_REQUEST_INVALID'));
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
      return Promise.reject(new NativeOperationsError('NATIVE_OPERATIONS_REQUEST_INVALID'));
    headers.set('x-erpx-context-company', input.expected.companyId);
    headers.set('x-erpx-context-assignment', input.expected.assignmentId);
    headers.set('x-erpx-context-revision', String(input.expected.revision));
    headers.set('x-erpx-context-assignment-version', String(input.expected.assignmentVersion));
  }
  if (method === 'POST') {
    if (!commandKey(key) || body === undefined)
      return Promise.reject(new NativeOperationsError('NATIVE_OPERATIONS_REQUEST_INVALID'));
    headers.set('content-type', 'application/json');
    headers.set('idempotency-key', key);
  } else if (body !== undefined) {
    return Promise.reject(new NativeOperationsError('NATIVE_OPERATIONS_REQUEST_INVALID'));
  }
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/operations`, base);
  if (method === 'GET' && input.id !== undefined) url.searchParams.set('id', input.id);
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, 30000);
  return transport(method, url, headers, body, controller.signal).finally(() => {
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
        return failure('NATIVE_OPERATIONS_RESPONSE_LIMIT');
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
    return failure('NATIVE_OPERATIONS_UNAVAILABLE', response.status);
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
    return failure(code ?? 'NATIVE_OPERATIONS_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_OPERATIONS_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_OPERATIONS_RESPONSE_INVALID');
  }
}

const directHttp: NativeOperationsTransport = (method, url, headers, body, signal) =>
  fetch(url, { method, headers, body, cache: 'no-store', redirect: 'error', signal });

export function createNativeOperationsClient(
  raw: NativeOperationsConfiguration,
  transport: NativeOperationsTransport = directHttp,
) {
  const config = configuration(raw);
  const request = (
    incoming: Pick<Headers, 'get'>,
    actor: NativeOperationsPageContext,
    input: NativeOperationsReadInput,
    method: 'GET' | 'POST',
    body: string | undefined,
    key: string | undefined,
    signal?: AbortSignal,
  ) => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return Promise.reject(new NativeOperationsError('NATIVE_OPERATIONS_CONTEXT_CHANGED'));
    return transportCall(
      transport,
      config.base,
      config.holdings,
      incoming,
      input,
      method,
      body,
      key,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeOperationsError) throw error;
      return failure('NATIVE_OPERATIONS_UNAVAILABLE');
    });
  };
  return {
    workspace: async (
      incoming: Pick<Headers, 'get'>,
      actor: NativeOperationsPageContext,
      input: NativeOperationsReadInput,
      signal?: AbortSignal,
    ) =>
      workspace(
        await boundedJson(await request(incoming, actor, input, 'GET', undefined, undefined, signal)),
        actor,
        input,
      ),
    detail: async (
      incoming: Pick<Headers, 'get'>,
      actor: NativeOperationsPageContext,
      input: NativeOperationsReadInput & { id: string },
      signal?: AbortSignal,
    ) =>
      detail(
        await boundedJson(await request(incoming, actor, input, 'GET', undefined, undefined, signal)),
        actor,
        input,
      ),
    command: async (
      incoming: Pick<Headers, 'get'>,
      actor: NativeOperationsPageContext,
      input: NativeOperationsReadInput,
      body: string,
      key: string,
      signal?: AbortSignal,
    ): Promise<NativeOperationsReceipt> => {
      if (!commandKey(key)) return failure('NATIVE_OPERATIONS_REQUEST_INVALID');
      const expected = commandExpectation(body, key);
      const value = await boundedJson(
        await request(incoming, actor, input, 'POST', body, key, signal),
      );
      if (!exact(value, ['data', 'context'])) return failure('NATIVE_OPERATIONS_RESPONSE_INVALID');
      context(value.context, actor, input);
      return commandReceipt(value.data, expected);
    },
  };
}

export function nativeOperationsConfiguration(raw: string): NativeOperationsConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeOperationsError('NATIVE_OPERATIONS_CONFIGURATION_INVALID');
  }
  if (!record(value) || !text(value.privateBaseUrl) || !integer(value.timeoutMs) || !Array.isArray(value.holdings))
    throw new NativeOperationsError('NATIVE_OPERATIONS_CONFIGURATION_INVALID');
  if (
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) && text(holding.origin) && text(holding.holdingSlug) && text(holding.holdingId),
    )
  )
    throw new NativeOperationsError('NATIVE_OPERATIONS_CONFIGURATION_INVALID');
  return value as unknown as NativeOperationsConfiguration;
}
