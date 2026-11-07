import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

/**
 * Pure Agent evidence owner transport. It carries the retained browser Host,
 * cookie and current context headers to the private native owner router and
 * validates the exact response before a page renders it. It never mints a
 * session, Holding, Company, role or permission.
 */
export type NativeAgentPageContext = {
  userId: string;
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  branchSlug: string | null;
  departmentSlug: string | null;
  assignmentId: string;
  role: string;
  assignmentScope: 'holding' | 'branch' | 'department';
  authority: { revision: number; assignmentVersion: number };
  modules: readonly string[];
  iam: { permissions: readonly string[] };
};

export type NativeAgentEvidenceConfiguration = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
}>;

export type NativeAgentEvidenceTransport = (
  method: 'GET',
  url: URL,
  headers: Headers,
  body: undefined,
  signal: AbortSignal,
) => Promise<Response>;

export type NativeAgentInvocation = {
  id: string;
  session_id: string;
  turn_id: string;
  company_id: string;
  user_id: string;
  agent_version_id: string | null;
  question_digest: string;
  sources_digest: string;
  outcome: 'succeeded' | 'failed';
  input_tokens: number | null;
  output_tokens: number | null;
  error: string | null;
  created_at: number;
} & Record<string, unknown>;

export type NativeAgentSession = {
  id: string;
  company_id: string;
  user_id: string;
  assignment_id: string;
  conversation_key: string;
  generation: number;
  created_at: number;
  status: 'active' | 'expired';
  expires_at: number;
  legal_hold: boolean;
  hold_reason: string | null;
  version: number;
  last_used_at: number;
  updated_at: number;
  effective_status: 'active' | 'expired';
} & Record<string, unknown>;

export type NativeAgentEvidence = {
  now: number;
  invocations: NativeAgentInvocation[];
  sessions: NativeAgentSession[];
};

type Row = Record<string, unknown>;

export class NativeAgentEvidenceError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Agent evidence request could not be completed.');
    this.name = 'NativeAgentEvidenceError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeAgentEvidenceError(code, status, message);
};
const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' &&
  value.length > 0 &&
  new TextEncoder().encode(value).length <= max &&
  !/[\u0000-\u001f\u007f]/.test(value);
const messageText = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' &&
  new TextEncoder().encode(value).length <= max &&
  !/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(value);
const identifier = (value: unknown): value is string => text(value, 200);
const optionalIdentifier = (value: unknown) => value === null || identifier(value);
const slug = (value: unknown) =>
  typeof value === 'string' && value.length <= 128 && /^[a-z0-9_-]+$/.test(value);
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const optionalInteger = (value: unknown, min = 0) =>
  value === null || integer(value, min);
const boolean = (value: unknown): value is boolean => typeof value === 'boolean';
const texts = (value: unknown, max = 256): value is string[] =>
  Array.isArray(value) && value.length <= max && value.every((item) => text(item, 256));
const digest = (value: unknown): value is string =>
  typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);
const sameSet = (left: readonly string[], right: readonly string[]) => {
  const a = new Set(left);
  const b = new Set(right);
  return a.size === b.size && [...a].every((value) => b.has(value));
};

const INVOCATION_LIMIT = 50;
const SESSION_LIMIT = 100;
const CONTEXT_KEYS = [
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
] as const;
const INVOCATION_KEYS = [
  'id',
  'session_id',
  'turn_id',
  'company_id',
  'user_id',
  'agent_version_id',
  'question_digest',
  'sources_digest',
  'outcome',
  'input_tokens',
  'output_tokens',
  'error',
  'created_at',
] as const;
const SESSION_KEYS = [
  'id',
  'company_id',
  'user_id',
  'assignment_id',
  'conversation_key',
  'generation',
  'created_at',
  'status',
  'expires_at',
  'legal_hold',
  'hold_reason',
  'version',
  'last_used_at',
  'updated_at',
  'effective_status',
] as const;

function invocation(value: unknown, actor: NativeAgentPageContext): value is NativeAgentInvocation {
  return (
    exact(value, INVOCATION_KEYS) &&
    identifier(value.id) &&
    identifier(value.session_id) &&
    identifier(value.turn_id) &&
    value.company_id === actor.companyId &&
    identifier(value.user_id) &&
    optionalIdentifier(value.agent_version_id) &&
    digest(value.question_digest) &&
    digest(value.sources_digest) &&
    (value.outcome === 'succeeded' || value.outcome === 'failed') &&
    optionalInteger(value.input_tokens) &&
    optionalInteger(value.output_tokens) &&
    (value.error === null || messageText(value.error, 2000)) &&
    integer(value.created_at)
  );
}

function session(
  value: unknown,
  actor: NativeAgentPageContext,
  now: number,
  manage: boolean,
): value is NativeAgentSession {
  if (
    !exact(value, SESSION_KEYS) ||
    !identifier(value.id) ||
    value.company_id !== actor.companyId ||
    !identifier(value.user_id) ||
    (!manage && value.user_id !== actor.userId) ||
    !identifier(value.assignment_id) ||
    !text(value.conversation_key, 400) ||
    !integer(value.generation, 1) ||
    !integer(value.version, 1) ||
    !integer(value.created_at) ||
    (value.status !== 'active' && value.status !== 'expired') ||
    !integer(value.expires_at) ||
    !boolean(value.legal_hold) ||
    !(value.hold_reason === null || messageText(value.hold_reason, 2000)) ||
    value.legal_hold !== (value.hold_reason !== null) ||
    !integer(value.last_used_at) ||
    !integer(value.updated_at)
  )
    return false;
  const effective =
    value.status === 'active' && value.expires_at <= now ? 'expired' : value.status;
  return value.effective_status === effective;
}

function workspace(value: unknown, actor: NativeAgentPageContext): NativeAgentEvidence {
  if (!exact(value, ['now', 'invocations', 'sessions'])) return failure('NATIVE_AGENT_RESPONSE_INVALID');
  const now = value.now;
  if (
    !integer(now, 1) ||
    !Array.isArray(value.invocations) ||
    value.invocations.length > INVOCATION_LIMIT ||
    !Array.isArray(value.sessions) ||
    value.sessions.length > SESSION_LIMIT
  )
    return failure('NATIVE_AGENT_RESPONSE_INVALID');
  const manage = actor.iam.permissions.includes('ai.manage');
  const invocationIds = new Set<string>();
  for (const row of value.invocations as unknown[]) {
    if (!invocation(row, actor) || invocationIds.has(row.id))
      return failure('NATIVE_AGENT_RESPONSE_INVALID');
    invocationIds.add(row.id);
  }
  const sessionIds = new Set<string>();
  for (const row of value.sessions as unknown[]) {
    if (!session(row, actor, now, manage) || sessionIds.has(row.id))
      return failure('NATIVE_AGENT_RESPONSE_INVALID');
    sessionIds.add(row.id);
  }
  for (let index = 1; index < value.invocations.length; index += 1) {
    const previous = value.invocations[index - 1] as NativeAgentInvocation;
    const current = value.invocations[index] as NativeAgentInvocation;
    if (
      previous.created_at < current.created_at ||
      (previous.created_at === current.created_at && previous.id > current.id)
    )
      return failure('NATIVE_AGENT_RESPONSE_INVALID');
  }
  for (let index = 1; index < value.sessions.length; index += 1) {
    const previous = value.sessions[index - 1] as NativeAgentSession;
    const current = value.sessions[index] as NativeAgentSession;
    if (
      previous.updated_at < current.updated_at ||
      (previous.updated_at === current.updated_at && previous.id > current.id)
    )
      return failure('NATIVE_AGENT_RESPONSE_INVALID');
  }
  return value as unknown as NativeAgentEvidence;
}

function context(value: unknown, actor: NativeAgentPageContext) {
  if (!exact(value, CONTEXT_KEYS)) return failure('NATIVE_AGENT_RESPONSE_INVALID');
  const parsed = value as Row;
  if (
    parsed.userId !== actor.userId ||
    parsed.holdingId !== actor.holdingId ||
    parsed.companyId !== actor.companyId ||
    parsed.branchId !== actor.branchId ||
    parsed.departmentId !== actor.departmentId ||
    parsed.branchSlug !== actor.branchSlug ||
    parsed.departmentSlug !== actor.departmentSlug ||
    parsed.assignmentId !== actor.assignmentId ||
    parsed.assignmentVersion !== actor.authority.assignmentVersion ||
    parsed.revision !== actor.authority.revision ||
    parsed.role !== actor.role ||
    parsed.assignmentScopeKind !== actor.assignmentScope ||
    !texts(parsed.modules) ||
    !sameSet(actor.modules, parsed.modules) ||
    !texts(parsed.permissions) ||
    !sameSet(actor.iam.permissions, parsed.permissions)
  )
    return failure('NATIVE_AGENT_CONTEXT_CHANGED');
  return parsed;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

type ParsedConfiguration = {
  base: URL;
  holdings: Map<string, { holdingId: string; holdingSlug: string }>;
  timeout: number;
};

function configuration(raw: NativeAgentEvidenceConfiguration): ParsedConfiguration {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_AGENT_CONFIGURATION_INVALID');
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
    return failure('NATIVE_AGENT_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_AGENT_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      origin.port ||
      origin.username ||
      origin.password ||
      !identifier(holding.holdingId) ||
      !slug(holding.holdingSlug) ||
      holdings.has(origin.host)
    )
      return failure('NATIVE_AGENT_CONFIGURATION_INVALID');
    holdings.set(origin.host, {
      holdingId: holding.holdingId,
      holdingSlug: holding.holdingSlug,
    });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativeAgentEvidenceTransport,
  config: ParsedConfiguration,
  incoming: Pick<Headers, 'get'>,
  actor: NativeAgentPageContext,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? config.holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== actor.holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(new NativeAgentEvidenceError('NATIVE_AGENT_AUTHORITY_INVALID'));
  const headers = new Headers({ host: host!, accept: 'application/json', 'cache-control': 'no-cache' });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeAgentEvidenceError('NATIVE_AGENT_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  for (const [key, value] of [
    ['x-erpx-workspace-branch', actor.branchSlug],
    ['x-erpx-workspace-department', actor.departmentSlug],
  ] as const) {
    if (value !== null) {
      if (!slug(value))
        return Promise.reject(new NativeAgentEvidenceError('NATIVE_AGENT_REQUEST_INVALID'));
      headers.set(key, value);
    }
  }
  headers.set('x-erpx-context-company', actor.companyId);
  headers.set('x-erpx-context-assignment', actor.assignmentId);
  headers.set('x-erpx-context-revision', String(actor.authority.revision));
  headers.set('x-erpx-context-assignment-version', String(actor.authority.assignmentVersion));
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/agents/evidence`,
    config.base,
  );
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
        return failure('NATIVE_AGENT_RESPONSE_LIMIT');
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
    return failure('NATIVE_AGENT_UNAVAILABLE', response.status);
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
          if (messageText(value.error.message, 2000)) message = value.error.message;
        }
      } catch {
        // A malformed owner error keeps the bounded transport failure.
      }
    }
    return failure(code ?? 'NATIVE_AGENT_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_AGENT_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_AGENT_RESPONSE_INVALID');
  }
}

const directHttp: NativeAgentEvidenceTransport = (method, url, headers, _body, signal) =>
  fetch(url, { method, headers, cache: 'no-store', redirect: 'error', signal });

export function createNativeAgentEvidenceClient(
  raw: NativeAgentEvidenceConfiguration,
  transport: NativeAgentEvidenceTransport = directHttp,
) {
  const config = configuration(raw);
  return {
    evidence: async (
      incoming: Pick<Headers, 'get'>,
      actor: NativeAgentPageContext,
      signal?: AbortSignal,
    ): Promise<NativeAgentEvidence> => {
      try {
        const response = await transportCall(transport, config, incoming, actor, signal);
        const value = await boundedJson(response);
        if (!exact(value, ['data', 'context'])) return failure('NATIVE_AGENT_RESPONSE_INVALID');
        context(value.context, actor);
        return workspace(value.data, actor);
      } catch (error) {
        if (error instanceof NativeAgentEvidenceError) throw error;
        return failure('NATIVE_AGENT_UNAVAILABLE');
      }
    },
  };
}

export function nativeAgentEvidenceConfiguration(raw: string): NativeAgentEvidenceConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return failure('NATIVE_AGENT_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    !exact(value, ['privateBaseUrl', 'timeoutMs', 'holdings']) ||
    !text(value.privateBaseUrl, 200) ||
    !integer(value.timeoutMs, 1) ||
    !Array.isArray(value.holdings) ||
    value.holdings.length > 64 ||
    !value.holdings.every(
      (holding) =>
        exact(holding, ['origin', 'holdingSlug', 'holdingId']) &&
        text(holding.origin, 200) &&
        identifier(holding.holdingId) &&
        text(holding.holdingSlug, 128),
    )
  )
    return failure('NATIVE_AGENT_CONFIGURATION_INVALID');
  const parsed = value as unknown as NativeAgentEvidenceConfiguration;
  configuration(parsed);
  return parsed;
}

export function nativeAgentEvidencePageContext(actor: WorkContext): NativeAgentPageContext {
  if (!actor.authority || !actor.iam) throw new NativeAgentEvidenceError('NATIVE_AGENT_CONTEXT_CHANGED');
  if (actor.assignmentScope !== 'holding' && actor.assignmentScope !== 'branch' && actor.assignmentScope !== 'department')
    throw new NativeAgentEvidenceError('NATIVE_AGENT_CONTEXT_CHANGED');
  const branchSlug =
    actor.scope.kind === 'holding'
      ? null
      : actor.scope.kind === 'branch'
        ? actor.scope.branchSlug
        : (actor.scope.branchSlug ?? null);
  const departmentSlug = actor.scope.kind === 'department' ? actor.scope.departmentSlug : null;
  return {
    userId: actor.userId,
    holdingId: actor.holdingId,
    holdingSlug: actor.holdingSlug,
    companyId: actor.companyId,
    branchId: actor.branchId,
    departmentId: actor.departmentId,
    branchSlug,
    departmentSlug,
    assignmentId: actor.assignmentId,
    role: actor.role,
    assignmentScope: actor.assignmentScope,
    authority: {
      revision: actor.authority.revision,
      assignmentVersion: actor.authority.assignmentVersion,
    },
    modules: [...actor.modules],
    iam: { permissions: [...actor.iam.permissions] },
  };
}

export function nativeAgentEvidenceFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeAgentEvidenceError)) return undefined;
  if (error.code === 'NATIVE_AGENT_CONTEXT_CHANGED' || error.code === 'NATIVE_AGENT_AUTHORITY_INVALID')
    return {
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before continuing.',
    };
  if (
    error.code === 'NATIVE_AGENT_CONFIGURATION_INVALID' ||
    error.code === 'NATIVE_AGENT_UNAVAILABLE' ||
    error.code === 'NATIVE_AGENT_RESPONSE_INVALID' ||
    error.code === 'NATIVE_AGENT_RESPONSE_LIMIT' ||
    error.code === 'NATIVE_AGENT_COOKIE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_AGENT_UNAVAILABLE',
      message: 'The native Agent evidence is unavailable.',
    };
  if (error.code === 'NATIVE_AGENT_REQUEST_INVALID')
    return {
      status: 422,
      code: 'AGENT_EVIDENCE_REQUEST_INVALID',
      message: 'Use one bounded Agent evidence request.',
    };
  if (error.status === 401)
    return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
  if (error.status === 403)
    return {
      status: 403,
      code: error.code === 'NATIVE_AGENT_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return { status: 404, code: error.code, message: 'Agent record not found in this scope.' };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_AGENT_UNAVAILABLE',
      message: 'The native Agent evidence is unavailable.',
    };
  return undefined;
}
