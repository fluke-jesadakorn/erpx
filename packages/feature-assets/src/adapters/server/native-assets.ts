import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';
import type {
  NativeAsset,
  NativeAssetContext,
  NativeAssetDetail,
  NativeAssetEvent,
  NativeAssetFinance,
  NativeAssetMaintenance,
  NativeAssetWorkspace,
} from '../../lib/asset-view';

/** Rendering input from the retained page authorization, never a grant. */
export type NativeAssetsPageContext = {
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

export type NativeAssetsConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;
type Context = Row & { modules: string[]; permissions: string[] };
export type NativeAssetsReadInput = Readonly<{
  holdingSlug: string;
  id?: string;
  branchSlug?: string;
  departmentSlug?: string;
  expected?: Readonly<{
    companyId: string;
    assignmentId: string;
    revision: number;
    assignmentVersion: number;
  }>;
}>;
export type NativeAssetsCommandInput = Readonly<{
  holdingSlug: string;
  branchSlug?: string;
  departmentSlug?: string;
  expected?: NativeAssetsReadInput['expected'];
  body: unknown;
  idempotencyKey: string;
  origin?: string;
}>;
export type NativeAssetsCommandResult = Readonly<{ id: string; version: number; status: string }>;
export type NativeAssetsTransport = (
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeAssetsError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Assets request could not be completed.');
    this.name = 'NativeAssetsError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeAssetsError(code, status, message);
};
const record = (value: unknown): value is Row => value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) && Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string => text(value, 200) && value.length > 0;
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const safeInteger = (value: unknown): value is number => integer(value, Number.MIN_SAFE_INTEGER);
const optionalText = (value: unknown, max = 4096) => value === null || text(value, max);
const optionalInteger = (value: unknown) => value === null || safeInteger(value);
const texts = (value: unknown, max = 256): value is string[] =>
  Array.isArray(value) && value.length <= max && value.every((item) => text(item, 256));
const sameSet = (left: readonly string[], right: readonly string[]) => {
  const a = new Set(left);
  const b = new Set(right);
  return a.size === b.size && [...a].every((value) => b.has(value));
};
const conditions = new Set(['good', 'degraded', 'unsafe']);
const lifecycles = new Set(['registered', 'in_service', 'maintenance', 'retirement_pending', 'retired']);

const assetRow = (row: Row) =>
  exact(row, [
    'id', 'company_id', 'branch_id', 'department_id', 'code', 'name', 'category', 'currency',
    'cost_minor', 'acquired_date', 'warranty_end', 'finance_asset_id', 'finance_asset_version',
    'condition', 'lifecycle_status', 'placement', 'custodian_id', 'custodian_name',
    'retirement_reason', 'version', 'created_by', 'created_at', 'updated_at', 'can_review',
  ]) &&
  identifier(row.id) &&
  text(row.company_id, 200) &&
  identifier(row.branch_id) &&
  optionalText(row.department_id, 200) &&
  text(row.code, 200) &&
  text(row.name, 400) &&
  text(row.category, 200) &&
  text(row.currency, 3) &&
  integer(row.cost_minor) &&
  optionalText(row.acquired_date, 32) &&
  optionalText(row.warranty_end, 32) &&
  optionalText(row.finance_asset_id, 200) &&
  optionalInteger(row.finance_asset_version) &&
  conditions.has(String(row.condition)) &&
  lifecycles.has(String(row.lifecycle_status)) &&
  optionalText(row.placement, 400) &&
  optionalText(row.custodian_id, 200) &&
  optionalText(row.custodian_name, 400) &&
  optionalText(row.retirement_reason, 4000) &&
  integer(row.version, 1) &&
  identifier(row.created_by) &&
  integer(row.created_at) &&
  integer(row.updated_at) &&
  typeof row.can_review === 'boolean';

const eventRow = (row: Row) =>
  exact(row, ['id', 'asset_id', 'version', 'action', 'actor_id', 'reason', 'condition', 'lifecycle_status', 'placement', 'custodian_id', 'created_at']) &&
  identifier(row.id) &&
  identifier(row.asset_id) &&
  integer(row.version, 1) &&
  identifier(row.action) &&
  identifier(row.actor_id) &&
  text(row.reason, 4000) &&
  conditions.has(String(row.condition)) &&
  lifecycles.has(String(row.lifecycle_status)) &&
  optionalText(row.placement, 400) &&
  optionalText(row.custodian_id, 200) &&
  integer(row.created_at);

const financeRow = (row: Row) =>
  exact(row, ['id', 'asset_number', 'name', 'status', 'currency', 'cost_minor', 'accumulated_depreciation_minor', 'version']) &&
  identifier(row.id) &&
  text(row.asset_number, 200) &&
  optionalText(row.name, 400) &&
  identifier(row.status) &&
  text(row.currency, 3) &&
  integer(row.cost_minor) &&
  optionalInteger(row.accumulated_depreciation_minor) &&
  integer(row.version, 1);

const candidateRow = (row: Row) =>
  exact(row, ['id', 'asset_number', 'name', 'currency', 'cost_minor', 'version', 'status']) &&
  identifier(row.id) &&
  text(row.asset_number, 200) &&
  optionalText(row.name, 400) &&
  text(row.currency, 3) &&
  integer(row.cost_minor) &&
  integer(row.version, 1) &&
  identifier(row.status);

const personRow = (row: Row) =>
  exact(row, ['id', 'name']) && identifier(row.id) && text(row.name, 400);
const nodeRow = (row: Row) =>
  exact(row, ['id', 'kind', 'display_name', 'branch_id']) &&
  identifier(row.id) &&
  identifier(row.kind) &&
  text(row.display_name, 400) &&
  optionalText(row.branch_id, 200);
const maintenanceRow = (row: Row) =>
  exact(row, ['id', 'title', 'status', 'priority', 'owner_id', 'scheduled_at', 'version']) &&
  identifier(row.id) &&
  text(row.title, 400) &&
  identifier(row.status) &&
  identifier(row.priority) &&
  identifier(row.owner_id) &&
  optionalInteger(row.scheduled_at) &&
  integer(row.version, 1);

function context(value: unknown, actor: NativeAssetsPageContext, input: NativeAssetsReadInput): NativeAssetContext {
  if (
    !exact(value, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug',
      'departmentSlug', 'assignmentId', 'assignmentVersion', 'revision', 'role',
      'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ])
  )
    return failure('NATIVE_ASSETS_RESPONSE_INVALID');
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
    return failure('NATIVE_ASSETS_CONTEXT_CHANGED');
  if (
    input.expected &&
    (parsed.companyId !== input.expected.companyId ||
      parsed.assignmentId !== input.expected.assignmentId ||
      parsed.revision !== input.expected.revision ||
      parsed.assignmentVersion !== input.expected.assignmentVersion)
  )
    return failure('NATIVE_ASSETS_CONTEXT_CHANGED');
  return parsed as unknown as NativeAssetContext;
}

function workspaceData(value: unknown): NativeAssetWorkspace {
  if (!exact(value, ['items', 'people', 'nodes', 'financeAssets', 'canLinkFinance', 'canCreate', 'actorId']))
    return failure('NATIVE_ASSETS_RESPONSE_INVALID');
  if (
    !Array.isArray(value.items) || value.items.length > 2000 || !value.items.every((row) => record(row) && assetRow(row)) ||
    !Array.isArray(value.people) || value.people.length > 1000 || !value.people.every((row) => record(row) && personRow(row)) ||
    !Array.isArray(value.nodes) || value.nodes.length > 1000 || !value.nodes.every((row) => record(row) && nodeRow(row)) ||
    !Array.isArray(value.financeAssets) || value.financeAssets.length > 1000 || !value.financeAssets.every((row) => record(row) && candidateRow(row)) ||
    typeof value.canLinkFinance !== 'boolean' ||
    typeof value.canCreate !== 'boolean' ||
    !identifier(value.actorId)
  )
    return failure('NATIVE_ASSETS_RESPONSE_INVALID');
  return value as unknown as NativeAssetWorkspace;
}

function workspace(value: unknown, actor: NativeAssetsPageContext, input: NativeAssetsReadInput): NativeAssetWorkspace {
  if (!exact(value, ['data', 'context'])) return failure('NATIVE_ASSETS_RESPONSE_INVALID');
  context(value.context, actor, input);
  return workspaceData(value.data);
}

function detail(value: unknown, actor: NativeAssetsPageContext, input: NativeAssetsReadInput): NativeAssetDetail {
  if (!exact(value, ['data', 'context'])) return failure('NATIVE_ASSETS_RESPONSE_INVALID');
  context(value.context, actor, input);
  if (!exact(value.data, ['items', 'people', 'nodes', 'financeAssets', 'canLinkFinance', 'canCreate', 'actorId', 'asset', 'finance', 'history', 'maintenance']))
    return failure('NATIVE_ASSETS_RESPONSE_INVALID');
  const data = value.data;
  const workspace = workspaceData({
    items: data.items,
    people: data.people,
    nodes: data.nodes,
    financeAssets: data.financeAssets,
    canLinkFinance: data.canLinkFinance,
    canCreate: data.canCreate,
    actorId: data.actorId,
  });
  if (!record(data.asset) || !assetRow(data.asset)) return failure('NATIVE_ASSETS_RESPONSE_INVALID');
  if (!Array.isArray(data.history) || data.history.length > 500 || !data.history.every((row) => record(row) && eventRow(row)))
    return failure('NATIVE_ASSETS_RESPONSE_INVALID');
  if (!Array.isArray(data.maintenance) || data.maintenance.length > 500 || !data.maintenance.every((row) => record(row) && maintenanceRow(row)))
    return failure('NATIVE_ASSETS_RESPONSE_INVALID');
  if (data.finance !== null && (!record(data.finance) || !financeRow(data.finance)))
    return failure('NATIVE_ASSETS_RESPONSE_INVALID');
  return {
    ...workspace,
    asset: data.asset as unknown as NativeAsset,
    finance: data.finance as NativeAssetFinance | null,
    history: data.history as unknown as NativeAssetEvent[],
    maintenance: data.maintenance as unknown as NativeAssetMaintenance[],
  };
}

function commandResult(value: unknown, actor: NativeAssetsPageContext, input: NativeAssetsReadInput): NativeAssetsCommandResult {
  if (!exact(value, ['data', 'context'])) return failure('NATIVE_ASSETS_RESPONSE_INVALID');
  context(value.context, actor, input);
  if (!exact(value.data, ['id', 'version', 'status']) || !identifier(value.data.id) || !integer(value.data.version, 1) || !identifier(value.data.status))
    return failure('NATIVE_ASSETS_RESPONSE_INVALID');
  return value.data as unknown as NativeAssetsCommandResult;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativeAssetsConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_ASSETS_CONFIGURATION_INVALID');
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
    return failure('NATIVE_ASSETS_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_ASSETS_CONFIGURATION_INVALID');
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
      return failure('NATIVE_ASSETS_CONFIGURATION_INVALID');
    holdings.set(origin.host, { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativeAssetsTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  holdingSlug: string,
  input: NativeAssetsReadInput,
  body: string | undefined,
  idempotencyKey?: string,
  origin?: string,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== holdingSlug)
    return Promise.reject(new NativeAssetsError('NATIVE_ASSETS_AUTHORITY_INVALID'));
  const headers = new Headers({ host: host!, accept: 'application/json', 'cache-control': 'no-cache' });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeAssetsError('NATIVE_ASSETS_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  for (const [key, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return Promise.reject(new NativeAssetsError('NATIVE_ASSETS_REQUEST_INVALID'));
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
      return Promise.reject(new NativeAssetsError('NATIVE_ASSETS_REQUEST_INVALID'));
    headers.set('x-erpx-context-company', input.expected.companyId);
    headers.set('x-erpx-context-assignment', input.expected.assignmentId);
    headers.set('x-erpx-context-revision', String(input.expected.revision));
    headers.set('x-erpx-context-assignment-version', String(input.expected.assignmentVersion));
  }
  if (body !== undefined) {
    if (new TextEncoder().encode(body).length > 64 * 1024)
      return Promise.reject(new NativeAssetsError('NATIVE_ASSETS_REQUEST_INVALID'));
    headers.set('content-type', 'application/json');
    if (!idempotencyKey || !/^[A-Za-z0-9:_.-]{8,180}$/.test(idempotencyKey))
      return Promise.reject(new NativeAssetsError('NATIVE_ASSETS_COMMAND_KEY_INVALID'));
    headers.set('idempotency-key', idempotencyKey);
    if (origin) {
      let originUrl: URL;
      try {
        originUrl = new URL(origin);
      } catch {
        return Promise.reject(new NativeAssetsError('NATIVE_ASSETS_REQUEST_INVALID'));
      }
      if (origin !== originUrl.origin)
        return Promise.reject(new NativeAssetsError('NATIVE_ASSETS_REQUEST_INVALID'));
      headers.set('origin', origin);
    }
  }
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(holdingSlug)}/assets`, base);
  if (input.id !== undefined) url.searchParams.set('id', input.id);
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
        return failure('NATIVE_ASSETS_RESPONSE_LIMIT');
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
    return failure('NATIVE_ASSETS_UNAVAILABLE', response.status);
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
    return failure(code ?? 'NATIVE_ASSETS_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_ASSETS_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_ASSETS_RESPONSE_INVALID');
  }
}

export function createNativeAssetsClient(
  raw: NativeAssetsConfiguration,
  transport: NativeAssetsTransport = directHttp,
) {
  const config = configuration(raw);
  const request = (
    incoming: Pick<Headers, 'get'>,
    actor: NativeAssetsPageContext,
    input: NativeAssetsReadInput,
    body: string | undefined,
    idempotencyKey?: string,
    origin?: string,
    signal?: AbortSignal,
  ) => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return Promise.reject(new NativeAssetsError('NATIVE_ASSETS_CONTEXT_CHANGED'));
    return transportCall(
      transport,
      config.base,
      config.holdings,
      incoming,
      input.holdingSlug,
      input,
      body,
      idempotencyKey,
      origin,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeAssetsError) throw error;
      return failure('NATIVE_ASSETS_UNAVAILABLE');
    });
  };
  const read = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeAssetsPageContext,
    input: NativeAssetsReadInput,
    signal?: AbortSignal,
  ): Promise<NativeAssetWorkspace> =>
    workspace(await boundedJson(await request(incoming, actor, input, undefined, undefined, undefined, signal)), actor, input);
  const detailRead = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeAssetsPageContext,
    input: NativeAssetsReadInput & { id: string },
    signal?: AbortSignal,
  ): Promise<NativeAssetDetail> =>
    detail(await boundedJson(await request(incoming, actor, input, undefined, undefined, undefined, signal)), actor, input);
  const command = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeAssetsPageContext,
    input: NativeAssetsCommandInput,
  ): Promise<NativeAssetsCommandResult> => {
    const body = JSON.stringify(input.body);
    return commandResult(
      await boundedJson(
        await request(incoming, actor, input, body, input.idempotencyKey, input.origin),
      ),
      actor,
      input,
    );
  };
  return { read, detail: detailRead, command };
}

export function nativeAssetsConfiguration(raw: string): NativeAssetsConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeAssetsError('NATIVE_ASSETS_CONFIGURATION_INVALID');
  }
  if (!record(value) || !text(value.privateBaseUrl) || !integer(value.timeoutMs) || !Array.isArray(value.holdings))
    throw new NativeAssetsError('NATIVE_ASSETS_CONFIGURATION_INVALID');
  if (
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) && text(holding.origin) && text(holding.holdingSlug) && text(holding.holdingId),
    )
  )
    throw new NativeAssetsError('NATIVE_ASSETS_CONFIGURATION_INVALID');
  return value as unknown as NativeAssetsConfiguration;
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
            const values = new Headers();
            for (let index = 0; index < incoming.rawHeaders.length; index += 2)
              values.append(incoming.rawHeaders[index]!, incoming.rawHeaders[index + 1]!);
            resolve(
              new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, {
                status: status >= 200 && status <= 599 ? status : 502,
                headers: values,
              }),
            );
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
