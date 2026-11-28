import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';
import { headers } from 'next/headers';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { ApiError } from '@erpx/platform-runtime-server/public/http';

/**
 * Strict owner transport for the native CRM activity workspace/work GET reads.
 *
 * The axum owner returns the complete typed payload and the validated current
 * context. This bridge emits no SQL, accepts no SQL from a caller and has no
 * TypeScript fallback: missing configuration, an unregistered Holding, a
 * non-200 reply or any unexpected field fails closed. The caller's session and
 * rendered CAS selection are forwarded unchanged; the owner re-authorizes them.
 */
export type NativeCrmActivityPurchase = Readonly<{
  newWork: boolean;
  settleExisting: boolean;
  history: boolean;
  revision: string;
}>;

export type NativeCrmActivityRow = Readonly<{
  id: string;
  company_id: string;
  lead_id: string;
  activity_type: string;
  subject: string;
  details: string;
  owner_id: string;
  due_date: string;
  status: string;
  outcome: string | null;
  version: number;
  created_by: string;
  created_at: number;
  updated_at: number;
  lead_title: string;
  lead_status: string;
  lead_stage: string | null;
  branch_id: string;
  department_id: string | null;
  owner_name: string | null;
  owner_eligible: number;
  ownerEligible: boolean;
  canComplete: boolean;
  canManage: boolean;
  canChange: boolean;
}>;

export type NativeCrmActivityWorkspaceData = Readonly<{
  lead: Readonly<{
    id: string;
    title: string;
    status: string;
    stage: string | null;
    version: number;
  }>;
  items: NativeCrmActivityRow[];
  canCreate: boolean;
  canManage: boolean;
  history: Record<string, unknown>[];
  recipients: { id: string; name: string }[];
  purchase?: NativeCrmActivityPurchase | undefined;
}>;

export type NativeCrmWorkItem = Readonly<{
  id: string;
  type: 'crm_activity';
  title: string;
  detail: string;
  href: string;
  status: string;
  updatedAt: number;
  queue: 'todo' | 'waiting' | 'completed';
  responsibleQueue: string;
  nextAction: string;
  dueAt: number;
  blocker: string | null;
  relationships: ('assigned' | 'requested' | 'team')[];
  canAct?: boolean | undefined;
  actionKind?: 'work' | undefined;
}>;

export type NativeCrmActivityWorkspaceConfiguration = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: readonly Readonly<{
    origin: string;
    holdingSlug: string;
    holdingId: string;
    companyId?: string | undefined;
  }>[];
}>;

export type NativeCrmActivityWorkspaceTransport = (
  url: URL,
  headers: Headers,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeCrmActivityWorkspaceError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native CRM activity workspace read could not be completed.');
    this.name = 'NativeCrmActivityWorkspaceError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeCrmActivityWorkspaceError(code, status, message);
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Record<string, unknown> =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string => text(value, 200) && value.length > 0;
const optionalText = (value: unknown, max = 4096) => value === null || text(value, max);
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const sameSet = (left: readonly string[], right: unknown) =>
  Array.isArray(right) &&
  right.every((item) => typeof item === 'string') &&
  new Set(left).size === new Set(right).size &&
  left.every((item) => (right as string[]).includes(item));
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

const ACTIVITY_KEYS = [
  'id', 'company_id', 'lead_id', 'activity_type', 'subject', 'details', 'owner_id',
  'due_date', 'status', 'outcome', 'version', 'created_by', 'created_at', 'updated_at',
  'lead_title', 'lead_status', 'lead_stage', 'branch_id', 'department_id', 'owner_name',
  'owner_eligible', 'ownerEligible', 'canComplete', 'canManage', 'canChange',
] as const;

const WORK_KEYS = [
  'id', 'type', 'title', 'detail', 'href', 'status', 'updatedAt', 'queue',
  'responsibleQueue', 'nextAction', 'dueAt', 'blocker', 'relationships',
] as const;
const WORK_OPTIONAL_KEYS = new Set(['canAct', 'actionKind', 'flow', 'relationships']);

function purchase(value: unknown): NativeCrmActivityPurchase | undefined {
  if (value === undefined) return undefined;
  if (
    !exact(value, ['newWork', 'settleExisting', 'history', 'revision']) ||
    typeof value.newWork !== 'boolean' ||
    typeof value.settleExisting !== 'boolean' ||
    typeof value.history !== 'boolean' ||
    !identifier(value.revision)
  )
    return failure('NATIVE_CRM_ACTIVITY_RESPONSE_INVALID');
  return {
    newWork: value.newWork,
    settleExisting: value.settleExisting,
    history: value.history,
    revision: value.revision,
  };
}

function activity(value: unknown, actor: WorkContext): value is NativeCrmActivityRow {
  if (!exact(value, ACTIVITY_KEYS)) return false;
  if (
    !identifier(value.id) ||
    !identifier(value.company_id) ||
    !identifier(value.lead_id) ||
    !text(value.activity_type, 120) ||
    !text(value.subject, 4096) ||
    !text(value.details, 8192) ||
    !identifier(value.owner_id) ||
    !text(value.due_date, 32) ||
    !text(value.status, 120) ||
    !optionalText(value.outcome, 4096) ||
    !integer(value.version, 1) ||
    !identifier(value.created_by) ||
    !integer(value.created_at) ||
    !integer(value.updated_at) ||
    !text(value.lead_title, 4096) ||
    !text(value.lead_status, 120) ||
    !optionalText(value.lead_stage, 120) ||
    !identifier(value.branch_id) ||
    !optionalText(value.department_id, 200) ||
    !optionalText(value.owner_name, 4096) ||
    !integer(value.owner_eligible) ||
    typeof value.ownerEligible !== 'boolean' ||
    typeof value.canComplete !== 'boolean' ||
    typeof value.canManage !== 'boolean' ||
    typeof value.canChange !== 'boolean'
  )
    return false;
  return (
    value.company_id === actor.companyId &&
    (actor.branchId === null || value.branch_id === actor.branchId) &&
    (actor.departmentId === null || value.department_id === actor.departmentId)
  );
}

function workspace(
  value: unknown,
  actor: WorkContext,
  leadId: string,
): NativeCrmActivityWorkspaceData {
  if (
    !exact(value, [
      'lead', 'items', 'canCreate', 'canManage', 'history', 'recipients',
    ]) &&
    !exact(value, [
      'lead', 'items', 'canCreate', 'canManage', 'history', 'recipients', 'purchase',
    ])
  )
    return failure('NATIVE_CRM_ACTIVITY_RESPONSE_INVALID');
  const row = value as Record<string, unknown>;
  if (
    !exact(row.lead, ['id', 'title', 'status', 'stage', 'version']) ||
    !identifier((row.lead as Record<string, unknown>).id) ||
    (row.lead as Record<string, unknown>).id !== leadId ||
    !text((row.lead as Record<string, unknown>).title, 4096) ||
    !text((row.lead as Record<string, unknown>).status, 120) ||
    !optionalText((row.lead as Record<string, unknown>).stage, 120) ||
    !integer((row.lead as Record<string, unknown>).version, 1) ||
    !Array.isArray(row.items) ||
    row.items.length > 500 ||
    typeof row.canCreate !== 'boolean' ||
    typeof row.canManage !== 'boolean' ||
    !Array.isArray(row.history) ||
    row.history.length > 5000 ||
    !row.history.every(record) ||
    !Array.isArray(row.recipients) ||
    row.recipients.length > 2000
  )
    return failure('NATIVE_CRM_ACTIVITY_RESPONSE_INVALID');
  for (const item of row.items) {
    if (!activity(item, actor)) return failure('NATIVE_CRM_ACTIVITY_RESPONSE_INVALID');
  }
  for (const recipient of row.recipients) {
    if (
      !exact(recipient, ['id', 'name']) ||
      !identifier((recipient as Record<string, unknown>).id) ||
      !text((recipient as Record<string, unknown>).name, 4096)
    )
      return failure('NATIVE_CRM_ACTIVITY_RESPONSE_INVALID');
  }
  return {
    lead: row.lead as NativeCrmActivityWorkspaceData['lead'],
    items: row.items as NativeCrmActivityRow[],
    canCreate: row.canCreate as boolean,
    canManage: row.canManage as boolean,
    history: row.history as Record<string, unknown>[],
    recipients: row.recipients as NativeCrmActivityWorkspaceData['recipients'],
    purchase: purchase(row.purchase),
  };
}

function work(value: unknown, actor: WorkContext): NativeCrmWorkItem[] {
  if (!Array.isArray(value) || value.length > 5000)
    return failure('NATIVE_CRM_ACTIVITY_RESPONSE_INVALID');
  for (const item of value) {
    if (!record(item)) return failure('NATIVE_CRM_ACTIVITY_RESPONSE_INVALID');
    const keys = Object.keys(item);
    if (
      keys.some((key) => !WORK_KEYS.includes(key as (typeof WORK_KEYS)[number]) && !WORK_OPTIONAL_KEYS.has(key))
    )
      return failure('NATIVE_CRM_ACTIVITY_RESPONSE_INVALID');
    if (
      !WORK_KEYS.every((key) => Object.hasOwn(item, key)) ||
      !identifier(item.id) ||
      item.type !== 'crm_activity' ||
      !text(item.title, 4096) ||
      !text(item.detail, 8192) ||
      !text(item.href, 4096) ||
      !text(item.status, 120) ||
      !integer(item.updatedAt) ||
      !['todo', 'waiting', 'completed'].includes(String(item.queue)) ||
      !text(item.responsibleQueue, 4096) ||
      !text(item.nextAction, 4096) ||
      !integer(item.dueAt, -9_007_199_254_740_991) ||
      !optionalText(item.blocker, 4096) ||
      !Array.isArray(item.relationships) ||
      item.relationships.length > 8 ||
      !item.relationships.every(
        (relationship) =>
          relationship === 'assigned' ||
          relationship === 'requested' ||
          relationship === 'team',
      )
    )
      return failure('NATIVE_CRM_ACTIVITY_RESPONSE_INVALID');
    if (
      (item.canAct !== undefined && typeof item.canAct !== 'boolean') ||
      (item.actionKind !== undefined && item.actionKind !== 'work')
    )
      return failure('NATIVE_CRM_ACTIVITY_RESPONSE_INVALID');
  }
  return value as NativeCrmWorkItem[];
}

function context(value: unknown, actor: WorkContext): void {
  if (
    !exact(value, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug',
      'departmentSlug', 'assignmentId', 'assignmentVersion', 'revision', 'role',
      'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ])
  )
    return failure('NATIVE_CRM_ACTIVITY_CONTEXT_CHANGED');
  const parsed = value as Record<string, unknown>;
  const branchSlug = actor.scope.kind === 'holding' ? null : actor.scope.branchSlug ?? null;
  const departmentSlug = actor.scope.kind === 'department' ? actor.scope.departmentSlug : null;
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
    !sameSet(actor.modules, parsed.modules) ||
    !sameSet(actor.iam?.permissions ?? [], parsed.permissions)
  )
    return failure('NATIVE_CRM_ACTIVITY_CONTEXT_CHANGED');
}

function configuration(raw: NativeCrmActivityWorkspaceConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_CRM_ACTIVITY_CONFIGURATION_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    reservedPorts.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    raw.privateBaseUrl !== base.origin ||
    !integer(raw.timeoutMs, 250) ||
    raw.timeoutMs > 30000 ||
    raw.holdings.length < 1 ||
    raw.holdings.length > 64
  )
    return failure('NATIVE_CRM_ACTIVITY_CONFIGURATION_INVALID');
  const holdings = new Map<string, NativeCrmActivityWorkspaceConfiguration['holdings'][number]>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_CRM_ACTIVITY_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      origin.username ||
      origin.password ||
      !identifier(holding.holdingId) ||
      (holding.companyId !== undefined && !identifier(holding.companyId)) ||
      !text(holding.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug) ||
      holdings.has(origin.host)
    )
      return failure('NATIVE_CRM_ACTIVITY_CONFIGURATION_INVALID');
    holdings.set(origin.host, holding);
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function call(
  transport: NativeCrmActivityWorkspaceTransport,
  settings: ReturnType<typeof configuration>,
  incoming: Pick<Headers, 'get'>,
  actor: WorkContext,
  path: string,
  query?: readonly [string, string][],
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? settings.holdings.get(host) : undefined;
  if (
    !host ||
    !holding ||
    holding.holdingSlug !== actor.holdingSlug ||
    holding.holdingId !== actor.holdingId ||
    (holding.companyId !== undefined && holding.companyId !== actor.companyId) ||
    !actor.authority ||
    !actor.iam
  )
    return Promise.reject(new NativeCrmActivityWorkspaceError('NATIVE_CRM_ACTIVITY_CONTEXT_CHANGED'));
  const headers = new Headers({
    host,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority.revision),
    'x-erpx-context-assignment-version': String(actor.authority.assignmentVersion),
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeCrmActivityWorkspaceError('NATIVE_CRM_ACTIVITY_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  if (actor.scope.kind !== 'holding' && actor.scope.branchSlug)
    headers.set('x-erpx-workspace-branch', actor.scope.branchSlug);
  if (actor.scope.kind === 'department')
    headers.set('x-erpx-workspace-department', actor.scope.departmentSlug);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/crm/activities${path}`,
    settings.base,
  );
  for (const [key, value] of query ?? []) url.searchParams.set(key, value);
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, settings.timeout);
  return transport(url, headers, controller.signal).finally(() => {
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
        return failure('NATIVE_CRM_ACTIVITY_RESPONSE_LIMIT');
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
    return failure('NATIVE_CRM_ACTIVITY_UNAVAILABLE', response.status);
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
    return failure(code ?? 'NATIVE_CRM_ACTIVITY_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_CRM_ACTIVITY_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_CRM_ACTIVITY_RESPONSE_INVALID');
  }
}

/** Fresh bounded socket; bypasses patched fetch/HMR and ambient proxies. */
export function directHttp(
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
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

export function nativeCrmActivityWorkspaceAvailable(): boolean {
  return process.env.ERPX_NATIVE_CRM_ACTIVITY_WORKSPACE_CONFIG !== undefined;
}

export function nativeCrmActivityWorkspaceConfiguration(
  raw: string,
): NativeCrmActivityWorkspaceConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return failure('NATIVE_CRM_ACTIVITY_CONFIGURATION_INVALID');
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
        text(holding.holdingId) &&
        (holding.companyId === undefined || text(holding.companyId)),
    )
  )
    return failure('NATIVE_CRM_ACTIVITY_CONFIGURATION_INVALID');
  return value as unknown as NativeCrmActivityWorkspaceConfiguration;
}

export function createNativeCrmActivityWorkspaceClient(
  raw: NativeCrmActivityWorkspaceConfiguration,
  transport: NativeCrmActivityWorkspaceTransport = (url, headers, signal) =>
    directHttp('GET', url, headers, undefined, signal),
) {
  const settings = configuration(raw);
  const read = async (
    incoming: Pick<Headers, 'get'>,
    actor: WorkContext,
    path: string,
    query: readonly [string, string][] | undefined,
    decode: (data: unknown) => unknown,
    signal?: AbortSignal,
  ): Promise<unknown> => {
    if (!identifier(actor.userId) || !identifier(actor.holdingSlug)) {
      throw new ApiError(409, 'WORK_CONTEXT_CHANGED', 'Your work role changed. Reload and review before retrying.');
    }
    const response = await call(transport, settings, incoming, actor, path, query, signal).catch(
      (error: unknown) => {
        if (error instanceof NativeCrmActivityWorkspaceError) throw error;
        return failure('NATIVE_CRM_ACTIVITY_UNAVAILABLE');
      },
    );
    const value = await boundedJson(response);
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_CRM_ACTIVITY_RESPONSE_INVALID');
    context(value.context, actor);
    return decode(value.data);
  };
  return {
    workspace: async (
      incoming: Pick<Headers, 'get'>,
      actor: WorkContext,
      leadId: string,
      signal?: AbortSignal,
    ): Promise<NativeCrmActivityWorkspaceData> => {
      if (!identifier(leadId)) return failure('NATIVE_CRM_ACTIVITY_REQUEST_INVALID');
      return (await read(
        incoming,
        actor,
        '',
        [['leadId', leadId]],
        (data) => workspace(data, actor, leadId),
        signal,
      )) as NativeCrmActivityWorkspaceData;
    },
    work: async (
      incoming: Pick<Headers, 'get'>,
      actor: WorkContext,
      signal?: AbortSignal,
    ): Promise<NativeCrmWorkItem[]> =>
      (await read(incoming, actor, '/work', undefined, (data) => work(data, actor), signal)) as NativeCrmWorkItem[],
  };
}

function failurePolicy(error: unknown): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeCrmActivityWorkspaceError)) return undefined;
  if (error.code === 'NATIVE_CRM_ACTIVITY_CONTEXT_CHANGED')
    return {
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before retrying.',
    };
  if (error.status === 401)
    return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
  if (error.status === 403)
    return { status: 403, code: error.code, message: error.message };
  if (error.status === 404)
    return { status: 404, code: 'NOT_FOUND', message: 'The CRM activity workspace is not available in this scope.' };
  if (error.status === 409 || error.status === 422)
    return { status: error.status, code: error.code, message: error.message };
  return {
    status: 503,
    code: 'NATIVE_CRM_ACTIVITY_WORKSPACE_UNAVAILABLE',
    message: 'The native CRM activity workspace is unavailable.',
  };
}

/** Request-scoped fail-closed reads: no configuration means no TypeScript path. */
export async function nativeCrmActivityWorkspace(
  actor: WorkContext,
  leadId: string,
): Promise<NativeCrmActivityWorkspaceData> {
  const raw = process.env.ERPX_NATIVE_CRM_ACTIVITY_WORKSPACE_CONFIG;
  if (raw === undefined)
    throw new ApiError(
      503,
      'NATIVE_CRM_ACTIVITY_WORKSPACE_UNAVAILABLE',
      'The native CRM activity workspace is unavailable.',
    );
  try {
    return await createNativeCrmActivityWorkspaceClient(
      nativeCrmActivityWorkspaceConfiguration(raw),
    ).workspace(await headers(), actor, leadId);
  } catch (error) {
    const failure = failurePolicy(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}

export async function nativeCrmActivityWork(
  actor: WorkContext,
): Promise<NativeCrmWorkItem[]> {
  const raw = process.env.ERPX_NATIVE_CRM_ACTIVITY_WORKSPACE_CONFIG;
  if (raw === undefined)
    throw new ApiError(
      503,
      'NATIVE_CRM_ACTIVITY_WORKSPACE_UNAVAILABLE',
      'The native CRM activity workspace is unavailable.',
    );
  try {
    return await createNativeCrmActivityWorkspaceClient(
      nativeCrmActivityWorkspaceConfiguration(raw),
    ).work(await headers(), actor);
  } catch (error) {
    const failure = failurePolicy(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}
