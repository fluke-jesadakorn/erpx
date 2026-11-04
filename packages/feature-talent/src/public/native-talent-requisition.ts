import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';

/** Rendering input from retained page authorization, never a grant. */
export type NativeTalentPageContext = {
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

export type NativeTalentConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;
type Context = Row & { modules: string[]; permissions: string[] };

export type NativeTalentReadInput = Readonly<{
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

export type NativeTalentRequisitionRow = Readonly<{
  id: string;
  company_id: string;
  branch_id: string;
  department_id: string | null;
  job_title: string;
  openings: number;
  target_start_date: string;
  description: string;
  status: string;
  created_by: string;
  reviewed_by: string | null;
  version: number;
  created_at: number;
  updated_at: number;
  canApprove: boolean;
  canReject: boolean;
  canOpen: boolean;
  canClose: boolean;
  canCancel: boolean;
}>;

export type NativeTalentEvent = Readonly<{
  version: number;
  action: string;
  actor_id: string;
  reason: string;
  status: string;
  created_at: number;
}>;

export type NativeTalentWorkspace = Readonly<{
  requisitions: NativeTalentRequisitionRow[];
}>;

export type NativeTalentDetail = Readonly<{
  requisition: NativeTalentRequisitionRow;
  events: NativeTalentEvent[];
}>;

export type NativeTalentReceipt = Readonly<{
  id: string;
  status: string;
  version: number;
}>;

export type NativeTalentTransport = (
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeTalentError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Talent request could not be completed.');
    this.name = 'NativeTalentError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeTalentError(code, status, message);
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
const optionalIdentifier = (value: unknown) =>
  value === null || (identifier(value) && !/[\u0000-\u001f\u007f]/.test(value));
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const boolean = (value: unknown): value is boolean => typeof value === 'boolean';
const trimmed = (value: unknown, minimum: number, maximum: number): value is string =>
  text(value, maximum) &&
  [...value].length >= minimum &&
  value.trim() === value &&
  !/[\u0000-\u001f\u007f]/.test(value);
const date = (value: unknown): value is string => {
  if (typeof value !== 'string') return false;
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return false;
  const year = Number(match[1]),
    month = Number(match[2]),
    day = Number(match[3]);
  const leap = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0);
  const days = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
  return month >= 1 && month <= 12 && day >= 1 && day <= days[month - 1]!;
};

const statuses = new Set(['draft', 'approved', 'rejected', 'open', 'closed', 'cancelled']);

const requisition = (value: unknown): value is NativeTalentRequisitionRow =>
  exact(value, [
    'id',
    'company_id',
    'branch_id',
    'department_id',
    'job_title',
    'openings',
    'target_start_date',
    'description',
    'status',
    'created_by',
    'reviewed_by',
    'version',
    'created_at',
    'updated_at',
    'canApprove',
    'canReject',
    'canOpen',
    'canClose',
    'canCancel',
  ]) &&
  identifier(value.id) &&
  identifier(value.company_id) &&
  identifier(value.branch_id) &&
  optionalIdentifier(value.department_id) &&
  trimmed(value.job_title, 2, 160) &&
  integer(value.openings, 1) &&
  value.openings <= 1_000 &&
  date(value.target_start_date) &&
  trimmed(value.description, 3, 3_000) &&
  typeof value.status === 'string' &&
  statuses.has(value.status) &&
  identifier(value.created_by) &&
  optionalIdentifier(value.reviewed_by) &&
  integer(value.version, 1) &&
  integer(value.created_at) &&
  integer(value.updated_at) &&
  boolean(value.canApprove) &&
  boolean(value.canReject) &&
  boolean(value.canOpen) &&
  boolean(value.canClose) &&
  boolean(value.canCancel);

const event = (value: unknown): value is NativeTalentEvent =>
  exact(value, ['version', 'action', 'actor_id', 'reason', 'status', 'created_at']) &&
  integer(value.version, 1) &&
  value.version <= 1_000_000 &&
  trimmed(value.action, 1, 200) &&
  identifier(value.actor_id) &&
  trimmed(value.reason, 3, 3_000) &&
  typeof value.status === 'string' &&
  statuses.has(value.status) &&
  integer(value.created_at);

function scoped(
  value: NativeTalentRequisitionRow,
  actor: NativeTalentPageContext,
): boolean {
  return (
    value.company_id === actor.companyId &&
    (actor.branchId === null || value.branch_id === actor.branchId) &&
    (actor.departmentId === null || value.department_id === actor.departmentId)
  );
}

function workspace(value: unknown, actor: NativeTalentPageContext): NativeTalentWorkspace {
  if (!exact(value, ['requisitions'])) return failure('NATIVE_TALENT_RESPONSE_INVALID');
  if (
    !Array.isArray(value.requisitions) ||
    value.requisitions.length > 200 ||
    !value.requisitions.every(requisition)
  )
    return failure('NATIVE_TALENT_RESPONSE_INVALID');
  const ids = new Set<string>();
  for (const item of value.requisitions) {
    if (!scoped(item, actor) || ids.has(item.id))
      return failure('NATIVE_TALENT_RESPONSE_INVALID');
    ids.add(item.id);
  }
  return value as unknown as NativeTalentWorkspace;
}

function detail(
  value: unknown,
  actor: NativeTalentPageContext,
  id: string,
): NativeTalentDetail {
  if (!exact(value, ['requisition', 'events'])) return failure('NATIVE_TALENT_RESPONSE_INVALID');
  if (
    !requisition(value.requisition) ||
    value.requisition.id !== id ||
    !scoped(value.requisition, actor) ||
    !Array.isArray(value.events) ||
    value.events.length > 500 ||
    !value.events.every(event)
  )
    return failure('NATIVE_TALENT_RESPONSE_INVALID');
  let previous = 0;
  for (const item of value.events) {
    if (item.version <= previous) return failure('NATIVE_TALENT_RESPONSE_INVALID');
    previous = item.version;
  }
  return value as unknown as NativeTalentDetail;
}

function receipt(value: unknown): NativeTalentReceipt {
  if (
    !exact(value, ['id', 'status', 'version']) ||
    !identifier(value.id) ||
    typeof value.status !== 'string' ||
    !statuses.has(value.status) ||
    !integer(value.version, 1)
  )
    return failure('NATIVE_TALENT_RESPONSE_INVALID');
  return value as unknown as NativeTalentReceipt;
}

function context(
  value: unknown,
  actor: NativeTalentPageContext,
  input: NativeTalentReadInput,
): Context {
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
    return failure('NATIVE_TALENT_RESPONSE_INVALID');
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
    return failure('NATIVE_TALENT_CONTEXT_CHANGED');
  if (
    input.expected &&
    (parsed.companyId !== input.expected.companyId ||
      parsed.assignmentId !== input.expected.assignmentId ||
      parsed.revision !== input.expected.revision ||
      parsed.assignmentVersion !== input.expected.assignmentVersion)
  )
    return failure('NATIVE_TALENT_CONTEXT_CHANGED');
  return parsed;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativeTalentConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_TALENT_CONFIGURATION_INVALID');
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
    !Array.isArray(raw.holdings) ||
    raw.holdings.length < 1 ||
    raw.holdings.length > 64
  )
    return failure('NATIVE_TALENT_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_TALENT_CONFIGURATION_INVALID');
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
      return failure('NATIVE_TALENT_CONFIGURATION_INVALID');
    holdings.set(origin.host, {
      holdingId: holding.holdingId,
      holdingSlug: holding.holdingSlug,
    });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativeTalentTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  actor: NativeTalentPageContext,
  input: NativeTalentReadInput,
  method: 'GET' | 'POST',
  path: string,
  body: string | undefined,
  key: string | undefined,
  timeout: number,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (
    !holding ||
    holding.holdingSlug !== input.holdingSlug ||
    holding.holdingId !== actor.holdingId
  )
    return Promise.reject(new NativeTalentError('NATIVE_TALENT_AUTHORITY_INVALID'));
  const headers = new Headers({ host: host!, accept: 'application/json', 'cache-control': 'no-cache' });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeTalentError('NATIVE_TALENT_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  for (const [header, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return Promise.reject(new NativeTalentError('NATIVE_TALENT_REQUEST_INVALID'));
      headers.set(header, slug);
    }
  }
  if (input.expected) {
    if (
      !identifier(input.expected.companyId) ||
      !identifier(input.expected.assignmentId) ||
      !integer(input.expected.revision) ||
      !integer(input.expected.assignmentVersion, 1)
    )
      return Promise.reject(new NativeTalentError('NATIVE_TALENT_REQUEST_INVALID'));
    headers.set('x-erpx-context-company', input.expected.companyId);
    headers.set('x-erpx-context-assignment', input.expected.assignmentId);
    headers.set('x-erpx-context-revision', String(input.expected.revision));
    headers.set('x-erpx-context-assignment-version', String(input.expected.assignmentVersion));
  }
  if (method === 'POST') {
    if (!key || !text(key, 180) || key.length < 8 || !/^[A-Za-z0-9:_.-]+$/.test(key))
      return Promise.reject(new NativeTalentError('NATIVE_TALENT_REQUEST_INVALID'));
    if (body === undefined)
      return Promise.reject(new NativeTalentError('NATIVE_TALENT_REQUEST_INVALID'));
    headers.set('content-type', 'application/json');
    headers.set('idempotency-key', key);
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/talent/requisitions${path}`,
    base,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, timeout);
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
        return failure('NATIVE_TALENT_RESPONSE_LIMIT');
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
    return failure('NATIVE_TALENT_UNAVAILABLE', response.status);
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
    return failure(code ?? 'NATIVE_TALENT_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_TALENT_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_TALENT_RESPONSE_INVALID');
  }
}

export function nativeTalentAvailable(): boolean {
  return process.env.ERPX_NATIVE_TALENT_CONFIG !== undefined;
}

export function nativeTalentConfiguration(raw: string): NativeTalentConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeTalentError('NATIVE_TALENT_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    !Array.isArray(value.holdings)
  )
    return failure('NATIVE_TALENT_CONFIGURATION_INVALID');
  if (
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) &&
        text(holding.origin) &&
        text(holding.holdingSlug) &&
        text(holding.holdingId),
    )
  )
    return failure('NATIVE_TALENT_CONFIGURATION_INVALID');
  return value as unknown as NativeTalentConfiguration;
}

export function createNativeTalentClient(
  raw: NativeTalentConfiguration,
  transport: NativeTalentTransport = directHttp,
) {
  const config = configuration(raw);
  const run = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeTalentPageContext,
    input: NativeTalentReadInput,
    method: 'GET' | 'POST',
    path: string,
    body: string | undefined,
    key: string | undefined,
    signal: AbortSignal | undefined,
    validate: (value: unknown) => unknown,
  ) => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return failure('NATIVE_TALENT_CONTEXT_CHANGED');
    const response = await transportCall(
      transport,
      config.base,
      config.holdings,
      incoming,
      actor,
      input,
      method,
      path,
      body,
      key,
      config.timeout,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeTalentError) throw error;
      return failure('NATIVE_TALENT_UNAVAILABLE');
    });
    const value = await boundedJson(response);
    if (!exact(value, ['data', 'context'])) return failure('NATIVE_TALENT_RESPONSE_INVALID');
    context(value.context, actor, input);
    return validate(value.data);
  };
  return {
    workspace: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeTalentPageContext,
      input: NativeTalentReadInput,
      signal?: AbortSignal,
    ) =>
      run(incoming, actor, input, 'GET', '', undefined, undefined, signal, (value) =>
        workspace(value, actor),
      ) as Promise<NativeTalentWorkspace>,
    detail: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeTalentPageContext,
      id: string,
      input: NativeTalentReadInput,
      signal?: AbortSignal,
    ) => {
      if (!identifier(id) || id.includes('/') || id.includes('\\'))
        return Promise.reject(new NativeTalentError('NATIVE_TALENT_REQUEST_INVALID'));
      return run(
        incoming,
        actor,
        input,
        'GET',
        `/${encodeURIComponent(id)}`,
        undefined,
        undefined,
        signal,
        (value) => detail(value, actor, id),
      ) as Promise<NativeTalentDetail>;
    },
    create: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeTalentPageContext,
      body: string,
      key: string,
      input: NativeTalentReadInput,
      signal?: AbortSignal,
    ) =>
      run(incoming, actor, input, 'POST', '', body, key, signal, receipt) as Promise<NativeTalentReceipt>,
    change: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeTalentPageContext,
      id: string,
      body: string,
      key: string,
      input: NativeTalentReadInput,
      signal?: AbortSignal,
    ) => {
      if (!identifier(id) || id.includes('/') || id.includes('\\'))
        return Promise.reject(new NativeTalentError('NATIVE_TALENT_REQUEST_INVALID'));
      return run(
        incoming,
        actor,
        input,
        'POST',
        `/${encodeURIComponent(id)}`,
        body,
        key,
        signal,
        receipt,
      ) as Promise<NativeTalentReceipt>;
    },
  };
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
      {
        method,
        agent,
        headers: { ...Object.fromEntries(headers) },
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
    outgoing.end(body);
  });
}
