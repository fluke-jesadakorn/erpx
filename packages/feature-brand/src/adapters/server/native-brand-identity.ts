import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';

/** Rendering input from the retained page authorization, never a grant. */
export type NativeBrandIdentityPageContext = {
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

export type NativeBrandIdentityConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;
type Identity = { identity: { shortName: string; tagline: string }; visual: { accentColor: string; logoPublicationId: string | null } };
export type NativeBrandIdentityWorkspace = {
  drafts: Row[];
  versions: Row[];
  locks: Row[];
  history: Row[];
  active: (Row & { identity: Identity; active_version: number }) | null;
  initial: Identity;
  media: { id: string; name: string | null }[];
  actorId: string;
  canManage: boolean;
  canLock: boolean;
};
type Context = Row & { modules: string[]; permissions: string[] };
export type NativeBrandIdentityReadInput = Readonly<{
  holdingSlug: string;
  branchSlug?: string;
  departmentSlug?: string;
  expected?: Readonly<{ companyId: string; assignmentId: string; revision: number; assignmentVersion: number }>;
}>;
export type NativeBrandIdentityCommandInput = Readonly<{
  holdingSlug: string;
  branchSlug?: string;
  departmentSlug?: string;
  expected?: Readonly<{ companyId: string; assignmentId: string; revision: number; assignmentVersion: number }>;
  body: unknown;
  idempotencyKey: string;
  origin?: string;
}>;
export type NativeBrandIdentityTransport = (
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeBrandIdentityError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number) {
    super('The native Brand identity request could not be completed.');
    this.name = 'NativeBrandIdentityError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number): never => {
  throw new NativeBrandIdentityError(code, status);
};
const record = (value: unknown): value is Row => value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) && Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string => text(value, 200) && value.length > 0;
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const optionalText = (value: unknown, max = 4096) => value === null || text(value, max);
const texts = (value: unknown, max = 4096): value is string[] =>
  Array.isArray(value) && value.length <= max && value.every((item) => text(item, 256));
const sameSet = (left: readonly string[], right: readonly string[]) => {
  const a = new Set(left);
  const b = new Set(right);
  return a.size === b.size && [...a].every((value) => b.has(value));
};
const identity = (value: unknown): value is Identity =>
  exact(value, ['identity', 'visual']) &&
  exact(value.identity, ['shortName', 'tagline']) &&
  text(value.identity.shortName, 40) &&
  value.identity.shortName.length > 0 &&
  text(value.identity.tagline, 160) &&
  exact(value.visual, ['accentColor', 'logoPublicationId']) &&
  typeof value.visual.accentColor === 'string' &&
  /^#[a-fA-F0-9]{6}$/.test(value.visual.accentColor) &&
  optionalText(value.visual.logoPublicationId, 200);

const draftRow = (row: Row) =>
  identifier(row.id) &&
  text(row.company_id, 200) &&
  integer(row.based_revision) &&
  text(row.identity_json, 65536) &&
  ['draft', 'published', 'cancelled'].includes(String(row.status)) &&
  integer(row.version, 1) &&
  identifier(row.created_by) &&
  identifier(row.updated_by) &&
  integer(row.created_at) &&
  integer(row.updated_at);
const versionRow = (row: Row) =>
  identifier(row.id) &&
  text(row.company_id, 200) &&
  integer(row.revision, 1) &&
  text(row.identity_json, 65536) &&
  identifier(row.draft_id) &&
  integer(row.draft_version, 1) &&
  identifier(row.created_by) &&
  identifier(row.published_by) &&
  integer(row.published_at);
const lockRow = (row: Row) =>
  text(row.company_id, 200) &&
  ['identity', 'visual'].includes(String(row.category)) &&
  typeof row.locked === 'boolean' &&
  text(row.approved_snapshot_json, 65536) &&
  integer(row.version) &&
  identifier(row.updated_by) &&
  text(row.reason, 4000) &&
  integer(row.updated_at);
const historyRow = (row: Row) =>
  identifier(row.id) &&
  text(row.company_id, 200) &&
  identifier(row.record_id) &&
  text(row.action, 200) &&
  integer(row.version, 1) &&
  identifier(row.actor_id) &&
  text(row.reason, 4000) &&
  text(row.details_json, 65536) &&
  integer(row.created_at);

function workspace(value: unknown, actor: NativeBrandIdentityPageContext, input: NativeBrandIdentityReadInput): NativeBrandIdentityWorkspace {
  if (
    !exact(value, ['data', 'context']) ||
    !exact(value.data, ['drafts', 'versions', 'locks', 'history', 'active', 'initial', 'media', 'actorId', 'canManage', 'canLock'])
  )
    return failure('NATIVE_BRAND_RESPONSE_INVALID');
  const data = value.data;
  const context = value.context as Context;
  if (
    !exact(context, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug', 'departmentSlug',
      'assignmentId', 'assignmentVersion', 'revision', 'role', 'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ]) ||
    context.userId !== actor.userId ||
    context.holdingId !== actor.holdingId ||
    context.companyId !== actor.companyId ||
    context.branchId !== actor.branchId ||
    context.departmentId !== actor.departmentId ||
    context.branchSlug !== (input.branchSlug ?? null) ||
    context.departmentSlug !== (input.departmentSlug ?? null) ||
    context.assignmentId !== actor.assignmentId ||
    context.assignmentVersion !== actor.authority?.assignmentVersion ||
    context.revision !== actor.authority?.revision ||
    context.role !== actor.role ||
    context.assignmentScopeKind !== actor.assignmentScope ||
    !integer(context.expiresAt, 1) ||
    !texts(context.modules) ||
    !sameSet(actor.modules, context.modules) ||
    !texts(context.permissions) ||
    !sameSet(actor.iam?.permissions ?? [], context.permissions)
  )
    return failure('NATIVE_BRAND_CONTEXT_CHANGED');
  if (
    !Array.isArray(data.drafts) || !data.drafts.every((row) => record(row) && draftRow(row)) ||
    !Array.isArray(data.versions) || !data.versions.every((row) => record(row) && versionRow(row)) ||
    !Array.isArray(data.locks) || !data.locks.every((row) => record(row) && lockRow(row)) ||
    !Array.isArray(data.history) || !data.history.every((row) => record(row) && historyRow(row)) ||
    !identity(data.initial) ||
    data.actorId !== actor.userId ||
    typeof data.canManage !== 'boolean' ||
    typeof data.canLock !== 'boolean'
  )
    return failure('NATIVE_BRAND_RESPONSE_INVALID');
  if (
    data.active !== null &&
    (!record(data.active) ||
      !versionRow(data.active) ||
      !integer(data.active.active_version, 1) ||
      !identity(data.active.identity))
  )
    return failure('NATIVE_BRAND_RESPONSE_INVALID');
  if (
    !Array.isArray(data.media) ||
    data.media.length > 1000 ||
    !data.media.every((item) => record(item) && exact(item, ['id', 'name']) && identifier(item.id) && optionalText(item.name, 500))
  )
    return failure('NATIVE_BRAND_RESPONSE_INVALID');
  if (
    data.drafts.length + data.versions.length + data.locks.length + data.history.length > 2000 ||
    data.history.length > 200
  )
    return failure('NATIVE_BRAND_RESPONSE_INVALID');
  if (
    input.expected &&
    (context.companyId !== input.expected.companyId ||
      context.assignmentId !== input.expected.assignmentId ||
      context.revision !== input.expected.revision ||
      context.assignmentVersion !== input.expected.assignmentVersion)
  )
    return failure('NATIVE_BRAND_CONTEXT_CHANGED');
  return data as unknown as NativeBrandIdentityWorkspace;
}

function commandData(value: unknown): Row {
  if (!exact(value, ['data']) || !record(value.data)) return failure('NATIVE_BRAND_RESPONSE_INVALID');
  return value.data;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(configuration: NativeBrandIdentityConfiguration) {
  let base: URL;
  try {
    base = new URL(configuration.privateBaseUrl);
  } catch {
    return failure('NATIVE_BRAND_CONFIGURATION_INVALID');
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
    configuration.privateBaseUrl !== base.origin ||
    !integer(configuration.timeoutMs, 250) ||
    configuration.timeoutMs > 30000 ||
    configuration.holdings.length < 1 ||
    configuration.holdings.length > 64
  )
    return failure('NATIVE_BRAND_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of configuration.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_BRAND_CONFIGURATION_INVALID');
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
      return failure('NATIVE_BRAND_CONFIGURATION_INVALID');
    holdings.set(origin.host, { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug });
  }
  return { base, holdings, timeout: configuration.timeoutMs };
}

function transport_call(
  transport: NativeBrandIdentityTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  holdingSlug: string,
  branchSlug: string | undefined,
  departmentSlug: string | undefined,
  expected: NativeBrandIdentityReadInput['expected'],
  body: string | undefined,
  idempotencyKey?: string,
  origin?: string,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== holdingSlug) return Promise.reject(new NativeBrandIdentityError('NATIVE_BRAND_AUTHORITY_INVALID'));
  const headers = new Headers({ host: host!, accept: 'application/json', 'cache-control': 'no-cache' });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192) return Promise.reject(new NativeBrandIdentityError('NATIVE_BRAND_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  for (const [key, slug] of [
    ['x-erpx-workspace-branch', branchSlug],
    ['x-erpx-workspace-department', departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug)) return Promise.reject(new NativeBrandIdentityError('NATIVE_BRAND_REQUEST_INVALID'));
      headers.set(key, slug);
    }
  }
  if (expected) {
    if (!identifier(expected.companyId) || !identifier(expected.assignmentId) || !integer(expected.revision) || !integer(expected.assignmentVersion, 1))
      return Promise.reject(new NativeBrandIdentityError('NATIVE_BRAND_REQUEST_INVALID'));
    headers.set('x-erpx-context-company', expected.companyId);
    headers.set('x-erpx-context-assignment', expected.assignmentId);
    headers.set('x-erpx-context-revision', String(expected.revision));
    headers.set('x-erpx-context-assignment-version', String(expected.assignmentVersion));
  }
  if (body !== undefined) {
    if (new TextEncoder().encode(body).length > 64 * 1024) return Promise.reject(new NativeBrandIdentityError('NATIVE_BRAND_REQUEST_INVALID'));
    headers.set('content-type', 'application/json');
    if (!idempotencyKey || !/^[A-Za-z0-9:_.-]{8,180}$/.test(idempotencyKey)) return Promise.reject(new NativeBrandIdentityError('NATIVE_BRAND_COMMAND_KEY_INVALID'));
    headers.set('idempotency-key', idempotencyKey);
    if (origin) {
      let originUrl: URL;
      try {
        originUrl = new URL(origin);
      } catch {
        return Promise.reject(new NativeBrandIdentityError('NATIVE_BRAND_REQUEST_INVALID'));
      }
      if (origin !== originUrl.origin) return Promise.reject(new NativeBrandIdentityError('NATIVE_BRAND_REQUEST_INVALID'));
      headers.set('origin', origin);
    }
  }
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(holdingSlug)}/brand`, base);
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

async function boundedJson(response: Response): Promise<unknown> {
  if (response.redirected || response.status !== 200 || !response.body) {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_BRAND_UNAVAILABLE', response.status);
  }
  if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_BRAND_RESPONSE_INVALID');
  }
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.byteLength;
      if (bytes > 2 * 1024 * 1024) {
        await reader.cancel();
        return failure('NATIVE_BRAND_RESPONSE_LIMIT');
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
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_BRAND_RESPONSE_INVALID');
  }
}

export function createNativeBrandIdentityClient(
  raw: NativeBrandIdentityConfiguration,
  transport: NativeBrandIdentityTransport = directHttp,
) {
  const config = configuration(raw);
  const read = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeBrandIdentityPageContext,
    input: NativeBrandIdentityReadInput,
    signal?: AbortSignal,
  ): Promise<NativeBrandIdentityWorkspace> => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return failure('NATIVE_BRAND_CONTEXT_CHANGED');
    const response = await transport_call(
      transport,
      config.base,
      config.holdings,
      incoming,
      input.holdingSlug,
      input.branchSlug,
      input.departmentSlug,
      input.expected,
      undefined,
      undefined,
      undefined,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeBrandIdentityError) throw error;
      return failure('NATIVE_BRAND_UNAVAILABLE');
    });
    return workspace(await boundedJson(response), actor, input);
  };
  const command = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeBrandIdentityPageContext,
    input: NativeBrandIdentityCommandInput,
  ): Promise<Row> => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return failure('NATIVE_BRAND_CONTEXT_CHANGED');
    const body = JSON.stringify(input.body);
    const response = await transport_call(
      transport,
      config.base,
      config.holdings,
      incoming,
      input.holdingSlug,
      input.branchSlug,
      input.departmentSlug,
      input.expected,
      body,
      input.idempotencyKey,
      input.origin,
    ).catch((error: unknown) => {
      if (error instanceof NativeBrandIdentityError) throw error;
      return failure('NATIVE_BRAND_UNAVAILABLE');
    });
    return commandData(await boundedJson(response));
  };
  return { read, command };
}

export function nativeBrandIdentityConfiguration(raw: string): NativeBrandIdentityConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeBrandIdentityError('NATIVE_BRAND_CONFIGURATION_INVALID');
  }
  if (!record(value) || !text(value.privateBaseUrl) || !integer(value.timeoutMs) || !Array.isArray(value.holdings))
    throw new NativeBrandIdentityError('NATIVE_BRAND_CONFIGURATION_INVALID');
  if (
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) && text(holding.origin) && text(holding.holdingSlug) && text(holding.holdingId),
    )
  )
    throw new NativeBrandIdentityError('NATIVE_BRAND_CONFIGURATION_INVALID');
  return value as unknown as NativeBrandIdentityConfiguration;
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
    const agent = new Agent({ keepAlive: false, maxSockets: 1, maxTotalSockets: 1, proxyEnv: {} as import('node:http').ProxyEnv });
    const outgoing = request(
      url,
      { method, agent, headers: { ...Object.fromEntries(headers), ...(body === undefined ? {} : { 'content-length': String(new TextEncoder().encode(body).length) }) }, signal, maxHeaderSize: 8192 },
      (incoming) => {
        incoming.once('close', () => agent.destroy());
        try {
          const status = incoming.statusCode ?? 502;
          if (status !== 200) {
            incoming.destroy();
            resolve(new Response(null, { status: status >= 200 && status <= 599 ? status : 502 }));
            return;
          }
          const values = new Headers();
          for (let index = 0; index < incoming.rawHeaders.length; index += 2)
            values.append(incoming.rawHeaders[index]!, incoming.rawHeaders[index + 1]!);
          resolve(
            new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, {
              status,
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
    if (body !== undefined) outgoing.write(body);
    outgoing.end();
  });
}
