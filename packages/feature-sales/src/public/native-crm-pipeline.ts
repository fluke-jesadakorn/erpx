import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';

/** Rendering input from retained page authorization, never a grant. */
export type NativeCrmPipelinePageContext = {
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

export type NativeCrmPipelineConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;
type Context = Row & { modules: string[]; permissions: string[] };

export type NativeCrmPipelineReadInput = Readonly<{
  holdingSlug: string;
  search?: string;
  stage?: string;
  cursor?: string;
  branchSlug?: string;
  departmentSlug?: string;
  expected?: Readonly<{
    companyId: string;
    assignmentId: string;
    revision: number;
    assignmentVersion: number;
  }>;
}>;

export type NativeCrmPipelineStage =
  | 'discovery'
  | 'proposal'
  | 'negotiation'
  | 'won'
  | 'lost';
export type NativeCrmPipelineStatus = 'open' | 'qualified' | 'disqualified';

export type NativeCrmPipelineRow = Readonly<{
  id: string;
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  title: string;
  contact_name: string;
  status: NativeCrmPipelineStatus;
  stage: NativeCrmPipelineStage | null;
  opportunity_id: string | null;
  expected_minor: number | null;
  currency: string | null;
  close_date: string | null;
  due_date: string;
  owner_id: string;
  owner_name: string | null;
  customer_id: string | null;
  customer_name: string | null;
  version: number;
  opportunity_version: number | null;
  updated_at: number;
}>;

export type NativeCrmForecastTotal = Readonly<{
  unweightedMinor: string;
  weightedMinor: string;
  wonMinor: string;
}>;

export type NativeCrmPurchaseControls = Readonly<{
  newWork: boolean;
  settleExisting: boolean;
  history: boolean;
  revision: string;
}>;

export type NativeCrmPipelinePage = Readonly<{
  items: NativeCrmPipelineRow[];
  forecast: Record<string, NativeCrmForecastTotal>;
  canCreate: boolean;
  purchase: NativeCrmPurchaseControls | null;
  nextCursor: string | null;
  previousCursor: null;
  hasMore: boolean;
}>;

export type NativeCrmPipelineTransport = (
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeCrmPipelineError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native CRM request could not be completed.');
    this.name = 'NativeCrmPipelineError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeCrmPipelineError(code, status, message);
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
const optionalText = (value: unknown, max = 4096) =>
  value === null || text(value, max);
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const texts = (value: unknown, max = 256): value is string[] =>
  Array.isArray(value) && value.length <= max && value.every((item) => text(item, 256));
const sameSet = (left: readonly string[], right: readonly string[]) => {
  const a = new Set(left);
  const b = new Set(right);
  return a.size === b.size && [...a].every((value) => b.has(value));
};
const stages = new Set<NativeCrmPipelineStage>([
  'discovery',
  'proposal',
  'negotiation',
  'won',
  'lost',
]);
const statuses = new Set<NativeCrmPipelineStatus>([
  'open',
  'qualified',
  'disqualified',
]);
const stageFilters = new Set([
  'open',
  'qualified',
  'disqualified',
  'discovery',
  'proposal',
  'negotiation',
  'won',
  'lost',
]);

const date = (value: unknown): value is string => {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const [year, month, day] = value.split('-').map(Number) as [number, number, number];
  if (month < 1 || month > 12 || day < 1) return false;
  const leap = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0);
  const days = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][month - 1]!;
  return day <= days;
};
const amount = (value: unknown): value is string =>
  typeof value === 'string' &&
  value.length <= 20 &&
  /^(0|[1-9]\d*)$/.test(value);

const row = (
  value: unknown,
  input: NativeCrmPipelineReadInput,
  actor: NativeCrmPipelinePageContext,
): value is NativeCrmPipelineRow => {
  if (
    !exact(value, [
      'id', 'company_id', 'branch_id', 'department_id', 'title', 'contact_name',
      'status', 'stage', 'opportunity_id', 'expected_minor', 'currency',
      'close_date', 'due_date', 'owner_id', 'owner_name', 'customer_id',
      'customer_name', 'version', 'opportunity_version', 'updated_at',
    ])
  )
    return false;
  const stage = value.stage;
  const opportunity = value.opportunity_id !== null;
  if (
    !identifier(value.id) ||
    !identifier(value.company_id) ||
    !identifier(value.branch_id) ||
    (value.department_id !== null && !identifier(value.department_id)) ||
    !text(value.title, 160) ||
    value.title.length === 0 ||
    !text(value.contact_name, 300) ||
    value.contact_name.length === 0 ||
    typeof value.status !== 'string' ||
    !statuses.has(value.status as NativeCrmPipelineStatus) ||
    !date(value.due_date) ||
    !identifier(value.owner_id) ||
    !optionalText(value.owner_name, 300) ||
    !integer(value.version, 1) ||
    !integer(value.updated_at)
  )
    return false;
  if (opportunity) {
    if (
      typeof stage !== 'string' ||
      !stages.has(stage as NativeCrmPipelineStage) ||
      !identifier(value.opportunity_id) ||
      !identifier(value.customer_id) ||
      !optionalText(value.customer_name, 300) ||
      !integer(value.expected_minor) ||
      typeof value.currency !== 'string' ||
      !/^[A-Z]{3}$/.test(value.currency) ||
      !date(value.close_date) ||
      !integer(value.opportunity_version, 1) ||
      value.status !== 'qualified'
    )
      return false;
  } else if (
    stage !== null ||
    value.opportunity_id !== null ||
    value.expected_minor !== null ||
    value.currency !== null ||
    value.close_date !== null ||
    value.customer_id !== null ||
    value.customer_name !== null ||
    value.opportunity_version !== null
  ) {
    return false;
  }
  if (value.status === 'qualified' && !opportunity) return false;
  if (
    value.company_id !== actor.companyId ||
    (actor.branchId !== null && value.branch_id !== actor.branchId) ||
    (actor.departmentId !== null && value.department_id !== actor.departmentId)
  )
    return false;
  if (
    input.stage !== undefined &&
    input.stage !== '' &&
    (value.stage ?? value.status) !== input.stage
  )
    return false;
  return true;
};

function forecast(value: unknown): Record<string, NativeCrmForecastTotal> {
  if (!record(value) || Object.keys(value).length > 32)
    return failure('NATIVE_CRM_PIPELINE_RESPONSE_INVALID');
  const entries: [string, NativeCrmForecastTotal][] = [];
  for (const [currency, total] of Object.entries(value)) {
    if (
      !/^[A-Z]{3}$/.test(currency) ||
      !exact(total, ['unweightedMinor', 'weightedMinor', 'wonMinor']) ||
      !amount(total.unweightedMinor) ||
      !amount(total.weightedMinor) ||
      !amount(total.wonMinor)
    )
      return failure('NATIVE_CRM_PIPELINE_RESPONSE_INVALID');
    entries.push([
      currency,
      {
        unweightedMinor: total.unweightedMinor,
        weightedMinor: total.weightedMinor,
        wonMinor: total.wonMinor,
      },
    ]);
  }
  return Object.fromEntries(entries);
}

function purchase(value: unknown): NativeCrmPurchaseControls | null {
  if (value === null) return null;
  if (
    !exact(value, ['newWork', 'settleExisting', 'history', 'revision']) ||
    typeof value.newWork !== 'boolean' ||
    typeof value.settleExisting !== 'boolean' ||
    typeof value.history !== 'boolean' ||
    !text(value.revision, 200) ||
    value.revision.length === 0
  )
    return failure('NATIVE_CRM_PIPELINE_RESPONSE_INVALID');
  return {
    newWork: value.newWork,
    settleExisting: value.settleExisting,
    history: value.history,
    revision: value.revision,
  };
}

function pageData(
  value: unknown,
  input: NativeCrmPipelineReadInput,
  actor: NativeCrmPipelinePageContext,
): NativeCrmPipelinePage {
  if (
    !exact(value, ['items', 'forecast', 'canCreate', 'purchase', 'nextCursor', 'hasMore']) ||
    !Array.isArray(value.items) ||
    typeof value.canCreate !== 'boolean' ||
    typeof value.hasMore !== 'boolean'
  )
    return failure('NATIVE_CRM_PIPELINE_RESPONSE_INVALID');
  if (value.nextCursor !== null && (!text(value.nextCursor, 4096) || value.nextCursor.length === 0))
    return failure('NATIVE_CRM_PIPELINE_RESPONSE_INVALID');
  if (value.hasMore !== (value.nextCursor !== null))
    return failure('NATIVE_CRM_PIPELINE_RESPONSE_INVALID');
  if (value.items.length > 30) return failure('NATIVE_CRM_PIPELINE_RESPONSE_INVALID');
  const seen = new Set<string>();
  let previous: number | null = null;
  for (const item of value.items) {
    if (!row(item, input, actor)) return failure('NATIVE_CRM_PIPELINE_RESPONSE_INVALID');
    if (seen.has(item.id)) return failure('NATIVE_CRM_PIPELINE_RESPONSE_INVALID');
    if (previous !== null && item.updated_at >= previous)
      return failure('NATIVE_CRM_PIPELINE_RESPONSE_INVALID');
    previous = item.updated_at;
    seen.add(item.id);
  }
  return {
    items: value.items as NativeCrmPipelineRow[],
    forecast: forecast(value.forecast),
    canCreate: value.canCreate,
    purchase: purchase(value.purchase),
    nextCursor: value.nextCursor as string | null,
    previousCursor: null,
    hasMore: value.hasMore,
  };
}

function context(
  value: unknown,
  actor: NativeCrmPipelinePageContext,
  input: NativeCrmPipelineReadInput,
): Context {
  if (
    !exact(value, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug',
      'departmentSlug', 'assignmentId', 'assignmentVersion', 'revision', 'role',
      'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ])
  )
    return failure('NATIVE_CRM_PIPELINE_RESPONSE_INVALID');
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
    return failure('NATIVE_CRM_PIPELINE_CONTEXT_CHANGED');
  if (
    input.expected &&
    (parsed.companyId !== input.expected.companyId ||
      parsed.assignmentId !== input.expected.assignmentId ||
      parsed.revision !== input.expected.revision ||
      parsed.assignmentVersion !== input.expected.assignmentVersion)
  )
    return failure('NATIVE_CRM_PIPELINE_CONTEXT_CHANGED');
  return parsed;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativeCrmPipelineConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_CRM_PIPELINE_CONFIGURATION_INVALID');
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
    return failure('NATIVE_CRM_PIPELINE_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_CRM_PIPELINE_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      (origin.port !== '' &&
        !(
          origin.port.length >= 1 &&
          origin.port.length <= 5 &&
          [...origin.port].every(character => character >= '0' && character <= '9')
        )) ||
      origin.username ||
      origin.password ||
      !identifier(holding.holdingId) ||
      !text(holding.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug) ||
      holdings.has(origin.host)
    )
      return failure('NATIVE_CRM_PIPELINE_CONFIGURATION_INVALID');
    holdings.set(origin.host, { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativeCrmPipelineTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  input: NativeCrmPipelineReadInput,
  actor: NativeCrmPipelinePageContext,
  timeout: number,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== input.holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(new NativeCrmPipelineError('NATIVE_CRM_PIPELINE_AUTHORITY_INVALID'));
  const headers = new Headers({ host: host!, accept: 'application/json', 'cache-control': 'no-cache' });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeCrmPipelineError('NATIVE_CRM_PIPELINE_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  for (const [key, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return Promise.reject(new NativeCrmPipelineError('NATIVE_CRM_PIPELINE_REQUEST_INVALID'));
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
      return Promise.reject(new NativeCrmPipelineError('NATIVE_CRM_PIPELINE_REQUEST_INVALID'));
    headers.set('x-erpx-context-company', input.expected.companyId);
    headers.set('x-erpx-context-assignment', input.expected.assignmentId);
    headers.set('x-erpx-context-revision', String(input.expected.revision));
    headers.set('x-erpx-context-assignment-version', String(input.expected.assignmentVersion));
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/crm/leads`,
    base,
  );
  if (input.search !== undefined) url.searchParams.set('q', input.search);
  if (input.stage !== undefined) url.searchParams.set('stage', input.stage);
  if (input.cursor !== undefined) url.searchParams.set('cursor', input.cursor);
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, timeout);
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
        return failure('NATIVE_CRM_PIPELINE_RESPONSE_LIMIT');
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
    return failure('NATIVE_CRM_PIPELINE_UNAVAILABLE', response.status);
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
    return failure(code ?? 'NATIVE_CRM_PIPELINE_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_CRM_PIPELINE_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_CRM_PIPELINE_RESPONSE_INVALID');
  }
}

export function nativeCrmPipelineAvailable(): boolean {
  return process.env.ERPX_NATIVE_CRM_PIPELINE_CONFIG !== undefined;
}

export function nativeCrmPipelineConfiguration(raw: string): NativeCrmPipelineConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeCrmPipelineError('NATIVE_CRM_PIPELINE_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    !Array.isArray(value.holdings)
  )
    return failure('NATIVE_CRM_PIPELINE_CONFIGURATION_INVALID');
  if (
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) &&
        text(holding.origin) &&
        text(holding.holdingSlug) &&
        text(holding.holdingId),
    )
  )
    return failure('NATIVE_CRM_PIPELINE_CONFIGURATION_INVALID');
  return value as unknown as NativeCrmPipelineConfiguration;
}

export function createNativeCrmPipelineClient(
  raw: NativeCrmPipelineConfiguration,
  transport: NativeCrmPipelineTransport = directHttp,
) {
  const config = configuration(raw);
  const pipeline = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeCrmPipelinePageContext,
    input: NativeCrmPipelineReadInput,
    signal?: AbortSignal,
  ): Promise<NativeCrmPipelinePage> => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return failure('NATIVE_CRM_PIPELINE_CONTEXT_CHANGED');
    if (
      (input.search !== undefined &&
        (input.search.length > 200 || /[\u0000-\u001f\u007f]/.test(input.search))) ||
      (input.stage !== undefined &&
        input.stage !== '' &&
        !stageFilters.has(input.stage)) ||
      (input.cursor !== undefined &&
        (input.cursor.length > 4096 || input.cursor.length === 0))
    )
      return failure('NATIVE_CRM_PIPELINE_REQUEST_INVALID');
    const response = await transportCall(
      transport,
      config.base,
      config.holdings,
      incoming,
      input,
      actor,
      config.timeout,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeCrmPipelineError) throw error;
      return failure('NATIVE_CRM_PIPELINE_UNAVAILABLE');
    });
    const value = await boundedJson(response);
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_CRM_PIPELINE_RESPONSE_INVALID');
    context(value.context, actor, input);
    return pageData(value.data, input, actor);
  };
  return { pipeline };
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
    outgoing.end(body);
  });
}
