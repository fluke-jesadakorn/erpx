import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';

/** Rendering input from the retained route authorization, never a grant. */
export type NativeOrganizationPageContext = {
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

export type NativeOrganizationConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;
export type NativeOrganizationWorkspace = {
  nodes: Row[];
  nodeChanges: Row[];
  changeQueue: Row;
  authorizedChannelNodes: string[];
  channels: Row[];
  actorId: string;
  canManage: boolean;
  canManageUnits: boolean;
};

export class NativeOrganizationError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number) {
    super('The native Organization request could not be completed.');
    this.name = 'NativeOrganizationError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number): never => {
  throw new NativeOrganizationError(code, status);
};
const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) && Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string => text(value, 200) && value.length > 0;
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const optionalText = (value: unknown, max = 4096) => value === null || text(value, max);

function workspace(value: unknown): NativeOrganizationWorkspace {
  if (
    !exact(value, ['data']) ||
    !exact(value.data, [
      'nodes',
      'nodeChanges',
      'changeQueue',
      'authorizedChannelNodes',
      'channels',
      'actorId',
      'canManage',
      'canManageUnits',
    ])
  )
    return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  const data = value.data;
  if (
    !Array.isArray(data.nodes) ||
    data.nodes.length > 4000 ||
    !data.nodes.every(
      (node) =>
        record(node) &&
        identifier(node.id) &&
        text(node.kind, 40) &&
        text(node.slug, 80) &&
        text(node.display_name, 120) &&
        optionalText(node.parent_id, 200) &&
        text(node.company_id, 200) &&
        optionalText(node.branch_id, 200) &&
        ['active', 'suspended', 'archived'].includes(String(node.status)) &&
        integer(node.hierarchy_revision, 1),
    ) ||
    !Array.isArray(data.nodeChanges) ||
    data.nodeChanges.length > 1000 ||
    !data.nodeChanges.every(
      (change) =>
        record(change) &&
        identifier(change.id) &&
        optionalText(change.target_node_id, 200) &&
        ['create', 'rename', 'suspend', 'reactivate', 'archive', 'relocate'].includes(String(change.action)) &&
        ['branch', 'department'].includes(String(change.node_kind)) &&
        optionalText(change.slug, 80) &&
        text(change.display_name, 120) &&
        optionalText(change.branch_id, 200) &&
        optionalText(change.source_branch_id, 200) &&
        integer(change.version, 1) &&
        ['draft', 'submitted', 'applied', 'rejected', 'cancelled'].includes(String(change.state)) &&
        typeof change.canReview === 'boolean' &&
        Array.isArray(change.events),
    ) ||
    !exact(data.changeQueue, ['recordId', 'state', 'query', 'page', 'pageSize', 'total', 'pages']) ||
    !Array.isArray(data.authorizedChannelNodes) ||
    !data.authorizedChannelNodes.every((value) => identifier(value)) ||
    !Array.isArray(data.channels) ||
    !data.channels.every(
      (channel) =>
        record(channel) &&
        identifier(channel.node_id) &&
        identifier(channel.room_id) &&
        text(channel.name, 120) &&
        text(channel.kind, 40) &&
        text(channel.status, 40),
    ) ||
    !identifier(data.actorId) ||
    typeof data.canManage !== 'boolean' ||
    typeof data.canManageUnits !== 'boolean'
  )
    return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  return {
    nodes: data.nodes as Row[],
    nodeChanges: data.nodeChanges as Row[],
    changeQueue: data.changeQueue as Row,
    authorizedChannelNodes: data.authorizedChannelNodes as string[],
    channels: data.channels as Row[],
    actorId: data.actorId as string,
    canManage: data.canManage as boolean,
    canManageUnits: data.canManageUnits as boolean,
  };
}

function commandData(value: unknown): Row {
  if (!exact(value, ['data']) || !record(value.data)) return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  const data = value.data;
  if (!identifier(data.id) || !integer(data.version, 1) || !text(data.state, 40))
    return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  return data;
}

function previewData(value: unknown): Row {
  if (!exact(value, ['data']) || !record(value.data)) return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  const data = value.data;
  if (!identifier(data.previewSnapshot) || !record(data.impact))
    return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  return data;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(configuration: NativeOrganizationConfiguration) {
  let base: URL;
  try {
    base = new URL(configuration.privateBaseUrl);
  } catch {
    return failure('NATIVE_ORGANIZATION_CONFIGURATION_INVALID');
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
    return failure('NATIVE_ORGANIZATION_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of configuration.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_ORGANIZATION_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      origin.port ||
      origin.username ||
      origin.password ||
      !identifier(holding.holdingId) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug) ||
      holdings.has(origin.host)
    )
      return failure('NATIVE_ORGANIZATION_CONFIGURATION_INVALID');
    holdings.set(origin.host, { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug });
  }
  return { base, holdings, timeout: configuration.timeoutMs };
}

type Transport = (
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
) => Promise<Response>;

function call(
  transport: Transport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  actor: NativeOrganizationPageContext,
  holdingSlug: string,
  path: string,
  body: string | undefined,
  idempotencyKey?: string,
  origin?: string,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(new NativeOrganizationError('NATIVE_ORGANIZATION_AUTHORITY_INVALID'));
  const headers = new Headers({ host: host!, accept: 'application/json', 'cache-control': 'no-cache' });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeOrganizationError('NATIVE_ORGANIZATION_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  for (const [key, slug] of [
    ['x-erpx-workspace-branch', actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug],
    ['x-erpx-workspace-department', actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined],
  ] as const) {
    if (slug !== undefined) {
      if (!/^[a-z0-9_-]+$/.test(slug)) return Promise.reject(new NativeOrganizationError('NATIVE_ORGANIZATION_REQUEST_INVALID'));
      headers.set(key, slug);
    }
  }
  if (actor.authority) {
    headers.set('x-erpx-context-company', actor.companyId);
    headers.set('x-erpx-context-assignment', actor.assignmentId);
    headers.set('x-erpx-context-revision', String(actor.authority.revision));
    headers.set('x-erpx-context-assignment-version', String(actor.authority.assignmentVersion));
  }
  if (body !== undefined) {
    if (new TextEncoder().encode(body).length > 64 * 1024)
      return Promise.reject(new NativeOrganizationError('NATIVE_ORGANIZATION_REQUEST_INVALID'));
    headers.set('content-type', 'application/json');
    if (!idempotencyKey || !/^[A-Za-z0-9:_.-]{8,180}$/.test(idempotencyKey))
      return Promise.reject(new NativeOrganizationError('NATIVE_ORGANIZATION_COMMAND_KEY_INVALID'));
    headers.set('idempotency-key', idempotencyKey);
    if (origin) {
      let originUrl: URL;
      try {
        originUrl = new URL(origin);
      } catch {
        return Promise.reject(new NativeOrganizationError('NATIVE_ORGANIZATION_REQUEST_INVALID'));
      }
      if (origin !== originUrl.origin) return Promise.reject(new NativeOrganizationError('NATIVE_ORGANIZATION_REQUEST_INVALID'));
      headers.set('origin', origin);
    }
  }
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(holdingSlug)}/people/organization${path}`, base);
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
    return failure('NATIVE_ORGANIZATION_UNAVAILABLE', response.status);
  }
  if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
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
        return failure('NATIVE_ORGANIZATION_RESPONSE_LIMIT');
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
    return failure('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  }
}

/** Fresh bounded socket; bypasses patched fetch/HMR and ambient proxies. */
function directHttp(
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
        headers: {
          ...Object.fromEntries(headers),
          ...(body === undefined ? {} : { 'content-length': String(new TextEncoder().encode(body).length) }),
        },
        signal,
        maxHeaderSize: 8192,
      },
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

export function createNativeOrganizationClient(
  raw: NativeOrganizationConfiguration,
  transport: Transport = directHttp,
) {
  const config = configuration(raw);
  return {
    async workspace(
      incoming: Pick<Headers, 'get'>,
      actor: NativeOrganizationPageContext,
      query: string,
      signal?: AbortSignal,
    ): Promise<NativeOrganizationWorkspace> {
      const response = await call(
        transport,
        config.base,
        config.holdings,
        incoming,
        actor,
        actor.holdingSlug,
        `?${query}`,
        undefined,
        undefined,
        undefined,
        signal,
      ).catch((error: unknown) => {
        if (error instanceof NativeOrganizationError) throw error;
        return failure('NATIVE_ORGANIZATION_UNAVAILABLE');
      });
      return workspace(await boundedJson(response));
    },
    async change(
      incoming: Pick<Headers, 'get'>,
      actor: NativeOrganizationPageContext,
      input: {
        path: string;
        body: unknown;
        idempotencyKey: string;
        origin?: string;
        preview?: boolean;
      },
    ): Promise<Row> {
      const response = await call(
        transport,
        config.base,
        config.holdings,
        incoming,
        actor,
        actor.holdingSlug,
        input.path,
        JSON.stringify(input.body),
        input.idempotencyKey,
        input.origin,
      ).catch((error: unknown) => {
        if (error instanceof NativeOrganizationError) throw error;
        return failure('NATIVE_ORGANIZATION_UNAVAILABLE');
      });
      const value = await boundedJson(response);
      return input.preview ? previewData(value) : commandData(value);
    },
  };
}

export function nativeOrganizationConfiguration(raw: string): NativeOrganizationConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeOrganizationError('NATIVE_ORGANIZATION_CONFIGURATION_INVALID');
  }
  if (!record(value) || !text(value.privateBaseUrl) || !integer(value.timeoutMs) || !Array.isArray(value.holdings))
    throw new NativeOrganizationError('NATIVE_ORGANIZATION_CONFIGURATION_INVALID');
  if (
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) && text(holding.origin) && text(holding.holdingSlug) && text(holding.holdingId),
    )
  )
    throw new NativeOrganizationError('NATIVE_ORGANIZATION_CONFIGURATION_INVALID');
  return value as unknown as NativeOrganizationConfiguration;
}

export function nativeOrganizationFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeOrganizationError)) return undefined;
  if (error.code === 'NATIVE_ORGANIZATION_RESPONSE_INVALID' || error.code === 'NATIVE_ORGANIZATION_RESPONSE_LIMIT')
    return { status: 502, code: 'NATIVE_ORGANIZATION_RESPONSE_INVALID', message: 'The organization workspace reply was invalid.' };
  if (error.code === 'NATIVE_ORGANIZATION_CONTEXT_CHANGED')
    return { status: 409, code: 'WORK_CONTEXT_CHANGED', message: 'Your work role changed. Reload and review before submitting.' };
  if (error.code === 'NATIVE_ORGANIZATION_AUTHORITY_INVALID')
    return { status: 403, code: 'FORBIDDEN', message: 'This action is unavailable in your current role.' };
  if (error.code === 'NATIVE_ORGANIZATION_REQUEST_INVALID' || error.code === 'NATIVE_ORGANIZATION_COMMAND_KEY_INVALID' || error.code === 'NATIVE_ORGANIZATION_COOKIE_LIMIT')
    return { status: 422, code: 'NATIVE_ORGANIZATION_REQUEST_INVALID', message: 'Review the organization request and submit it again.' };
  if (error.code === 'NATIVE_ORGANIZATION_CONFIGURATION_INVALID')
    return { status: 503, code: 'NATIVE_ORGANIZATION_UNAVAILABLE', message: 'The organization workspace is unavailable.' };
  switch (error.status) {
    case 401:
      return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
    case 403:
      return { status: 403, code: 'FORBIDDEN', message: 'This action is unavailable in your current role.' };
    case 404:
      return { status: 404, code: 'NOT_FOUND', message: 'Organization change not found.' };
    case 409:
      return { status: 409, code: 'CONFLICT', message: 'The organization changed. Refresh and review again.' };
    default:
      return { status: 503, code: 'NATIVE_ORGANIZATION_UNAVAILABLE', message: 'The organization workspace is unavailable.' };
  }
}
