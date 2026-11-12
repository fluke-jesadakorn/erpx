import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';
import { ApiError } from '@erpx/server-runtime/public/http';
import {
  NativeProjectsError,
  nativeProjectsConfiguration,
  type NativeProjectsConfiguration,
  type NativeProjectsPageContext,
} from './native-project-wbs';
import { nativeProjectsFailure } from './native-project-wbs-page';

/**
 * Strict private-loopback client for the native Projects work-structure
 * command. The retained route forwards the caller's cookie, selected Work
 * context, Origin and Idempotency-Key, then accepts only the exact native
 * `{data, context}` receipt bound to that key. There is no TypeScript writer
 * fallback: an absent or invalid Projects configuration is unavailable.
 *
 * The native owner serializes `ProjectStructureReceipt` with snake_case
 * optional fields. This client validates that exact wire shape and returns the
 * retained camel-case acknowledgement consumed by the Projects console.
 */

export type NativeProjectStructureCommandInput = Readonly<{
  projectId: string;
  key: string;
  /** Camel-case owner command body; the native owner validates the schema. */
  body: unknown;
}>;

export type NativeProjectStructureAcknowledgement = Readonly<{
  id: string;
  version: number;
  status: string;
  taskId?: string;
  bindingVersion?: number;
  actualId?: string;
  handoffId?: string;
  snapshotSha256?: string;
}>;

export type NativeProjectStructureTransport = (
  method: 'POST',
  url: URL,
  headers: Headers,
  body: Uint8Array,
  signal: AbortSignal,
) => Promise<Response>;

type Row = Record<string, unknown>;
type Holding = Readonly<{ holdingId: string; holdingSlug: string; origin: string }>;
type Context = Row & { modules: string[]; permissions: string[] };
type Action =
  | 'assign-task'
  | 'unassign-task'
  | 'link-actual'
  | 'prepare-handoff'
  | 'accept-handoff'
  | 'reject-handoff'
  | 'cancel-handoff'
  | 'reopen';

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const bodyLimit = 64 * 1024;
const actions = new Set<Action>([
  'assign-task',
  'unassign-task',
  'link-actual',
  'prepare-handoff',
  'accept-handoff',
  'reject-handoff',
  'cancel-handoff',
  'reopen',
]);
const receiptKeys: Readonly<Record<Action, readonly string[]>> = {
  'assign-task': ['id', 'version', 'status', 'task_id', 'binding_version'],
  'unassign-task': ['id', 'version', 'status', 'task_id', 'binding_version'],
  'link-actual': ['id', 'version', 'status', 'actual_id'],
  'prepare-handoff': ['id', 'version', 'status', 'handoff_id', 'snapshot_sha256'],
  'accept-handoff': ['id', 'version', 'status', 'handoff_id', 'snapshot_sha256'],
  'reject-handoff': ['id', 'version', 'status', 'handoff_id', 'snapshot_sha256'],
  'cancel-handoff': ['id', 'version', 'status', 'handoff_id', 'snapshot_sha256'],
  reopen: ['id', 'version', 'status', 'handoff_id', 'snapshot_sha256'],
};
const receiptStatus: Readonly<Record<Action, string>> = {
  'assign-task': 'active',
  'unassign-task': 'active',
  'link-actual': 'active',
  'prepare-handoff': 'handoff_pending',
  'accept-handoff': 'completed',
  'reject-handoff': 'active',
  'cancel-handoff': 'active',
  reopen: 'active',
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
const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeProjectsError(code, status, message);
};
const digest = (value: unknown): value is string =>
  typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);

function settings(raw: NativeProjectsConfiguration) {
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
    raw.timeoutMs > 30000
  )
    return failure('NATIVE_PROJECTS_CONFIGURATION_INVALID');
  const holdings = new Map<string, Holding>();
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
      origin.username ||
      origin.password ||
      !identifier(holding.holdingId) ||
      !text(holding.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug) ||
      holdings.has(origin.host)
    )
      return failure('NATIVE_PROJECTS_CONFIGURATION_INVALID');
    holdings.set(origin.host, {
      holdingId: holding.holdingId,
      holdingSlug: holding.holdingSlug,
      origin: holding.origin,
    });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function verifyContext(value: unknown, actor: NativeProjectsPageContext): Context {
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
    parsed.assignmentId !== actor.assignmentId ||
    parsed.assignmentVersion !== actor.authority?.assignmentVersion ||
    parsed.revision !== actor.authority?.revision ||
    parsed.role !== actor.role ||
    parsed.assignmentScopeKind !== actor.assignmentScope ||
    !integer(parsed.expiresAt, 1) ||
    parsed.expiresAt <= Date.now() ||
    !texts(parsed.modules) ||
    !sameSet(actor.modules, parsed.modules) ||
    !texts(parsed.permissions) ||
    !sameSet(actor.iam?.permissions ?? [], parsed.permissions)
  )
    return failure('NATIVE_PROJECTS_CONTEXT_CHANGED');
  return parsed;
}

function commandBody(body: unknown): { action: Action; expectedVersion: number; value: Row } {
  if (!record(body) || typeof body.action !== 'string' || !actions.has(body.action as Action))
    return failure('NATIVE_PROJECTS_REQUEST_INVALID');
  if (!integer(body.expectedVersion, 1))
    return failure('NATIVE_PROJECTS_REQUEST_INVALID');
  return {
    action: body.action as Action,
    expectedVersion: body.expectedVersion,
    value: body,
  };
}

function receiptData(
  value: unknown,
  projectId: string,
  key: string,
  input: { action: Action; expectedVersion: number; value: Row },
): NativeProjectStructureAcknowledgement {
  const action = input.action;
  if (!exact(value, receiptKeys[action]))
    return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  if (
    value.id !== projectId ||
    value.version !== input.expectedVersion + 1 ||
    value.status !== receiptStatus[action]
  )
    return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  const acknowledgement: {
    id: string;
    version: number;
    status: string;
    taskId?: string;
    bindingVersion?: number;
    actualId?: string;
    handoffId?: string;
    snapshotSha256?: string;
  } = { id: value.id, version: value.version, status: value.status };
  if (action === 'assign-task' || action === 'unassign-task') {
    if (
      !identifier(value.task_id) ||
      value.task_id !== input.value.taskId ||
      !integer(value.binding_version) ||
      value.binding_version !==
        (action === 'assign-task'
          ? Number(input.value.bindingVersion ?? 0) + 1
          : 0)
    )
      return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
    acknowledgement.taskId = String(value.task_id);
    acknowledgement.bindingVersion = Number(value.binding_version);
  } else if (action === 'link-actual') {
    if (value.actual_id !== `project-package-actual-${key}`)
      return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
    acknowledgement.actualId = String(value.actual_id);
  } else if (action === 'prepare-handoff') {
    if (value.handoff_id !== `project-handoff-${key}` || !digest(value.snapshot_sha256))
      return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
    acknowledgement.handoffId = String(value.handoff_id);
    acknowledgement.snapshotSha256 = value.snapshot_sha256;
  } else {
    if (
      value.handoff_id !== input.value.handoffId ||
      !digest(value.snapshot_sha256)
    )
      return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
    acknowledgement.handoffId = String(value.handoff_id);
    acknowledgement.snapshotSha256 = value.snapshot_sha256;
  }
  return acknowledgement;
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

async function boundedReply(response: Response): Promise<unknown> {
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

function commandCall(
  transport: NativeProjectStructureTransport,
  base: URL,
  holdings: Map<string, Holding>,
  incoming: Pick<Headers, 'get'>,
  actor: NativeProjectsPageContext,
  input: NativeProjectStructureCommandInput,
  body: Uint8Array,
  timeout: number,
): Promise<Response> {
  if (!actor.authority || !actor.iam)
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_CONTEXT_CHANGED'));
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== actor.holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_AUTHORITY_INVALID'));
  if (
    !identifier(input.projectId) ||
    input.projectId.includes('/') ||
    input.projectId.includes('\\')
  )
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_REQUEST_INVALID'));
  if (
    !text(input.key, 180) ||
    input.key.length < 8 ||
    !/^[a-zA-Z0-9:_.-]+$/.test(input.key)
  )
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_REQUEST_INVALID'));
  const captured = {
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority.revision),
    'x-erpx-context-assignment-version': String(actor.authority.assignmentVersion),
  } as const;
  if (Object.entries(captured).some(([name, value]) => incoming.get(name) !== value))
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_CONTEXT_CHANGED'));
  const origin = incoming.get('origin');
  if (origin !== holding.origin)
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_AUTHORITY_INVALID'));
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    origin,
    'cache-control': 'no-cache',
    'content-type': 'application/json',
    'idempotency-key': input.key,
    ...captured,
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  if (actor.scope.kind !== 'holding' && actor.scope.branchSlug)
    headers.set('x-erpx-workspace-branch', actor.scope.branchSlug);
  if (actor.scope.kind === 'department')
    headers.set('x-erpx-workspace-department', actor.scope.departmentSlug);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/projects/${encodeURIComponent(input.projectId)}/structure`,
    base,
  );
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeout);
  return transport('POST', url, headers, body, controller.signal)
    .finally(() => clearTimeout(timer))
    .catch((error: unknown) => {
      if (error instanceof NativeProjectsError) throw error;
      return failure('NATIVE_PROJECTS_UNAVAILABLE');
    });
}

export function createNativeProjectStructureClient(
  raw: NativeProjectsConfiguration,
  transport: NativeProjectStructureTransport = directHttpPost,
) {
  const config = settings(raw);
  return {
    async command(
      incoming: Pick<Headers, 'get'>,
      actor: NativeProjectsPageContext,
      input: NativeProjectStructureCommandInput,
    ): Promise<NativeProjectStructureAcknowledgement> {
      const parsed = commandBody(input.body);
      let payload: string;
      try {
        payload = JSON.stringify(input.body);
      } catch {
        return failure('NATIVE_PROJECTS_REQUEST_INVALID');
      }
      if (typeof payload !== 'string')
        return failure('NATIVE_PROJECTS_REQUEST_INVALID');
      const body = new TextEncoder().encode(payload);
      if (body.byteLength > bodyLimit)
        return failure('NATIVE_PROJECTS_REQUEST_INVALID');
      const response = await commandCall(
        transport,
        config.base,
        config.holdings,
        incoming,
        actor,
        input,
        body,
        config.timeout,
      );
      const value = await boundedReply(response);
      if (!exact(value, ['data', 'context']))
        return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
      verifyContext(value.context, actor);
      return receiptData(value.data, input.projectId, input.key, parsed);
    },
  };
}

/** Fresh bounded socket; bypasses patched fetch/HMR and ambient proxies. */
export function directHttpPost(
  method: 'POST',
  url: URL,
  headers: Headers,
  body: Uint8Array,
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

/** Native-only route entry. An absent or invalid configuration is a 503. */
export async function nativeProjectStructureCommand(
  request: Request,
  actor: NativeProjectsPageContext,
  holdingSlug: string,
  projectId: string,
  key: string,
  body: unknown,
): Promise<NativeProjectStructureAcknowledgement> {
  const raw = process.env.ERPX_NATIVE_PROJECTS_CONFIG;
  if (raw === undefined)
    throw new ApiError(
      503,
      'NATIVE_PROJECTS_UNAVAILABLE',
      'Native Projects structure commands are not configured.',
    );
  try {
    const client = createNativeProjectStructureClient(nativeProjectsConfiguration(raw));
    if (actor.holdingSlug !== holdingSlug)
      throw new NativeProjectsError('NATIVE_PROJECTS_CONTEXT_CHANGED');
    return await client.command(request.headers, actor, { projectId, key, body });
  } catch (error) {
    const mapped = nativeProjectsFailure(error);
    if (mapped) throw new ApiError(mapped.status, mapped.code, mapped.message);
    throw error;
  }
}
