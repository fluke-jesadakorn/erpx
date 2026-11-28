import 'server-only';
import { Agent, request as httpRequest, type IncomingMessage } from 'node:http';
import { Readable } from 'node:stream';
import { headers } from 'next/headers';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import type { ProjectGraph } from '@erpx/projects-server/domain/graph';
import { ApiError } from '@erpx/platform-runtime-server/public/http';

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const MAX_BODY = 2 * 1024 * 1024;

type Holding = Readonly<{ holdingId: string; holdingSlug: string; origin: string }>;
type Config = Readonly<{ base: URL; holdings: Map<string, Holding>; timeoutMs: number }>;

class NativeGraphError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Projects graph request could not be completed.');
    this.name = 'NativeGraphError';
    this.code = code;
    this.status = status;
  }
}
const fail = (code: string, status?: number): never => { throw new NativeGraphError(code, status); };
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Record<string, unknown> =>
  record(value) && Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string => text(value, 200) && value.length > 0;
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;

function configuration(raw: string): Config {
  let config: unknown;
  try { config = JSON.parse(raw); } catch { return fail('NATIVE_PROJECTS_CONFIGURATION_INVALID'); }
  if (!record(config) || !text(config.privateBaseUrl, 512))
    return fail('NATIVE_PROJECTS_CONFIGURATION_INVALID');
  let base: URL;
  try { base = new URL(config.privateBaseUrl); } catch { return fail('NATIVE_PROJECTS_CONFIGURATION_INVALID'); }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    reserved.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username || base.password ||
    !record(config) ||
    config.privateBaseUrl !== base.origin ||
    !integer(config.timeoutMs, 250) ||
    (config.timeoutMs as number) > 30000 ||
    !Array.isArray(config.holdings) ||
    config.holdings.length < 1 ||
    config.holdings.length > 64
  )
    return fail('NATIVE_PROJECTS_CONFIGURATION_INVALID');
  const holdings = new Map<string, Holding>();
  for (const item of config.holdings) {
    if (!record(item) || !text(item.origin, 512) || !identifier(item.holdingSlug) || !identifier(item.holdingId))
      return fail('NATIVE_PROJECTS_CONFIGURATION_INVALID');
    let origin: URL;
    try { origin = new URL(item.origin); } catch { return fail('NATIVE_PROJECTS_CONFIGURATION_INVALID'); }
    if (
      origin.protocol !== 'https:' ||
      item.origin !== origin.origin ||
      origin.username || origin.password ||
      !/^[a-z0-9_-]+$/.test(item.holdingSlug) ||
      holdings.has(origin.host)
    )
      return fail('NATIVE_PROJECTS_CONFIGURATION_INVALID');
    holdings.set(origin.host, {
      holdingId: item.holdingId,
      holdingSlug: item.holdingSlug,
      origin: item.origin,
    });
  }
  return { base, holdings, timeoutMs: config.timeoutMs as number };
}

/** Validate the exact native `{data, context}` receipt and its caller-scoped identity. */
function graphData(value: unknown, actor: WorkContext, projectId: string): ProjectGraph {
  if (!exact(value, ['data', 'context']) || !record(value.data) || !record(value.context))
    return fail('NATIVE_PROJECTS_RESPONSE_INVALID');
  const context = value.context;
  if (
    context.userId !== actor.userId ||
    context.holdingId !== actor.holdingId ||
    context.companyId !== actor.companyId ||
    context.assignmentId !== actor.assignmentId ||
    context.assignmentVersion !== actor.authority?.assignmentVersion ||
    context.revision !== actor.authority?.revision ||
    context.role !== actor.role ||
    context.assignmentScopeKind !== actor.assignmentScope
  )
    return fail('NATIVE_PROJECTS_CONTEXT_CHANGED');
  const data = value.data;
  const project = data.project;
  if (
    !record(project) ||
    project.id !== projectId ||
    project.company_id !== actor.companyId ||
    !identifier(project.title) ||
    !integer(project.version, 1)
  )
    return fail('NATIVE_PROJECTS_RESPONSE_INVALID');
  for (const key of ['packages', 'tasks', 'actuals', 'edges', 'unassignedTasks'] as const) {
    if (!Array.isArray(data[key])) return fail('NATIVE_PROJECTS_RESPONSE_INVALID');
  }
  if (typeof data.canReadCosts !== 'boolean' || typeof data.isProjectOwner !== 'boolean' || !record(data.totals))
    return fail('NATIVE_PROJECTS_RESPONSE_INVALID');
  return data as unknown as ProjectGraph;
}

/** Fresh bounded socket; bypasses patched fetch/HMR and ambient proxies. */
function get(url: URL, outgoing: Readonly<Record<string, string>>, timeoutMs: number, signal: AbortSignal): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({ keepAlive: false, maxSockets: 1, maxTotalSockets: 1, proxyEnv: {} as import('node:http').ProxyEnv });
    const req = httpRequest(url, { method: 'GET', agent, headers: { ...outgoing }, signal, maxHeaderSize: 8192 }, (incoming: IncomingMessage) => {
      incoming.once('close', () => agent.destroy());
      try {
        const status = incoming.statusCode ?? 502;
        const values = new Headers();
        for (let index = 0; index < incoming.rawHeaders.length; index += 2)
          values.append(incoming.rawHeaders[index]!, incoming.rawHeaders[index + 1]!);
        resolve(new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, {
          status: status >= 200 && status <= 599 ? status : 502,
          headers: values,
        }));
      } catch (error) {
        incoming.destroy(); agent.destroy(); reject(error);
      }
    });
    req.once('error', (error) => { agent.destroy(); reject(error); });
    req.end();
  });
}

function mapFailure(error: unknown): never {
  if (
    (error instanceof NativeGraphError && error.status === 409) ||
    (error instanceof NativeGraphError && error.code === 'NATIVE_PROJECTS_CONTEXT_CHANGED')
  )
    throw new ApiError(409, 'WORK_CONTEXT_CHANGED', 'Your work role changed. Reload and review before retrying.');
  if (error instanceof NativeGraphError && (error.status === 401 || error.status === 403))
    throw new ApiError(error.status, error.status === 401 ? 'UNAUTHENTICATED' : 'FORBIDDEN', 'The native Projects graph read refused this caller.');
  throw new ApiError(503, 'NATIVE_PROJECTS_UNAVAILABLE', 'The native Projects graph read could not be completed.');
}

/**
 * Native Projects graph read. The mounted owner route performs the
 * Company/membership, purchase, context and serializable boundary and returns
 * the complete graph; there is no TypeScript graph query or fallback. An
 * absent or invalid native configuration is a 503.
 */
export type NativeGraphTransport = (
  url: URL,
  outgoing: Readonly<Record<string, string>>,
  timeoutMs: number,
  signal: AbortSignal,
) => Promise<Response>;

export function createNativeProjectGraphReader(
  incoming: Pick<Headers, 'get'>,
  transport: NativeGraphTransport = get,
) {
  return async function nativeProjectGraph(actor: WorkContext, projectId: string): Promise<ProjectGraph> {
    let raw: string | undefined;
    try { raw = process.env.ERPX_NATIVE_PROJECTS_CONFIG; } catch { raw = undefined; }
    if (raw === undefined) throw new ApiError(503, 'NATIVE_PROJECTS_UNAVAILABLE', 'The native Projects graph read is not configured.');
    try {
    const config = configuration(raw);
    const host = incoming.get('host');
    const origin = incoming.get('origin');
    if (!host || !origin || !identifier(projectId) || projectId.includes('/') || projectId.includes('\\'))
      return fail('NATIVE_PROJECTS_REQUEST_INVALID');
    let holding: Holding | undefined;
    try { holding = config.holdings.get(new URL(origin).host); } catch { return fail('NATIVE_PROJECTS_REQUEST_INVALID'); }
    if (!holding || holding.holdingId !== actor.holdingId || holding.holdingSlug !== actor.holdingSlug)
      return fail('NATIVE_PROJECTS_REQUEST_INVALID');
    const captured = {
      'x-erpx-context-company': actor.companyId,
      'x-erpx-context-assignment': actor.assignmentId,
      'x-erpx-context-revision': String(actor.authority?.revision),
      'x-erpx-context-assignment-version': String(actor.authority?.assignmentVersion),
    } as const;
    if (Object.entries(captured).some(([name, value]) => incoming.get(name) !== value))
      return fail('NATIVE_PROJECTS_CONTEXT_CHANGED');
    const outgoing: Record<string, string> = { host, accept: 'application/json', origin, 'cache-control': 'no-cache', ...captured };
    const cookie = incoming.get('cookie');
    if (cookie) {
      if (new TextEncoder().encode(cookie).length > 8192) return fail('NATIVE_PROJECTS_REQUEST_INVALID');
      outgoing.cookie = cookie;
    }
    if (actor.scope.kind === 'branch' && actor.scope.branchSlug) outgoing['x-erpx-workspace-branch'] = actor.scope.branchSlug;
    if (actor.scope.kind === 'department' && actor.scope.departmentSlug) outgoing['x-erpx-workspace-department'] = actor.scope.departmentSlug;
    const url = new URL(
      `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/projects/${encodeURIComponent(projectId)}/graph`,
      config.base,
    );
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), config.timeoutMs);
    try {
      const response = await transport(url, outgoing, config.timeoutMs, controller.signal);
      if (response.redirected || response.status !== 200)
        return fail('NATIVE_PROJECTS_UNAVAILABLE', response.status);
      if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json')
        return fail('NATIVE_PROJECTS_RESPONSE_INVALID');
      const bytes = await response.arrayBuffer();
      if (bytes.byteLength > MAX_BODY) return fail('NATIVE_PROJECTS_RESPONSE_LIMIT');
      const value = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
      return graphData(value, actor, projectId);
    } finally {
      clearTimeout(timer);
    }
  } catch (error) {
    if (error instanceof ApiError) throw error;
    return mapFailure(error);
  }
  };
}

/** Native Projects graph read bound to the current Next request headers. */
export async function nativeProjectGraph(actor: WorkContext, projectId: string): Promise<ProjectGraph> {
  return createNativeProjectGraphReader(await headers())(actor, projectId);
}
