import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';
import {
  NativeProjectsError,
  nativeProjectsConfiguration,
  type NativeProjectsConfiguration,
  type NativeProjectsPageContext,
} from './native-project-wbs';

type Row = Record<string, unknown>;

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
const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function settings(raw: string) {
  const value = nativeProjectsConfiguration(raw);
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
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
    value.privateBaseUrl !== base.origin ||
    !integer(value.timeoutMs, 250) ||
    value.timeoutMs > 30000
  )
    return failure('NATIVE_PROJECTS_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string; origin: string }>();
  for (const holding of value.holdings) {
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
  return { base, holdings, timeout: value.timeoutMs };
}

function verifyContext(value: unknown, actor: NativeProjectsPageContext) {
  if (
    !exact(value, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug',
      'departmentSlug', 'assignmentId', 'assignmentVersion', 'revision', 'role',
      'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ])
  )
    return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  const parsed = value as Row;
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

async function jsonReply(response: Response): Promise<unknown> {
  if (response.redirected) return failure('NATIVE_PROJECTS_UNAVAILABLE', response.status);
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
        // keep the bounded transport failure
      }
    }
    return failure(code ?? 'NATIVE_PROJECTS_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  }
}

export type NativeProjectAttachmentReadTransport = (
  method: 'GET',
  url: URL,
  headers: Headers,
  signal: AbortSignal,
) => Promise<Response>;

function call(
  transport: NativeProjectAttachmentReadTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string; origin: string }>,
  incoming: Pick<Headers, 'get'>,
  actor: NativeProjectsPageContext,
  path: string,
  timeout: number,
): Promise<Response> {
  if (!actor.authority || !actor.iam)
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_CONTEXT_CHANGED'));
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== actor.holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_AUTHORITY_INVALID'));
  const captured = {
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority.revision),
    'x-erpx-context-assignment-version': String(actor.authority.assignmentVersion),
  } as const;
  if (Object.entries(captured).some(([name, value]) => incoming.get(name) !== value))
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_CONTEXT_CHANGED'));
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    'cache-control': 'no-cache',
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
    `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}${path}`,
    base,
  );
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeout);
  return transport('GET', url, headers, controller.signal)
    .finally(() => clearTimeout(timer))
    .catch((error: unknown) => {
      if (error instanceof NativeProjectsError) throw error;
      return failure('NATIVE_PROJECTS_UNAVAILABLE');
    });
}

export function createNativeProjectAttachmentReadClient(
  raw: NativeProjectsConfiguration,
  transport: NativeProjectAttachmentReadTransport = directHttpGet,
) {
  const config = settings(JSON.stringify(raw));
  const read = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeProjectsPageContext,
    path: string,
  ): Promise<unknown> => {
    const value = await jsonReply(
      await call(transport, config.base, config.holdings, incoming, actor, path, config.timeout),
    );
    if (!exact(value, ['data', 'context'])) return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
    verifyContext(value.context, actor);
    return value.data;
  };
  return {
    sets: (incoming: Pick<Headers, 'get'>, actor: NativeProjectsPageContext, projectId: string) =>
      read(incoming, actor, `/projects/${encodeURIComponent(projectId)}/attachment-sets`),
    links: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeProjectsPageContext,
      projectId: string,
      setId: string,
    ) =>
      read(
        incoming,
        actor,
        `/projects/${encodeURIComponent(projectId)}/attachment-sets/${encodeURIComponent(setId)}/links`,
      ),
    status: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeProjectsPageContext,
      projectId: string,
      linkId: string,
    ) =>
      read(
        incoming,
        actor,
        `/projects/${encodeURIComponent(projectId)}/attachment-links/${encodeURIComponent(linkId)}/status`,
      ),
    content: async (
      incoming: Pick<Headers, 'get'>,
      actor: NativeProjectsPageContext,
      projectId: string,
      linkId: string,
      variant: 'original' | 'preview',
    ): Promise<Response> => {
      const response = await call(
        transport,
        config.base,
        config.holdings,
        incoming,
        actor,
        `/projects/${encodeURIComponent(projectId)}/attachment-links/${encodeURIComponent(linkId)}/content?variant=${variant}`,
        config.timeout,
      );
      if (response.status !== 200) {
        await jsonReply(response);
        return failure('NATIVE_PROJECTS_UNAVAILABLE', response.status);
      }
      const body = await boundedBytes(response, 10 * 1024 * 1024);
      const headers = new Headers();
      for (const name of [
        'content-type',
        'content-disposition',
        'cache-control',
        'x-content-type-options',
        'content-security-policy',
        'accept-ranges',
      ]) {
        const value = response.headers.get(name);
        if (value) headers.set(name, value);
      }
      return new Response(body as unknown as BodyInit, { headers });
    },
  };
}

export async function nativeProjectAttachmentSets(
  incoming: Pick<Headers, 'get'>,
  actor: NativeProjectsPageContext,
  projectId: string,
): Promise<unknown> {
  const raw = process.env.ERPX_NATIVE_PROJECTS_CONFIG;
  if (raw === undefined) return failure('NATIVE_PROJECTS_CONFIGURATION_INVALID', 503);
  return createNativeProjectAttachmentReadClient(nativeProjectsConfiguration(raw)).sets(
    incoming,
    actor,
    projectId,
  );
}

export async function nativeProjectAttachmentStatus(
  incoming: Pick<Headers, 'get'>,
  actor: NativeProjectsPageContext,
  projectId: string,
  linkId: string,
): Promise<unknown> {
  const raw = process.env.ERPX_NATIVE_PROJECTS_CONFIG;
  if (raw === undefined) return failure('NATIVE_PROJECTS_CONFIGURATION_INVALID', 503);
  return createNativeProjectAttachmentReadClient(nativeProjectsConfiguration(raw)).status(
    incoming,
    actor,
    projectId,
    linkId,
  );
}

export async function nativeProjectAttachmentContent(
  incoming: Pick<Headers, 'get'>,
  actor: NativeProjectsPageContext,
  projectId: string,
  linkId: string,
  variant: 'original' | 'preview',
): Promise<Response> {
  const raw = process.env.ERPX_NATIVE_PROJECTS_CONFIG;
  if (raw === undefined) return failure('NATIVE_PROJECTS_CONFIGURATION_INVALID', 503);
  return createNativeProjectAttachmentReadClient(nativeProjectsConfiguration(raw)).content(
    incoming,
    actor,
    projectId,
    linkId,
    variant,
  );
}

export function directHttpGet(
  method: 'GET',
  url: URL,
  headers: Headers,
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
    outgoing.end();
  });
}
