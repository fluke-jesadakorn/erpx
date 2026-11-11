import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';
import {
  NativeProjectsError,
  nativeProjectsConfiguration,
  type NativeProjectsConfiguration,
  type NativeProjectsPageContext,
} from './native-project-wbs';

export type NativeProjectAttachmentManifestItem = Readonly<{
  linkId: string;
  sourceId: string;
  purposeKey: string;
  checksumSha256: string;
  mediaType: string;
  bytes: number;
}>;

export type NativeProjectAttachmentBinding = Readonly<{
  setId: string;
  version: number;
  manifest: NativeProjectAttachmentManifestItem[];
}>;

export type NativeProjectAttachmentBindInput = Readonly<{
  key: string;
  expectedVersion: number;
}>;

export type NativeProjectAttachmentTransport = (
  method: 'POST',
  url: URL,
  headers: Headers,
  body: Uint8Array,
  signal: AbortSignal,
) => Promise<Response>;

type Row = Record<string, unknown>;
type Context = Row & { modules: string[]; permissions: string[] };

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
  const holdings = new Map<
    string,
    { holdingId: string; holdingSlug: string; origin: string }
  >();
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

function bindingData(
  value: unknown,
  setId: string,
  expectedVersion: number,
): NativeProjectAttachmentBinding {
  if (
    !exact(value, [
      'id', 'version', 'status', 'ownerVersion', 'attachmentSetId',
      'attachmentLinkId', 'manifest', 'setId',
    ])
  )
    return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  if (
    !identifier(value.id) ||
    !integer(value.version, 1) ||
    value.status !== 'bound' ||
    !integer(value.ownerVersion, 1) ||
    value.attachmentSetId !== setId ||
    value.setId !== setId ||
    value.version !== expectedVersion + 1 ||
    value.ownerVersion !== expectedVersion + 1 ||
    !identifier(value.attachmentLinkId) ||
    !Array.isArray(value.manifest) ||
    value.manifest.length < 1 ||
    value.manifest.length > 100
  )
    return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  const seen = new Set<string>();
  for (const item of value.manifest) {
    if (
      !exact(item, [
        'linkId', 'sourceId', 'purposeKey', 'checksumSha256', 'mediaType', 'bytes',
      ]) ||
      !identifier(item.linkId) ||
      !identifier(item.sourceId) ||
      !text(item.purposeKey) ||
      !/^[a-f0-9]{64}$/.test(String(item.checksumSha256)) ||
      !text(item.mediaType) ||
      !integer(item.bytes, 1) ||
      item.bytes > 10 * 1024 * 1024 ||
      seen.has(item.linkId)
    )
      return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
    seen.add(item.linkId);
  }
  const manifest = value.manifest as NativeProjectAttachmentManifestItem[];
  if (manifest[0]!.linkId !== value.attachmentLinkId)
    return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
  return { setId, version: value.ownerVersion, manifest };
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

function requestCall(
  transport: NativeProjectAttachmentTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string; origin: string }>,
  incoming: Pick<Headers, 'get'>,
  actor: NativeProjectsPageContext,
  setId: string,
  input: NativeProjectAttachmentBindInput,
  timeout: number,
): Promise<Response> {
  if (!actor.authority || !actor.iam)
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_CONTEXT_CHANGED'));
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== actor.holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_AUTHORITY_INVALID'));
  if (!identifier(setId) || setId.includes('/') || setId.includes('\\'))
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_REQUEST_INVALID'));
  if (!identifier(input.key) || input.key.length < 8 || input.key.length > 180 || !/^[a-zA-Z0-9:_.-]+$/.test(input.key))
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_REQUEST_INVALID'));
  if (!integer(input.expectedVersion, 1))
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_REQUEST_INVALID'));
  const captured = {
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority.revision),
    'x-erpx-context-assignment-version': String(actor.authority.assignmentVersion),
  } as const;
  if (
    Object.entries(captured).some(
      ([name, value]) => incoming.get(name) !== value,
    )
  )
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
  const body = new TextEncoder().encode(
    JSON.stringify({ expectedTargetVersion: input.expectedVersion }),
  );
  if (body.byteLength > 1024)
    return Promise.reject(new NativeProjectsError('NATIVE_PROJECTS_REQUEST_INVALID'));
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/project-attachments/${encodeURIComponent(setId)}/bind`,
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

export function nativeProjectAttachmentConfigured(): boolean {
  return process.env.ERPX_NATIVE_PROJECTS_CONFIG !== undefined;
}

export function createNativeProjectAttachmentClient(
  raw: NativeProjectsConfiguration,
  transport: NativeProjectAttachmentTransport = directHttpPost,
) {
  const config = settings(JSON.stringify(raw));
  return {
    async bind(
      incoming: Pick<Headers, 'get'>,
      actor: NativeProjectsPageContext,
      setId: string,
      input: NativeProjectAttachmentBindInput,
    ): Promise<NativeProjectAttachmentBinding> {
      const response = await requestCall(
        transport,
        config.base,
        config.holdings,
        incoming,
        actor,
        setId,
        input,
        config.timeout,
      );
      const value = await boundedReply(response);
      if (!exact(value, ['data', 'context']))
        return failure('NATIVE_PROJECTS_RESPONSE_INVALID');
      verifyContext(value.context, actor);
      return bindingData(value.data, setId, input.expectedVersion);
    },
  };
}

/** Route switch entry. Once the Projects configuration is present a selected
 * project set never falls back to the retained generic binder. */
export async function nativeProjectAttachmentBind(
  incoming: Pick<Headers, 'get'>,
  actor: NativeProjectsPageContext,
  setId: string,
  input: NativeProjectAttachmentBindInput,
): Promise<NativeProjectAttachmentBinding> {
  const raw = process.env.ERPX_NATIVE_PROJECTS_CONFIG;
  if (raw === undefined)
    return failure('NATIVE_PROJECTS_CONFIGURATION_INVALID', 503);
  let parsed: NativeProjectsConfiguration;
  try {
    parsed = nativeProjectsConfiguration(raw);
  } catch {
    return failure('NATIVE_PROJECTS_CONFIGURATION_INVALID', 503);
  }
  return createNativeProjectAttachmentClient(parsed).bind(incoming, actor, setId, input);
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
