import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';

/** Rendering input from retained page authorization, never a grant. */
export type NativeProjectsPageContext = {
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

export type NativeProjectsConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;
type Context = Row & { modules: string[]; permissions: string[] };

export type NativeProjectWbsReadInput = Readonly<{
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

export type NativeProjectRow = Readonly<{
  id: string;
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  title: string;
  description: string;
  owner_user_id: string;
  lifecycle_status: 'active' | 'handoff_pending' | 'completed' | 'cancelled';
  version: number;
  updated_at: number;
}>;

export type NativeProjectMilestoneRow = Readonly<{
  id: string;
  company_id: string;
  project_id: string;
  parent_milestone_id: string | null;
  ordinal_position: number;
  title: string;
  description: string;
  due_at: number;
  owner_user_id: string;
  owner_name: string | null;
  state: 'planned' | 'in_progress' | 'completed' | 'cancelled';
  is_overdue: boolean;
  version: number;
  created_by: string;
  completed_at: number | null;
  created_at: number;
  updated_at: number;
  depends_on_ids: string[];
  depends_on_titles: string[];
  blocked_by_titles: string[];
  blocked_by_count: number;
}>;

export type NativeProjectWbs = Readonly<{
  project: NativeProjectRow;
  milestones: NativeProjectMilestoneRow[];
}>;

export type NativeProjectsTransport = (
  method: 'GET',
  url: URL,
  headers: Headers,
  body: undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeProjectsError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Projects request could not be completed.');
    this.name = 'NativeProjectsError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeProjectsError(code, status, message);
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
const optionalInteger = (value: unknown) =>
  value === null || integer(value);
const texts = (value: unknown, max = 256): value is string[] =>
  Array.isArray(value) && value.length <= max && value.every((item) => text(item, 256));
const sameSet = (left: readonly string[], right: readonly string[]) => {
  const a = new Set(left);
  const b = new Set(right);
  return a.size === b.size && [...a].every((value) => b.has(value));
};
const title = (value: unknown): value is string =>
  text(value) && value.trim() === value && [...value].length >= 2 && [...value].length <= 200;
const description = (value: unknown): value is string =>
  typeof value === 'string' && [...value].length <= 10_000;
const lifecycle = new Set(['active', 'handoff_pending', 'completed', 'cancelled']);
const states = new Set(['planned', 'in_progress', 'completed', 'cancelled']);

const projectRow = (value: unknown): value is NativeProjectRow =>
  exact(value, [
    'id', 'company_id', 'branch_id', 'department_id', 'title', 'description',
    'owner_user_id', 'lifecycle_status', 'version', 'updated_at',
  ]) &&
  identifier(value.id) &&
  identifier(value.company_id) &&
  optionalText(value.branch_id, 200) &&
  optionalText(value.department_id, 200) &&
  title(value.title) &&
  description(value.description) &&
  identifier(value.owner_user_id) &&
  typeof value.lifecycle_status === 'string' &&
  lifecycle.has(value.lifecycle_status) &&
  integer(value.version, 1) &&
  integer(value.updated_at);

const milestoneRow = (value: unknown): value is NativeProjectMilestoneRow => {
  if (
    !exact(value, [
      'id', 'company_id', 'project_id', 'parent_milestone_id', 'ordinal_position',
      'title', 'description', 'due_at', 'owner_user_id', 'owner_name', 'state',
      'is_overdue', 'version', 'created_by', 'completed_at', 'created_at',
      'updated_at', 'depends_on_ids', 'depends_on_titles', 'blocked_by_titles',
      'blocked_by_count',
    ])
  )
    return false;
  if (
    !identifier(value.id) ||
    !identifier(value.company_id) ||
    !identifier(value.project_id) ||
    !(value.parent_milestone_id === null || identifier(value.parent_milestone_id)) ||
    !integer(value.ordinal_position) ||
    value.ordinal_position > 1_000_000 ||
    !title(value.title) ||
    !description(value.description) ||
    !integer(value.due_at) ||
    !identifier(value.owner_user_id) ||
    !optionalText(value.owner_name, 3_000) ||
    typeof value.state !== 'string' ||
    !states.has(value.state) ||
    typeof value.is_overdue !== 'boolean' ||
    !integer(value.version, 1) ||
    !identifier(value.created_by) ||
    !optionalInteger(value.completed_at) ||
    !integer(value.created_at) ||
    !integer(value.updated_at) ||
    !Array.isArray(value.depends_on_ids) ||
    value.depends_on_ids.length > 50 ||
    !value.depends_on_ids.every(identifier) ||
    !texts(value.depends_on_titles, 50) ||
    value.depends_on_titles.length !== value.depends_on_ids.length ||
    !texts(value.blocked_by_titles, 50) ||
    !integer(value.blocked_by_count) ||
    value.blocked_by_count !== value.blocked_by_titles.length ||
    (value.state === 'completed') !== (value.completed_at !== null)
  )
    return false;
  if (value.parent_milestone_id === value.id) return false;
  if (new Set(value.depends_on_ids).size !== value.depends_on_ids.length) return false;
  if (value.depends_on_ids.includes(value.id)) return false;
  const titles = new Set(value.depends_on_titles);
  return value.blocked_by_titles.every((item) => titles.has(item));
};

function wbsData(value: unknown, projectId: string, companyId: string): NativeProjectWbs {
  if (!exact(value, ['project', 'milestones']) || !projectRow(value.project) || !Array.isArray(value.milestones))
    return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  if (value.project.id !== projectId || value.project.company_id !== companyId)
    return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  if (value.milestones.length > 1000 || !value.milestones.every(milestoneRow))
    return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  const seen = new Set<string>();
  for (const milestone of value.milestones) {
    if (seen.has(milestone.id)) return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
    if (milestone.project_id !== projectId || milestone.company_id !== companyId)
      return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
    seen.add(milestone.id);
  }
  for (const milestone of value.milestones) {
    if (milestone.parent_milestone_id !== null && !seen.has(milestone.parent_milestone_id))
      return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
    if (milestone.depends_on_ids.some((id) => !seen.has(id)))
      return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  }
  return { project: value.project, milestones: value.milestones as NativeProjectMilestoneRow[] };
}

function context(value: unknown, actor: NativeProjectsPageContext, input: NativeProjectWbsReadInput): Context {
  if (
    !exact(value, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug',
      'departmentSlug', 'assignmentId', 'assignmentVersion', 'revision', 'role',
      'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ])
  )
    return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
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
    return failure('NATIVE_PROJECTS_CONTEXT_CHANGED');
  if (
    input.expected &&
    (parsed.companyId !== input.expected.companyId ||
      parsed.assignmentId !== input.expected.assignmentId ||
      parsed.revision !== input.expected.revision ||
      parsed.assignmentVersion !== input.expected.assignmentVersion)
  )
    return failure('NATIVE_PROJECTS_CONTEXT_CHANGED');
  return parsed;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativeProjectsConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_PROJECTS_CONFIGURATION_INVALID');
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
    return failure('NATIVE_PROJECTS_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_PROJECTS_CONFIGURATION_INVALID');
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
      return failure('NATIVE_PROJECTS_CONFIGURATION_INVALID');
    holdings.set(origin.host, { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativeProjectsTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  projectId: string,
  holdingSlug: string,
  input: NativeProjectWbsReadInput,
  timeout: number,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== holdingSlug)
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_AUTHORITY_INVALID'));
  const headers = new Headers({ host: host!, accept: 'application/json', 'cache-control': 'no-cache' });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  for (const [key, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_REQUEST_INVALID'));
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
      return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_REQUEST_INVALID'));
    headers.set('x-erpx-context-company', input.expected.companyId);
    headers.set('x-erpx-context-assignment', input.expected.assignmentId);
    headers.set('x-erpx-context-revision', String(input.expected.revision));
    headers.set('x-erpx-context-assignment-version', String(input.expected.assignmentVersion));
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/projects/${encodeURIComponent(projectId)}/wbs`,
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
        return failure('NATIVE_PROJECTS_RESPONSE_LIMIT');
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
    return failure('NATIVE_PROJECTS_UNAVAILABLE', response.status);
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
    return failure(code ?? 'NATIVE_PROJECTS_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  }
}

export function nativeProjectsAvailable(): boolean {
  return process.env.ERPX_NATIVE_PROJECTS_CONFIG !== undefined;
}

export function nativeProjectsConfiguration(raw: string): NativeProjectsConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeProjectsError('NATIVE_PROJECTS_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    !Array.isArray(value.holdings)
  )
    return failure('NATIVE_PROJECTS_CONFIGURATION_INVALID');
  if (
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) &&
        text(holding.origin) &&
        text(holding.holdingSlug) &&
        text(holding.holdingId),
    )
  )
    return failure('NATIVE_PROJECTS_CONFIGURATION_INVALID');
  return value as unknown as NativeProjectsConfiguration;
}

export function createNativeProjectsClient(
  raw: NativeProjectsConfiguration,
  transport: NativeProjectsTransport = directHttp,
) {
  const config = configuration(raw);
  const request = (
    incoming: Pick<Headers, 'get'>,
    actor: NativeProjectsPageContext,
    projectId: string,
    input: NativeProjectWbsReadInput,
    signal?: AbortSignal,
  ) => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_CONTEXT_CHANGED'));
    if (!identifier(projectId) || projectId.includes('/') || projectId.includes('\\'))
      return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_REQUEST_INVALID'));
    return transportCall(
      transport,
      config.base,
      config.holdings,
      incoming,
      projectId,
      input.holdingSlug,
      input,
      config.timeout,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeProjectsError) throw error;
      return failure('NATIVE_PROJECTS_UNAVAILABLE');
    });
  };
  const wbs = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeProjectsPageContext,
    projectId: string,
    input: NativeProjectWbsReadInput,
    signal?: AbortSignal,
  ): Promise<NativeProjectWbs> => {
    const value = await boundedJson(await request(incoming, actor, projectId, input, signal));
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
    context(value.context, actor, input);
    return wbsData(value.data, projectId, actor.companyId);
  };
  return { wbs };
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
