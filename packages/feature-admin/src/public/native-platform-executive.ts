import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';
import { allowedPrivateLoopbackPort } from '@erpx/contracts/private-loopback';

/**
 * Strict private-loopback transport for the Platform Executive owner read.
 *
 * It mirrors the other native clients: exact `127.0.0.1`/`[::1]` base URL with
 * the shared private port policy, one configured Holding host, forwarded
 * session cookie, current Platform assignment and workspace selectors, a
 * bounded timeout, bounded JSON and named failures. It never mints a session,
 * Platform role or permission; the native owner re-checks the current actor.
 */

/** Rendering input from the retained Manage ERPX authorization, never a grant. */
export type NativePlatformExecutiveWorkInput = {
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

export type NativePlatformExecutivePlatformInput = {
  userId: string;
  assignmentId: string;
  role: string;
};

export type NativePlatformExecutiveConfiguration = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  production?: boolean;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
}>;

export type NativePlatformExecutiveGroup = { label: string; value: string };
export type NativePlatformExecutiveSourceReference = {
  dataset: string;
  revision: string;
  referencedAt: number;
};
export type NativePlatformExecutiveMetric = {
  id: 'failed-provisioning' | 'abnormal-platform-services';
  label: string;
  formula: string;
  dateBasis: string;
  total: string;
  rows: NativePlatformExecutiveGroup[];
  status: 'ready' | 'empty';
  sourceReferences: NativePlatformExecutiveSourceReference[];
};
export type NativePlatformExecutiveSnapshot = {
  scope: 'platform';
  referenceAt: number;
  filters: { from: string; to: string };
  sourceRevision: string;
  metrics: NativePlatformExecutiveMetric[];
};
export type NativePlatformExecutiveSourceRow =
  | { created_at: number; action: string }
  | { component: string; status: string; checked_at: number };
export type NativePlatformExecutiveSources = {
  id: string;
  name: string;
  metric: 'failed-provisioning' | 'abnormal-platform-services';
  filters: { from: string; to: string };
  rows: NativePlatformExecutiveSourceRow[];
  page: number;
  hasNext: boolean;
};
export type NativePlatformExecutiveQuery = {
  from?: string;
  to?: string;
  metric?: 'failed-provisioning' | 'abnormal-platform-services';
  group?: string;
  page?: number;
};

type Actor = NativePlatformExecutiveActor;

export type NativePlatformExecutiveActor = Readonly<{
  userId: string;
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  workAssignmentId: string;
  platformAssignmentId: string;
  platformRole: string;
  authority: { revision: number; assignmentVersion: number };
  modules: readonly string[];
  permissions: readonly string[];
  branchSlug?: string;
  departmentSlug?: string;
}>;

type Row = Record<string, unknown>;

export type NativePlatformExecutiveTransport = (
  method: 'GET',
  url: URL,
  headers: Headers,
  signal: AbortSignal,
) => Promise<Response>;

export class NativePlatformExecutiveError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Platform Executive request could not be completed.');
    this.name = 'NativePlatformExecutiveError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativePlatformExecutiveError(code, status, message);
};
const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
function fields(value: unknown, keys: readonly string[]): Row {
  if (!exact(value, keys)) return failure('NATIVE_PLATFORM_EXECUTIVE_RESPONSE_INVALID');
  return value;
}
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string => text(value, 200) && value.length > 0;
const nullableIdentifier = (value: unknown): value is string | null =>
  value === null || identifier(value);
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const boolean = (value: unknown): value is boolean => typeof value === 'boolean';
const executiveDate = (value: unknown): value is string =>
  typeof value === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(value);
const executiveMetricId = (value: unknown): value is NativePlatformExecutiveMetric['id'] =>
  value === 'failed-provisioning' || value === 'abnormal-platform-services';

const configurationNames = [
  'ERPX_NATIVE_PLATFORM_EXECUTIVE_CONFIG',
  'ERPX_NATIVE_PLATFORM_ADMIN_CONFIG',
  'ERPX_NATIVE_PLATFORM_ADMINISTRATION_CONFIG',
] as const;

/** Trusted deployment input only. The Executive name is canonical; the existing
 * Platform Administration names are accepted so one deployment config can
 * enable the owner reads together. */
function configurationValue(): string | undefined {
  for (const name of configurationNames) {
    const value = process.env[name];
    if (value !== undefined) return value;
  }
  return undefined;
}

/** Selector presence only; a configured failure never falls back to the retained reader. */
export function nativePlatformExecutiveAvailable(): boolean {
  return configurationValue() !== undefined;
}

function configuration(raw: NativePlatformExecutiveConfiguration) {
  if (
    !record(raw) ||
    !text(raw.privateBaseUrl) ||
    (raw.production !== undefined && typeof raw.production !== 'boolean') ||
    !integer(raw.timeoutMs, 250) ||
    raw.timeoutMs > 30_000 ||
    !Array.isArray(raw.holdings) ||
    raw.holdings.length < 1 ||
    raw.holdings.length > 64
  )
    return failure('NATIVE_PLATFORM_EXECUTIVE_CONFIGURATION_INVALID');
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_PLATFORM_EXECUTIVE_CONFIGURATION_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    !allowedPrivateLoopbackPort(port, raw.production === true) ||
    base.username ||
    base.password ||
    raw.privateBaseUrl !== base.origin
  )
    return failure('NATIVE_PLATFORM_EXECUTIVE_CONFIGURATION_INVALID');
  const hosts = new Set<string>();
  const slugs = new Set<string>();
  const ids = new Set<string>();
  const holdings = raw.holdings.map((holding) => {
    if (
      !record(holding) ||
      typeof holding.origin !== 'string' ||
      !identifier(holding.holdingSlug) ||
      !/^[a-z0-9_-]{1,128}$/.test(holding.holdingSlug) ||
      !identifier(holding.holdingId)
    )
      return failure('NATIVE_PLATFORM_EXECUTIVE_CONFIGURATION_INVALID');
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_PLATFORM_EXECUTIVE_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== holding.origin ||
      hosts.has(origin.host) ||
      slugs.has(holding.holdingSlug) ||
      ids.has(holding.holdingId)
    )
      return failure('NATIVE_PLATFORM_EXECUTIVE_CONFIGURATION_INVALID');
    hosts.add(origin.host);
    slugs.add(holding.holdingSlug);
    ids.add(holding.holdingId);
    return { origin: holding.origin, holdingSlug: holding.holdingSlug, holdingId: holding.holdingId };
  });
  return {
    base,
    timeout: raw.timeoutMs,
    holdings,
  };
}

export function nativePlatformExecutiveConfiguration(
  raw: string,
): NativePlatformExecutiveConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativePlatformExecutiveError('NATIVE_PLATFORM_EXECUTIVE_CONFIGURATION_INVALID');
  }
  if (!record(value)) throw new NativePlatformExecutiveError('NATIVE_PLATFORM_EXECUTIVE_CONFIGURATION_INVALID');
  configuration(value as unknown as NativePlatformExecutiveConfiguration);
  return value as unknown as NativePlatformExecutiveConfiguration;
}

function actor(
  work: NativePlatformExecutiveWorkInput,
  platform: NativePlatformExecutivePlatformInput,
): Actor {
  if (work.userId !== platform.userId) return failure('NATIVE_PLATFORM_EXECUTIVE_CONTEXT_CHANGED');
  if (work.holdingId !== 'h003' || work.holdingSlug !== 'erpx')
    return failure('NATIVE_PLATFORM_EXECUTIVE_CONTEXT_CHANGED');
  if (!['platform_admin', 'platform_auditor'].includes(platform.role))
    return failure('NATIVE_PLATFORM_EXECUTIVE_CONTEXT_CHANGED');
  if (!work.authority || !work.iam)
    return failure('NATIVE_PLATFORM_EXECUTIVE_CONTEXT_CHANGED');
  return {
    userId: work.userId,
    holdingId: work.holdingId,
    holdingSlug: work.holdingSlug,
    companyId: work.companyId,
    branchId: work.branchId,
    departmentId: work.departmentId,
    workAssignmentId: work.assignmentId,
    platformAssignmentId: platform.assignmentId,
    platformRole: platform.role,
    authority: work.authority,
    modules: work.modules,
    permissions: work.iam.permissions,
    branchSlug: work.scope.kind === 'holding' ? undefined : work.scope.branchSlug,
    departmentSlug: work.scope.kind === 'department' ? work.scope.departmentSlug : undefined,
  };
}

function context(value: unknown, expected: Actor) {
  const row = fields(value, [
    'userId',
    'holdingId',
    'holdingSlug',
    'companyId',
    'branchId',
    'departmentId',
    'workAssignmentId',
    'platformAssignmentId',
    'platformRole',
    'assignmentVersion',
    'revision',
    'modules',
    'permissions',
    'expiresAt',
  ]);
  if (
    !identifier(row.userId) ||
    !identifier(row.holdingId) ||
    !identifier(row.holdingSlug) ||
    !identifier(row.companyId) ||
    !nullableIdentifier(row.branchId) ||
    !nullableIdentifier(row.departmentId) ||
    !identifier(row.workAssignmentId) ||
    !identifier(row.platformAssignmentId) ||
    typeof row.platformRole !== 'string' ||
    !/^[a-z][a-z0-9_-]{0,39}$/.test(row.platformRole) ||
    !integer(row.assignmentVersion) ||
    !integer(row.revision) ||
    !integer(row.expiresAt, 1) ||
    !Array.isArray(row.modules) ||
    row.modules.length > 128 ||
    !row.modules.every((item) => identifier(item)) ||
    !Array.isArray(row.permissions) ||
    row.permissions.length > 512 ||
    !row.permissions.every((item) => identifier(item)) ||
    row.userId !== expected.userId ||
    row.holdingId !== expected.holdingId ||
    row.holdingSlug !== expected.holdingSlug ||
    row.companyId !== expected.companyId ||
    row.branchId !== expected.branchId ||
    row.departmentId !== expected.departmentId ||
    row.workAssignmentId !== expected.workAssignmentId ||
    row.platformAssignmentId !== expected.platformAssignmentId ||
    row.platformRole !== expected.platformRole
  )
    return failure('NATIVE_PLATFORM_EXECUTIVE_RESPONSE_INVALID');
  return row;
}

function filters(value: unknown) {
  const row = fields(value, ['from', 'to']);
  if (!executiveDate(row.from) || !executiveDate(row.to))
    return failure('NATIVE_PLATFORM_EXECUTIVE_RESPONSE_INVALID');
  return row as { from: string; to: string };
}

function group(value: unknown): NativePlatformExecutiveGroup {
  const row = fields(value, ['label', 'value']);
  if (!text(row.label, 250) || typeof row.value !== 'string' || !/^-?\d+$/.test(row.value))
    return failure('NATIVE_PLATFORM_EXECUTIVE_RESPONSE_INVALID');
  return row as NativePlatformExecutiveGroup;
}

function reference(value: unknown): NativePlatformExecutiveSourceReference {
  const row = fields(value, ['dataset', 'revision', 'referencedAt']);
  if (!text(row.dataset, 200) || !/^[a-f0-9]{64}$/.test(String(row.revision)) || !integer(row.referencedAt))
    return failure('NATIVE_PLATFORM_EXECUTIVE_RESPONSE_INVALID');
  return row as NativePlatformExecutiveSourceReference;
}

function metric(value: unknown): NativePlatformExecutiveMetric {
  const row = fields(value, [
    'id',
    'label',
    'formula',
    'dateBasis',
    'total',
    'rows',
    'status',
    'sourceReferences',
  ]);
  if (
    !executiveMetricId(row.id) ||
    !text(row.label, 250) ||
    !text(row.formula, 2_000) ||
    !text(row.dateBasis, 500) ||
    typeof row.total !== 'string' ||
    !/^-?\d+$/.test(row.total) ||
    !Array.isArray(row.rows) ||
    row.rows.length > 10_000 ||
    !['ready', 'empty'].includes(String(row.status)) ||
    !Array.isArray(row.sourceReferences) ||
    row.sourceReferences.length < 1 ||
    row.sourceReferences.length > 8
  )
    return failure('NATIVE_PLATFORM_EXECUTIVE_RESPONSE_INVALID');
  return {
    ...(row as unknown as NativePlatformExecutiveMetric),
    rows: row.rows.map(group),
    sourceReferences: row.sourceReferences.map(reference),
  };
}

function snapshot(value: unknown, expected: Actor): NativePlatformExecutiveSnapshot {
  const row = fields(value, ['data', 'context']);
  context(row.context, expected);
  const payload = fields(row.data, ['scope', 'referenceAt', 'filters', 'sourceRevision', 'metrics']);
  if (
    payload.scope !== 'platform' ||
    !integer(payload.referenceAt) ||
    typeof payload.sourceRevision !== 'string' ||
    !/^[a-f0-9]{64}$/.test(payload.sourceRevision) ||
    !Array.isArray(payload.metrics) ||
    payload.metrics.length !== 2
  )
    return failure('NATIVE_PLATFORM_EXECUTIVE_RESPONSE_INVALID');
  const metrics = payload.metrics.map(metric);
  if (
    new Set(metrics.map((item) => item.id)).size !== 2 ||
    metrics.some((item) => item.sourceReferences.some((source) => source.revision !== payload.sourceRevision))
  )
    return failure('NATIVE_PLATFORM_EXECUTIVE_RESPONSE_INVALID');
  return {
    scope: 'platform',
    referenceAt: payload.referenceAt,
    filters: filters(payload.filters),
    sourceRevision: payload.sourceRevision,
    metrics,
  };
}

function sources(value: unknown, expected: Actor): NativePlatformExecutiveSources {
  const row = fields(value, ['data', 'context']);
  context(row.context, expected);
  const data = fields(row.data, ['id', 'name', 'metric', 'filters', 'rows', 'page', 'hasNext']);
  if (
    !text(data.id, 250) ||
    !text(data.name, 250) ||
    !executiveMetricId(data.metric) ||
    !Array.isArray(data.rows) ||
    data.rows.length > 30 ||
    !integer(data.page, 1) ||
    data.page > 100_000 ||
    !boolean(data.hasNext)
  )
    return failure('NATIVE_PLATFORM_EXECUTIVE_RESPONSE_INVALID');
  const rows = data.rows.map((item) => {
    if (data.metric === 'failed-provisioning') {
      const itemRow = fields(item, ['created_at', 'action']);
      if (!integer(itemRow.created_at) || !text(itemRow.action, 40))
        return failure('NATIVE_PLATFORM_EXECUTIVE_RESPONSE_INVALID');
      return item as NativePlatformExecutiveSourceRow;
    }
    const itemRow = fields(item, ['component', 'status', 'checked_at']);
    if (!text(itemRow.component, 200) || !text(itemRow.status, 40) || !integer(itemRow.checked_at))
      return failure('NATIVE_PLATFORM_EXECUTIVE_RESPONSE_INVALID');
    return item as NativePlatformExecutiveSourceRow;
  });
  return {
    id: data.id,
    name: data.name,
    metric: data.metric,
    filters: filters(data.filters),
    rows,
    page: data.page,
    hasNext: data.hasNext,
  };
}

function requestHeaders(
  incoming: Pick<Headers, 'get'>,
  expected: Actor,
  holdingHost: string,
): Headers {
  const headers = new Headers({
    host: holdingHost,
    accept: 'application/json',
    'cache-control': 'no-cache',
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return failure('NATIVE_PLATFORM_EXECUTIVE_COOKIE_LIMIT');
    headers.set('cookie', cookie);
  }
  const origin = incoming.get('origin');
  if (origin) headers.set('origin', origin);
  headers.set('x-erpx-platform-assignment', expected.platformAssignmentId);
  for (const [key, slug] of [
    ['x-erpx-workspace-branch', expected.branchSlug],
    ['x-erpx-workspace-department', expected.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return failure('NATIVE_PLATFORM_EXECUTIVE_REQUEST_INVALID');
      headers.set(key, slug);
    }
  }
  return headers;
}

async function boundedJson(response: Response): Promise<unknown> {
  if (response.redirected) {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_PLATFORM_EXECUTIVE_UNAVAILABLE', response.status);
  }
  if (response.status !== 200) {
    let code: string | undefined;
    let message: string | undefined;
    try {
      const body = await boundedBytes(response, 64 * 1024);
      const value: unknown = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(body));
      if (record(value) && record(value.error)) {
        if (text(value.error.code, 200)) code = value.error.code;
        if (text(value.error.message, 2_000)) message = value.error.message;
      }
    } catch {
      /* Retain the bounded status failure. */
    }
    return failure(code ?? 'NATIVE_PLATFORM_EXECUTIVE_UNAVAILABLE', response.status, message);
  }
  const length = Number(response.headers.get('content-length') ?? '0');
  if (Number.isFinite(length) && length > 2 * 1024 * 1024) {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_PLATFORM_EXECUTIVE_RESPONSE_LIMIT');
  }
  const bytes = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
  } catch {
    return failure('NATIVE_PLATFORM_EXECUTIVE_RESPONSE_INVALID');
  }
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
        return failure('NATIVE_PLATFORM_EXECUTIVE_RESPONSE_LIMIT');
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

/** Fresh bounded socket; bypasses patched fetch/HMR and ambient proxies. */
export function directHttp(
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
          const values = new Headers();
          for (let index = 0; index < incoming.rawHeaders.length; index += 2)
            values.append(incoming.rawHeaders[index]!, incoming.rawHeaders[index + 1]!);
          resolve(
            new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, {
              status: incoming.statusCode ?? 502,
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

export function createNativePlatformExecutiveClient(
  raw: NativePlatformExecutiveConfiguration,
  transport: NativePlatformExecutiveTransport = directHttp,
) {
  const config = configuration(raw);
  const call = async (
    incoming: Pick<Headers, 'get'>,
    expected: Actor,
    path: string,
  ): Promise<unknown> => {
    const holding = config.holdings.find(
      (candidate) =>
        candidate.holdingSlug === expected.holdingSlug && candidate.holdingId === expected.holdingId,
    );
    if (!holding) return failure('NATIVE_PLATFORM_EXECUTIVE_CONTEXT_CHANGED');
    const host = incoming.get('host');
    if (host !== new URL(holding.origin).host)
      return failure('NATIVE_PLATFORM_EXECUTIVE_AUTHORITY_INVALID');
    const headers = requestHeaders(incoming, expected, host);
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), config.timeout);
    try {
      return await boundedJson(
        await transport('GET', new URL(path, config.base), headers, controller.signal),
      );
    } catch (error) {
      if (error instanceof NativePlatformExecutiveError) throw error;
      throw new NativePlatformExecutiveError('NATIVE_PLATFORM_EXECUTIVE_UNAVAILABLE');
    } finally {
      clearTimeout(timer);
    }
  };
  return {
    async snapshot(
      incoming: Pick<Headers, 'get'>,
      expected: Actor,
      query: NativePlatformExecutiveQuery = {},
    ): Promise<NativePlatformExecutiveSnapshot> {
      const params = new URLSearchParams();
      if (query.from !== undefined) params.set('from', query.from);
      if (query.to !== undefined) params.set('to', query.to);
      return snapshot(
        await call(incoming, expected, `/api/v1/platform/executive${params.size ? `?${params}` : ''}`),
        expected,
      );
    },
    async sources(
      incoming: Pick<Headers, 'get'>,
      expected: Actor,
      query: Required<Pick<NativePlatformExecutiveQuery, 'metric' | 'group' | 'from' | 'to'>> &
        Pick<NativePlatformExecutiveQuery, 'page'>,
    ): Promise<NativePlatformExecutiveSources> {
      if (!executiveMetricId(query.metric) || !text(query.group, 250) || query.group.length === 0)
        return failure('NATIVE_PLATFORM_EXECUTIVE_REQUEST_INVALID');
      if (!executiveDate(query.from) || !executiveDate(query.to))
        return failure('NATIVE_PLATFORM_EXECUTIVE_REQUEST_INVALID');
      const params = new URLSearchParams({
        metric: query.metric,
        group: query.group,
        from: query.from,
        to: query.to,
      });
      if (query.page !== undefined) {
        if (!integer(query.page, 1) || query.page > 100_000)
          return failure('NATIVE_PLATFORM_EXECUTIVE_REQUEST_INVALID');
        params.set('page', String(query.page));
      }
      return sources(
        await call(incoming, expected, `/api/v1/platform/executive?${params}`),
        expected,
      );
    },
  };
}

function client() {
  const raw = configurationValue();
  if (raw === undefined) return undefined;
  return createNativePlatformExecutiveClient(nativePlatformExecutiveConfiguration(raw));
}

/** Native snapshot. Unset configuration returns undefined so the caller keeps
 * the retained reader; a configured failure is surfaced. */
export async function nativePlatformExecutiveSnapshot(
  work: NativePlatformExecutiveWorkInput,
  platform: NativePlatformExecutivePlatformInput,
  incoming: Pick<Headers, 'get'>,
  query?: NativePlatformExecutiveQuery,
): Promise<NativePlatformExecutiveSnapshot | undefined> {
  const native = client();
  if (!native) return undefined;
  return native.snapshot(incoming, actor(work, platform), query);
}

export async function nativePlatformExecutiveSources(
  work: NativePlatformExecutiveWorkInput,
  platform: NativePlatformExecutivePlatformInput,
  incoming: Pick<Headers, 'get'>,
  query: Required<Pick<NativePlatformExecutiveQuery, 'metric' | 'group' | 'from' | 'to'>> &
    Pick<NativePlatformExecutiveQuery, 'page'>,
): Promise<NativePlatformExecutiveSources | undefined> {
  const native = client();
  if (!native) return undefined;
  return native.sources(incoming, actor(work, platform), query);
}

/**
 * Keep the explicit retained implementation only while the native transport is
 * unset. A configured failure is surfaced, so this selector can never replace
 * it with retained data.
 */
export async function nativePlatformExecutiveOrRetained<T>(
  native: () => Promise<T | undefined>,
  retained: () => Promise<T>,
): Promise<T> {
  const result = await native();
  return result ?? (await retained());
}

/** Bounded failure policy for the retained route error handling. */
export function nativePlatformExecutiveFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativePlatformExecutiveError)) return undefined;
  if (
    error.code === 'NATIVE_PLATFORM_EXECUTIVE_RESPONSE_INVALID' ||
    error.code === 'NATIVE_PLATFORM_EXECUTIVE_RESPONSE_LIMIT'
  )
    return {
      status: 502,
      code: error.code,
      message: 'The Platform Executive reply was invalid.',
    };
  if (error.code === 'NATIVE_PLATFORM_EXECUTIVE_CONTEXT_CHANGED')
    return {
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before retrying.',
    };
  if (error.code === 'NATIVE_PLATFORM_EXECUTIVE_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_PLATFORM_EXECUTIVE_UNAVAILABLE',
      message: 'Native Platform Executive access is unavailable.',
    };
  if (
    error.code === 'NATIVE_PLATFORM_EXECUTIVE_REQUEST_INVALID' ||
    error.code === 'NATIVE_PLATFORM_EXECUTIVE_COOKIE_LIMIT'
  )
    return {
      status: 422,
      code: error.code,
      message: 'Review the Platform Executive request.',
    };
  if (error.code === 'NATIVE_PLATFORM_EXECUTIVE_AUTHORITY_INVALID')
    return {
      status: 403,
      code: 'FORBIDDEN',
      message: 'The Platform Executive authority is unavailable.',
    };
  if (error.status !== undefined && error.status >= 400 && error.status <= 599)
    return { status: error.status, code: error.code, message: error.message };
  return {
    status: 503,
    code: 'NATIVE_PLATFORM_EXECUTIVE_UNAVAILABLE',
    message: 'The native Platform Executive service is unavailable.',
  };
}
