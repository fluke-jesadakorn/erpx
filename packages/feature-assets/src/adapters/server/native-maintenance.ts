import 'server-only';
import type {
  NativeMaintenanceDetail,
  NativeMaintenancePlan,
  NativeMaintenancePlanDetail,
  NativeMaintenanceWorkspace,
} from '../../lib/maintenance-view';
import { directHttp } from './native-assets';

/** Rendering input from the retained page authorization, never a grant. */
export type NativeMaintenancePageContext = {
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

export type NativeMaintenanceConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;
type Context = Row & { modules: string[]; permissions: string[] };
export type NativeMaintenanceReadInput = Readonly<{
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
export type NativeMaintenanceCommandInput = Readonly<{
  holdingSlug: string;
  branchSlug?: string;
  departmentSlug?: string;
  expected?: NativeMaintenanceReadInput['expected'];
  body: unknown;
  idempotencyKey: string;
  origin?: string;
}>;
export type NativeMaintenanceCommandResult = Readonly<{ id: string; version: number; status: string }>;
export type NativeMaintenanceTransport = (
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeMaintenanceError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Maintenance request could not be completed.');
    this.name = 'NativeMaintenanceError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeMaintenanceError(code, status, message);
};
const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) && Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string => text(value, 200) && value.length > 0;
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const safeInteger = (value: unknown): value is number => integer(value, Number.MIN_SAFE_INTEGER);
const optionalText = (value: unknown, max = 4096) => value === null || text(value, max);
const optionalInteger = (value: unknown) => value === null || safeInteger(value);
const texts = (value: unknown, max = 256): value is string[] =>
  Array.isArray(value) && value.length <= max && value.every((item) => text(item, 256));
const jsonText = (value: unknown, max = 65536): value is string =>
  text(value, max) && (() => { try { JSON.parse(value); return true; } catch { return false; } })();
const sameSet = (left: readonly string[], right: readonly string[]) => {
  const a = new Set(left);
  const b = new Set(right);
  return a.size === b.size && [...a].every((value) => b.has(value));
};
const workStatuses = new Set(['open', 'scheduled', 'in_progress', 'blocked', 'completed', 'cancelled']);
const priorities = new Set(['normal', 'high', 'critical']);
const conditions = new Set(['good', 'degraded', 'unsafe']);
const planStatuses = new Set(['draft', 'active', 'paused', 'cancelled']);

const workRow = (row: Row) =>
  exact(row, [
    'id', 'company_id', 'branch_id', 'department_id', 'asset_id', 'title', 'priority', 'status',
    'owner_id', 'scheduled_at', 'downtime_started_at', 'downtime_ended_at', 'blocked_reason',
    'completion_evidence', 'resulting_condition', 'version', 'created_by', 'created_at',
    'updated_at', 'asset_code', 'asset_name', 'owner_name', 'owner_eligible', 'can_review',
  ]) &&
  identifier(row.id) &&
  identifier(row.company_id) &&
  identifier(row.branch_id) &&
  optionalText(row.department_id, 200) &&
  identifier(row.asset_id) &&
  text(row.title, 160) &&
  priorities.has(String(row.priority)) &&
  workStatuses.has(String(row.status)) &&
  identifier(row.owner_id) &&
  optionalInteger(row.scheduled_at) &&
  optionalInteger(row.downtime_started_at) &&
  optionalInteger(row.downtime_ended_at) &&
  optionalText(row.blocked_reason, 2000) &&
  optionalText(row.completion_evidence, 4000) &&
  (row.resulting_condition === null || conditions.has(String(row.resulting_condition))) &&
  integer(row.version, 1) &&
  identifier(row.created_by) &&
  integer(row.created_at) &&
  integer(row.updated_at) &&
  text(row.asset_code, 200) &&
  text(row.asset_name, 400) &&
  optionalText(row.owner_name, 400) &&
  [0, 1].includes(Number(row.owner_eligible)) &&
  [0, 1].includes(Number(row.can_review));

const assetRow = (row: Row) =>
  exact(row, ['id', 'code', 'name', 'version', 'condition', 'lifecycle_status']) &&
  identifier(row.id) &&
  text(row.code, 200) &&
  text(row.name, 400) &&
  integer(row.version, 1) &&
  conditions.has(String(row.condition)) &&
  ['registered', 'in_service', 'maintenance'].includes(String(row.lifecycle_status));

const personRow = (row: Row) =>
  exact(row, ['id', 'name']) && identifier(row.id) && text(row.name, 400);
const nodeRow = (row: Row) =>
  exact(row, ['id', 'kind', 'display_name', 'branch_id']) &&
  identifier(row.id) &&
  identifier(row.kind) &&
  text(row.display_name, 400) &&
  optionalText(row.branch_id, 200);

const eventRow = (row: Row) =>
  exact(row, [
    'id', 'work_order_id', 'version', 'action', 'actor_id', 'reason', 'status', 'owner_id',
    'scheduled_at', 'downtime_started_at', 'downtime_ended_at', 'created_at',
  ]) &&
  identifier(row.id) &&
  identifier(row.work_order_id) &&
  integer(row.version, 1) &&
  identifier(row.action) &&
  identifier(row.actor_id) &&
  text(row.reason, 4000) &&
  workStatuses.has(String(row.status)) &&
  identifier(row.owner_id) &&
  optionalInteger(row.scheduled_at) &&
  optionalInteger(row.downtime_started_at) &&
  optionalInteger(row.downtime_ended_at) &&
  integer(row.created_at);

const planRow = (row: Row) =>
  exact(row, [
    'id', 'company_id', 'branch_id', 'department_id', 'asset_id', 'title', 'priority', 'owner_id',
    'interval_days', 'time_zone', 'first_due_local', 'next_due_local', 'next_due_at', 'lead_days',
    'status', 'version', 'created_by', 'edited_by', 'approved_by', 'approved_at', 'created_at',
    'updated_at', 'asset_code', 'asset_name', 'owner_name', 'owner_eligible',
  ]) &&
  identifier(row.id) &&
  identifier(row.company_id) &&
  identifier(row.branch_id) &&
  optionalText(row.department_id, 200) &&
  identifier(row.asset_id) &&
  text(row.title, 160) &&
  priorities.has(String(row.priority)) &&
  identifier(row.owner_id) &&
  integer(row.interval_days, 1) &&
  text(row.time_zone, 80) &&
  text(row.first_due_local, 64) &&
  text(row.next_due_local, 64) &&
  integer(row.next_due_at) &&
  integer(row.lead_days, 0) &&
  planStatuses.has(String(row.status)) &&
  integer(row.version, 1) &&
  identifier(row.created_by) &&
  identifier(row.edited_by) &&
  optionalText(row.approved_by, 200) &&
  optionalInteger(row.approved_at) &&
  integer(row.created_at) &&
  integer(row.updated_at) &&
  text(row.asset_code, 200) &&
  text(row.asset_name, 400) &&
  optionalText(row.owner_name, 400) &&
  [0, 1].includes(Number(row.owner_eligible));

const planEventRow = (row: Row) =>
  exact(row, ['id', 'plan_id', 'version', 'action', 'actor_id', 'reason', 'created_at']) &&
  identifier(row.id) &&
  identifier(row.plan_id) &&
  integer(row.version, 1) &&
  identifier(row.action) &&
  identifier(row.actor_id) &&
  text(row.reason, 4000) &&
  integer(row.created_at);
const occurrenceRow = (row: Row) =>
  exact(row, [
    'id', 'plan_id', 'plan_version', 'work_order_id', 'due_local', 'due_at',
    'source_asset_version', 'generated_by', 'generated_at', 'status', 'work_version',
    'definition_json',
  ]) &&
  identifier(row.id) &&
  identifier(row.plan_id) &&
  integer(row.plan_version, 1) &&
  identifier(row.work_order_id) &&
  text(row.due_local, 64) &&
  integer(row.due_at) &&
  integer(row.source_asset_version, 1) &&
  identifier(row.generated_by) &&
  integer(row.generated_at) &&
  workStatuses.has(String(row.status)) &&
  integer(row.work_version, 1) &&
  jsonText(row.definition_json);

function context(value: unknown, actor: NativeMaintenancePageContext, input: NativeMaintenanceReadInput): Context {
  if (
    !exact(value, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug',
      'departmentSlug', 'assignmentId', 'assignmentVersion', 'revision', 'role',
      'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ])
  )
    return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
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
    return failure('NATIVE_MAINTENANCE_CONTEXT_CHANGED');
  return parsed;
}

function workspaceData(value: unknown): NativeMaintenanceWorkspace {
  if (!record(value) || !Array.isArray(value.items) || value.items.length > 2_000)
    return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  if (!Array.isArray(value.assets) || value.assets.length > 1_000) return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  if (!Array.isArray(value.people) || value.people.length > 1_000) return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  if (!Array.isArray(value.nodes) || value.nodes.length > 1_000) return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  if (!value.items.every((row) => record(row) && workRow(row))) return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  if (!value.assets.every((row) => record(row) && assetRow(row))) return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  if (!value.people.every((row) => record(row) && personRow(row))) return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  if (!value.nodes.every((row) => record(row) && nodeRow(row))) return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  if (typeof value.canCreate !== 'boolean') return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  return value as unknown as NativeMaintenanceWorkspace;
}

function workspace(value: unknown, actor: NativeMaintenancePageContext, input: NativeMaintenanceReadInput) {
  if (!exact(value, ['data', 'context'])) return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  context(value.context, actor, input);
  return workspaceData(value.data);
}

function detail(value: unknown, actor: NativeMaintenancePageContext, input: NativeMaintenanceReadInput): NativeMaintenanceDetail {
  if (!exact(value, ['data', 'context'])) return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  context(value.context, actor, input);
  const data = value.data;
  const base = workspaceData(data);
  if (!record(data) || !record(data.work) || !workRow(data.work)) return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  if (!Array.isArray(data.history) || data.history.length > 500 || !data.history.every((row) => record(row) && eventRow(row)))
    return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  if (!Array.isArray(data.consumptions) || data.consumptions.length > 500)
    return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  if (!record(data.spares) || !Array.isArray(data.spares.stock) || !Array.isArray(data.spares.lots))
    return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  return {
    ...base,
    work: data.work as unknown as NativeMaintenanceDetail['work'],
    history: data.history as unknown as NativeMaintenanceDetail['history'],
    consumptions: data.consumptions as NativeMaintenanceDetail['consumptions'],
    spares: data.spares as NativeMaintenanceDetail['spares'],
  };
}

function planList(value: unknown, actor: NativeMaintenancePageContext, input: NativeMaintenanceReadInput): NativeMaintenancePlan[] {
  if (!exact(value, ['data', 'context'])) return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  context(value.context, actor, input);
  if (!Array.isArray(value.data) || value.data.length > 300 || !value.data.every((row) => record(row) && planRow(row)))
    return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  return value.data as unknown as NativeMaintenancePlan[];
}

function planDetail(value: unknown, actor: NativeMaintenancePageContext, input: NativeMaintenanceReadInput): NativeMaintenancePlanDetail {
  if (!exact(value, ['data', 'context'])) return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  context(value.context, actor, input);
  const data = value.data;
  if (!record(data) || !record(data.plan) || !planRow(data.plan)) return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  if (!identifier(data.actorId) || typeof data.canWrite !== 'boolean' || typeof data.canReview !== 'boolean')
    return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  if (data.assetVersion !== null && !integer(data.assetVersion, 1)) return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  if (!Array.isArray(data.people) || data.people.length > 1_000 || !data.people.every((row) => record(row) && personRow(row)))
    return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  if (!Array.isArray(data.history) || data.history.length > 500 || !data.history.every((row) => record(row) && planEventRow(row)))
    return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  if (!Array.isArray(data.occurrences) || data.occurrences.length > 500 || !data.occurrences.every((row) => record(row) && occurrenceRow(row)))
    return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  return data as unknown as NativeMaintenancePlanDetail;
}

function commandResult(value: unknown, actor: NativeMaintenancePageContext, input: NativeMaintenanceReadInput): NativeMaintenanceCommandResult {
  if (!exact(value, ['data', 'context'])) return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  context(value.context, actor, input);
  if (!exact(value.data, ['id', 'version', 'status']) || !identifier(value.data.id) || !integer(value.data.version, 1) || !identifier(value.data.status))
    return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  return value.data as unknown as NativeMaintenanceCommandResult;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativeMaintenanceConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_MAINTENANCE_CONFIGURATION_INVALID');
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
    return failure('NATIVE_MAINTENANCE_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_MAINTENANCE_CONFIGURATION_INVALID');
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
      return failure('NATIVE_MAINTENANCE_CONFIGURATION_INVALID');
    holdings.set(origin.host, { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativeMaintenanceTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  holdingSlug: string,
  input: NativeMaintenanceReadInput,
  path: string,
  body: string | undefined,
  idempotencyKey?: string,
  origin?: string,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== holdingSlug)
    return Promise.reject(new NativeMaintenanceError('NATIVE_MAINTENANCE_AUTHORITY_INVALID'));
  const headers = new Headers({ host: host!, accept: 'application/json', 'cache-control': 'no-cache' });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeMaintenanceError('NATIVE_MAINTENANCE_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  for (const [key, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return Promise.reject(new NativeMaintenanceError('NATIVE_MAINTENANCE_REQUEST_INVALID'));
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
      return Promise.reject(new NativeMaintenanceError('NATIVE_MAINTENANCE_REQUEST_INVALID'));
    headers.set('x-erpx-context-company', input.expected.companyId);
    headers.set('x-erpx-context-assignment', input.expected.assignmentId);
    headers.set('x-erpx-context-revision', String(input.expected.revision));
    headers.set('x-erpx-context-assignment-version', String(input.expected.assignmentVersion));
  }
  if (body !== undefined) {
    if (new TextEncoder().encode(body).length > 64 * 1024)
      return Promise.reject(new NativeMaintenanceError('NATIVE_MAINTENANCE_REQUEST_INVALID'));
    headers.set('content-type', 'application/json');
    if (!idempotencyKey || !/^[A-Za-z0-9:_.-]{8,180}$/.test(idempotencyKey))
      return Promise.reject(new NativeMaintenanceError('NATIVE_MAINTENANCE_COMMAND_KEY_INVALID'));
    headers.set('idempotency-key', idempotencyKey);
    const effectiveOrigin = origin ?? `https://${host}`;
    let originUrl: URL;
    try {
      originUrl = new URL(effectiveOrigin);
    } catch {
      return Promise.reject(new NativeMaintenanceError('NATIVE_MAINTENANCE_REQUEST_INVALID'));
    }
    if (effectiveOrigin !== originUrl.origin)
      return Promise.reject(new NativeMaintenanceError('NATIVE_MAINTENANCE_REQUEST_INVALID'));
    headers.set('origin', effectiveOrigin);
  }
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(holdingSlug)}${path}`, base);
  if (input.id !== undefined) url.searchParams.set('id', input.id);
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, 30000);
  return transport(body === undefined ? 'GET' : 'POST', url, headers, body, controller.signal).finally(() => {
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
        return failure('NATIVE_MAINTENANCE_RESPONSE_LIMIT');
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
    return failure('NATIVE_MAINTENANCE_UNAVAILABLE', response.status);
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
    return failure(code ?? 'NATIVE_MAINTENANCE_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_MAINTENANCE_RESPONSE_INVALID');
  }
}

export function createNativeMaintenanceClient(
  raw: NativeMaintenanceConfiguration,
  transport: NativeMaintenanceTransport = directHttp,
) {
  const config = configuration(raw);
  const request = (
    incoming: Pick<Headers, 'get'>,
    actor: NativeMaintenancePageContext,
    input: NativeMaintenanceReadInput,
    path: string,
    body: string | undefined,
    idempotencyKey?: string,
    origin?: string,
    signal?: AbortSignal,
  ) => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return Promise.reject(new NativeMaintenanceError('NATIVE_MAINTENANCE_CONTEXT_CHANGED'));
    return transportCall(
      transport, config.base, config.holdings, incoming, input.holdingSlug, input, path,
      body, idempotencyKey, origin, signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeMaintenanceError) throw error;
      return failure('NATIVE_MAINTENANCE_UNAVAILABLE');
    });
  };
  return {
    workspace: async (incoming: Pick<Headers, 'get'>, actor: NativeMaintenancePageContext, input: NativeMaintenanceReadInput, signal?: AbortSignal) =>
      workspace(await boundedJson(await request(incoming, actor, input, '/maintenance', undefined, undefined, undefined, signal)), actor, input),
    detail: async (incoming: Pick<Headers, 'get'>, actor: NativeMaintenancePageContext, input: NativeMaintenanceReadInput & { id: string }, signal?: AbortSignal) =>
      detail(await boundedJson(await request(incoming, actor, input, '/maintenance', undefined, undefined, undefined, signal)), actor, input),
    plans: async (incoming: Pick<Headers, 'get'>, actor: NativeMaintenancePageContext, input: NativeMaintenanceReadInput, signal?: AbortSignal) =>
      planList(await boundedJson(await request(incoming, actor, input, '/maintenance/plans', undefined, undefined, undefined, signal)), actor, input),
    planDetail: async (incoming: Pick<Headers, 'get'>, actor: NativeMaintenancePageContext, input: NativeMaintenanceReadInput & { id: string }, signal?: AbortSignal) =>
      planDetail(await boundedJson(await request(incoming, actor, input, '/maintenance/plans', undefined, undefined, undefined, signal)), actor, input),
    planCommand: async (incoming: Pick<Headers, 'get'>, actor: NativeMaintenancePageContext, input: NativeMaintenanceCommandInput) => {
      const body = JSON.stringify(input.body);
      const payload = await boundedJson(
        await request(
          incoming,
          actor,
          input,
          '/maintenance/plans',
          body,
          input.idempotencyKey,
          incoming.get('origin') ?? undefined,
        ),
      );
      if (!record(payload) || !('data' in payload)) throw new NativeMaintenanceError('NATIVE_MAINTENANCE_RESPONSE_INVALID');
      return (payload as { data: unknown }).data;
    },
    command: async (incoming: Pick<Headers, 'get'>, actor: NativeMaintenancePageContext, input: NativeMaintenanceCommandInput) => {
      const body = JSON.stringify(input.body);
      return commandResult(
        await boundedJson(await request(incoming, actor, input, '/maintenance', body, input.idempotencyKey, input.origin)),
        actor,
        input,
      );
    },
  };
}

export function nativeMaintenanceConfiguration(raw: string): NativeMaintenanceConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeMaintenanceError('NATIVE_MAINTENANCE_CONFIGURATION_INVALID');
  }
  if (!record(value) || !text(value.privateBaseUrl) || !integer(value.timeoutMs) || !Array.isArray(value.holdings))
    throw new NativeMaintenanceError('NATIVE_MAINTENANCE_CONFIGURATION_INVALID');
  if (
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) && text(holding.origin) && text(holding.holdingSlug) && text(holding.holdingId),
    )
  )
    throw new NativeMaintenanceError('NATIVE_MAINTENANCE_CONFIGURATION_INVALID');
  return value as unknown as NativeMaintenanceConfiguration;
}
