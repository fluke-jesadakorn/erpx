import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { Readable } from 'node:stream';
import { ApiError } from '@erpx/server-runtime/public/http';

/**
 * Strict private-loopback client for the native Projects milestone and actual
 * command routes. The retained staff/finance routes forward the caller's
 * cookie, selected Work context, Origin and Idempotency-Key, then accept only
 * the exact native `{data, context}` receipt. There is no TypeScript writer
 * fallback: an absent or invalid Projects configuration is unavailable.
 */

export type NativeProjectCommandOperation =
  | 'work-log.save'
  | 'work-log.review'
  | 'cost-allocation.create'
  | 'cost-allocation.review'
  | 'milestone.save'
  | 'milestone.transition'
  | 'project.save';

export type NativeProjectCommandInput = Readonly<{
  operation: NativeProjectCommandOperation;
  /** Existing project id; absent only for a `project.save` create. */
  projectId?: string;
  /** Present for revisions, decisions and transitions; absent for creates. */
  recordId?: string;
  key: string;
  /** Already schema-validated camel-case owner input. */
  body: unknown;
  /** Current server-resolved workspace scope; never taken from the body. */
  branchSlug?: string;
  departmentSlug?: string;
}>;

export type NativeProjectCommandReceipt = Readonly<{
  id: string;
  version: number;
  state?: string;
}>;

type Row = Record<string, unknown>;
type Holding = Readonly<{ holdingSlug: string; origin: string }>;
type Configuration = Readonly<{
  base: URL;
  holdings: Map<string, Holding>;
  timeoutMs: number;
}>;

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const contextHeaders = [
  'x-erpx-context-company',
  'x-erpx-context-assignment',
  'x-erpx-context-revision',
  'x-erpx-context-assignment-version',
] as const;
const workLogStates = new Set(['submitted', 'approved', 'rejected']);
const allocationStates = new Set(['draft', 'approved', 'rejected']);
const milestoneStates = new Set(['planned', 'in_progress', 'completed', 'cancelled']);

const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string =>
  text(value, 200) && value.length > 0 && !value.includes('/') && !value.includes('\\');
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const optionalIdentifier = (value: unknown) => value === null || identifier(value);
const texts = (value: unknown, max = 256): value is string[] =>
  Array.isArray(value) && value.length <= max && value.every((item) => text(item, 256));

function unavailable(message = 'The native Projects service is unavailable.'): never {
  throw new ApiError(503, 'NATIVE_PROJECTS_UNAVAILABLE', message);
}

function invalid(message = 'The native Projects request could not be completed.'): never {
  throw new ApiError(503, 'NATIVE_PROJECTS_UNAVAILABLE', message);
}

function configuration(raw: string | undefined): Configuration {
  if (raw === undefined) return unavailable();
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return invalid();
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs, 250) ||
    value.timeoutMs > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return invalid();
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return invalid();
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
    value.privateBaseUrl !== base.origin
  )
    return invalid();
  const holdings = new Map<string, Holding>();
  for (const holding of value.holdings as unknown[]) {
    if (
      !record(holding) ||
      !text(holding.origin) ||
      !text(holding.holdingSlug, 128) ||
      !text(holding.holdingId) ||
      !identifier(holding.holdingId) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug)
    )
      return invalid();
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return invalid();
    }
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      origin.username ||
      origin.password ||
      holdings.has(origin.host)
    )
      return invalid();
    holdings.set(origin.host, {
      holdingSlug: holding.holdingSlug,
      origin: holding.origin,
    });
  }
  return { base, holdings, timeoutMs: value.timeoutMs };
}

function path(
  operation: NativeProjectCommandOperation,
  projectId: string | undefined,
  recordId?: string,
) {
  if (operation === 'project.save') {
    return projectId === undefined
      ? '/api/v1/holdings/{slug}/projects'
      : `/api/v1/holdings/{slug}/projects/${encodeURIComponent(projectId)}`;
  }
  if (projectId === undefined) return undefined;
  const root = `/api/v1/holdings/{slug}/projects/${encodeURIComponent(projectId)}`;
  switch (operation) {
    case 'work-log.save':
      return `${root}/work-logs`;
    case 'work-log.review':
      return recordId ? `${root}/work-logs/${encodeURIComponent(recordId)}/decision` : undefined;
    case 'cost-allocation.create':
      return `${root}/cost-allocations`;
    case 'cost-allocation.review':
      return recordId
        ? `${root}/cost-allocations/${encodeURIComponent(recordId)}/decision`
        : undefined;
    case 'milestone.save':
      return recordId ? `${root}/milestones/${encodeURIComponent(recordId)}` : `${root}/milestones`;
    case 'milestone.transition':
      return recordId
        ? `${root}/milestones/${encodeURIComponent(recordId)}/transition`
        : undefined;
  }
}

function expectedId(
  operation: NativeProjectCommandOperation,
  projectId?: string,
  recordId?: string,
): { exact?: string; prefix?: string } {
  if (recordId) return { exact: recordId };
  if (operation === 'project.save')
    return projectId ? { exact: projectId } : { prefix: 'project-' };
  if (operation === 'work-log.save') return { prefix: 'project-time-' };
  if (operation === 'cost-allocation.create') return { prefix: 'project-cost-' };
  if (operation === 'milestone.save') return { prefix: 'milestone-' };
  return {};
}

function verifyContext(value: unknown, headers: Headers, input: NativeProjectCommandInput): void {
  if (
    !exact(value, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug',
      'departmentSlug', 'assignmentId', 'assignmentVersion', 'revision', 'role',
      'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ]) ||
    !identifier(value.userId) ||
    !identifier(value.holdingId) ||
    !identifier(value.companyId) ||
    !optionalIdentifier(value.branchId) ||
    !optionalIdentifier(value.departmentId) ||
    !optionalIdentifier(value.branchSlug) ||
    !optionalIdentifier(value.departmentSlug) ||
    !identifier(value.assignmentId) ||
    !integer(value.assignmentVersion, 1) ||
    !integer(value.revision) ||
    !text(value.role) ||
    !text(value.assignmentScopeKind) ||
    !texts(value.modules) ||
    !texts(value.permissions) ||
    !integer(value.expiresAt, 1) ||
    value.expiresAt <= Date.now()
  )
    invalid();
  const company = headers.get('x-erpx-context-company');
  const assignment = headers.get('x-erpx-context-assignment');
  const revision = headers.get('x-erpx-context-revision');
  const assignmentVersion = headers.get('x-erpx-context-assignment-version');
  if (
    value.companyId !== company ||
    value.assignmentId !== assignment ||
    String(value.revision) !== revision ||
    String(value.assignmentVersion) !== assignmentVersion ||
    value.branchSlug !== (input.branchSlug ?? null) ||
    value.departmentSlug !== (input.departmentSlug ?? null)
  )
    throw new ApiError(
      409,
      'WORK_CONTEXT_CHANGED',
      'Your work role changed. Reload and review before retrying.',
    );
}

function receipt(
  value: unknown,
  operation: NativeProjectCommandOperation,
  expected: { exact?: string; prefix?: string },
): NativeProjectCommandReceipt {
  const withState =
    operation === 'work-log.save' ||
    operation === 'work-log.review' ||
    operation === 'cost-allocation.create' ||
    operation === 'cost-allocation.review' ||
    operation === 'milestone.transition';
  if (
    !exact(value, withState ? ['id', 'version', 'state'] : ['id', 'version']) ||
    !identifier(value.id) ||
    !integer(value.version, 1) ||
    (expected.exact !== undefined
      ? value.id !== expected.exact
      : expected.prefix !== undefined
        ? !value.id.startsWith(expected.prefix)
        : true) ||
    (withState && !text(value.state, 40))
  )
    invalid();
  if (
    (operation === 'work-log.save' || operation === 'work-log.review') &&
    !workLogStates.has(String(value.state))
  )
    invalid();
  if (
    (operation === 'cost-allocation.create' || operation === 'cost-allocation.review') &&
    !allocationStates.has(String(value.state))
  )
    invalid();
  if (operation === 'milestone.transition' && !milestoneStates.has(String(value.state)))
    invalid();
  const receiptId = value.id as string;
  const version = value.version as number;
  return withState
    ? { id: receiptId, version, state: String(value.state) }
    : { id: receiptId, version };
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
        return invalid();
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

async function reply(response: Response): Promise<unknown> {
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
    throw new ApiError(
      response.status >= 400 && response.status <= 599 ? response.status : 503,
      code ?? 'NATIVE_PROJECTS_UNAVAILABLE',
      message ?? 'The native Projects service is unavailable.',
    );
  }
  if (contentType !== 'application/json') return invalid();
  const body = await boundedBytes(response, 64 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(body));
  } catch {
    return invalid();
  }
}

/** Fresh bounded socket; bypasses patched fetch/HMR and ambient proxies. */
function direct(
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
    const outgoing = httpRequest(
      url,
      {
        method: 'POST',
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

export async function nativeProjectCommand(
  request: Request,
  holdingSlug: string,
  input: NativeProjectCommandInput,
): Promise<NativeProjectCommandReceipt> {
  const config = configuration(process.env.ERPX_NATIVE_PROJECTS_CONFIG);
  const host = request.headers.get('host');
  const holding = host ? config.holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== holdingSlug)
    throw new ApiError(404, 'NOT_FOUND', 'Projects resource not found.');
  const origin = request.headers.get('origin');
  if (origin !== holding.origin)
    throw new ApiError(403, 'ORIGIN_FORBIDDEN', 'Use the application origin.');
  if (input.operation === 'project.save') {
    if (input.recordId !== undefined)
      throw new ApiError(422, 'PROJECTS_REQUEST_INVALID', 'Review the Projects request.');
    if (input.projectId !== undefined && !identifier(input.projectId))
      throw new ApiError(422, 'PROJECTS_REQUEST_INVALID', 'Review the Projects request.');
  } else if (input.projectId === undefined || !identifier(input.projectId)) {
    throw new ApiError(422, 'PROJECTS_REQUEST_INVALID', 'Review the Projects request.');
  }
  if (!/^[a-zA-Z0-9:_.-]{8,180}$/.test(input.key))
    throw new ApiError(422, 'PROJECTS_REQUEST_INVALID', 'Review the Projects request.');
  if (input.recordId !== undefined && !identifier(input.recordId))
    throw new ApiError(422, 'PROJECTS_REQUEST_INVALID', 'Review the Projects request.');
  if (
    input.operation === 'work-log.review' ||
    input.operation === 'cost-allocation.review' ||
    input.operation === 'milestone.transition'
  ) {
    if (input.recordId === undefined)
      throw new ApiError(422, 'PROJECTS_REQUEST_INVALID', 'Review the Projects request.');
  }
  for (const name of contextHeaders) {
    const value = request.headers.get(name);
    if (value === null || !text(value, 200))
      throw new ApiError(
        409,
        'WORK_CONTEXT_CHANGED',
        'Your work role changed. Reload and review before retrying.',
      );
  }
  const suffix = path(input.operation, input.projectId, input.recordId);
  if (!suffix) throw new ApiError(422, 'PROJECTS_REQUEST_INVALID', 'Review the Projects request.');
  const body = new TextEncoder().encode(JSON.stringify(input.body));
  if (body.byteLength === 0 || body.byteLength > 64 * 1024)
    throw new ApiError(422, 'PROJECTS_REQUEST_INVALID', 'Review the Projects request.');
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'content-type': 'application/json',
    origin,
    'idempotency-key': input.key,
  });
  for (const name of contextHeaders) {
    const value = request.headers.get(name);
    if (value !== null) headers.set(name, value);
  }
  for (const [name, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        throw new ApiError(422, 'PROJECTS_REQUEST_INVALID', 'Review the Projects request.');
      headers.set(name, slug);
    }
  }
  const cookie = request.headers.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      throw new ApiError(431, 'NATIVE_PROJECTS_COOKIE_LIMIT', 'Use a smaller session cookie.');
    headers.set('cookie', cookie);
  }
  const url = new URL(
    suffix.replace('{slug}', encodeURIComponent(holdingSlug)),
    config.base,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  request.signal.addEventListener('abort', cancel, { once: true });
  if (request.signal.aborted) controller.abort();
  const timer = setTimeout(cancel, config.timeoutMs);
  try {
    let response: Response;
    try {
      response = await direct(url, headers, body, controller.signal);
    } catch {
      return unavailable();
    }
    const value = await reply(response);
    if (!exact(value, ['data', 'context'])) invalid();
    verifyContext(value.context, headers, input);
    return receipt(
      value.data,
      input.operation,
      expectedId(input.operation, input.projectId, input.recordId),
    );
  } finally {
    clearTimeout(timer);
    request.signal.removeEventListener('abort', cancel);
  }
}
