import 'server-only';
import { Agent, request } from 'node:http';

type Row = Record<string, unknown>;
type Context = Row & { modules: string[]; permissions: string[] };

export type NativeMarketingLeadActor = {
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

export type NativeMarketingLeadPageContext = {
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

export type NativeMarketingLeadCandidate = Readonly<{
  id: string;
  company_id: string;
  branch_id: string;
  department_id: string | null;
  form_id: string;
  form_version: number;
  form_name: string;
  contact_name: string;
  contact_detail: string;
  consent_evidence: string;
  attribution_json: string;
  brief: string;
  status: string;
  crm_lead_id: string | null;
  created_by: string;
  reviewed_by: string | null;
  version: number;
  created_at: number;
  updated_at: number;
}>;

export type NativeMarketingLeadEngagement = Readonly<{
  id: string;
  candidate_id: string;
  version: number;
  event_type: string;
  channel: string;
  evidence: string;
  actor_id: string;
  created_at: number;
}>;

export type NativeMarketingAssetProfile = Readonly<{
  id: string;
  company_id: string;
  kind: string;
  name: string;
  status: string;
  version: number;
  configuration_json: string;
  preview_json: string;
}>;

export type NativeMarketingAssetProfiles = Readonly<{
  assets: NativeMarketingAssetProfile[];
}>;

export type NativeMarketingLeadDetail = Readonly<{
  candidate: NativeMarketingLeadCandidate;
  history: NativeMarketingLeadEngagement[];
  canWrite: false;
  canHandoff: false;
  purchase: Readonly<{ newWork: false; settleExisting: false; history: true; revision: string }>;
}>;

export type NativeMarketingLeadConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

export type NativeMarketingLeadCommandReceipt = Readonly<{
  id: string;
  status: string;
  version: number;
  crmLeadId: string | null;
  code: string | null;
  name: string | null;
}>;

export type NativeMarketingLeadInput = Readonly<{
  holdingSlug: string;
  candidateId: string;
  branchSlug?: string;
  departmentSlug?: string;
  expected: Readonly<{
    companyId: string;
    assignmentId: string;
    revision: number;
    assignmentVersion: number;
  }>;
}>;

export type NativeMarketingLeadTransport = (
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeMarketingLeadError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Marketing lead request could not be completed.');
    this.name = 'NativeMarketingLeadError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeMarketingLeadError(code, status, message);
};
const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' &&
  value.length <= max &&
  !/[\u0000-\u001f\u007f]/.test(value);
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

function candidate(value: unknown, actor: NativeMarketingLeadPageContext): NativeMarketingLeadCandidate {
  if (
    !exact(value, [
      'id', 'company_id', 'branch_id', 'department_id', 'form_id', 'form_version',
      'form_name', 'contact_name', 'contact_detail', 'consent_evidence',
      'attribution_json', 'brief', 'status', 'crm_lead_id', 'created_by',
      'reviewed_by', 'version', 'created_at', 'updated_at',
    ]) ||
    !identifier(value.id) ||
    value.company_id !== actor.companyId ||
    !identifier(value.branch_id) ||
    (value.department_id !== null && !identifier(value.department_id)) ||
    !identifier(value.form_id) ||
    !integer(value.form_version, 1) ||
    !text(value.form_name, 160) ||
    !text(value.contact_name, 160) ||
    !text(value.contact_detail, 3000) ||
    !text(value.consent_evidence, 3000) ||
    !text(value.attribution_json, 65536) ||
    !text(value.brief, 3000) ||
    !['pending', 'handed_off', 'rejected'].includes(String(value.status)) ||
    (value.crm_lead_id !== null && !identifier(value.crm_lead_id)) ||
    !identifier(value.created_by) ||
    (value.reviewed_by !== null && !identifier(value.reviewed_by)) ||
    !integer(value.version, 1) ||
    !integer(value.created_at) ||
    !integer(value.updated_at)
  )
    return failure('NATIVE_MARKETING_LEAD_RESPONSE_INVALID');
  if (actor.branchId !== null && value.branch_id !== actor.branchId)
    return failure('NATIVE_MARKETING_LEAD_RESPONSE_INVALID');
  if (actor.departmentId !== null && value.department_id !== actor.departmentId)
    return failure('NATIVE_MARKETING_LEAD_RESPONSE_INVALID');
  return value as NativeMarketingLeadCandidate;
}

function engagement(value: unknown): NativeMarketingLeadEngagement {
  if (
    !exact(value, ['id', 'candidate_id', 'version', 'event_type', 'channel', 'evidence', 'actor_id', 'created_at']) ||
    !identifier(value.id) ||
    !identifier(value.candidate_id) ||
    !integer(value.version, 1) ||
    !text(value.event_type, 80) ||
    !text(value.channel, 80) ||
    !text(value.evidence, 3000) ||
    !identifier(value.actor_id) ||
    !integer(value.created_at)
  )
    return failure('NATIVE_MARKETING_LEAD_RESPONSE_INVALID');
  return value as NativeMarketingLeadEngagement;
}

function purchase(value: unknown): NativeMarketingLeadDetail['purchase'] {
  if (
    !record(value) ||
    value.newWork !== false ||
    value.settleExisting !== false ||
    value.history !== true ||
    !text(value.revision, 200)
  )
    return failure('NATIVE_MARKETING_LEAD_RESPONSE_INVALID');
  return { newWork: false, settleExisting: false, history: true, revision: value.revision };
}

function detail(value: unknown, actor: NativeMarketingLeadPageContext): NativeMarketingLeadDetail {
  if (!exact(value, ['candidate', 'history', 'purchase']) || !Array.isArray(value.history) || value.history.length > 500)
    return failure('NATIVE_MARKETING_LEAD_RESPONSE_INVALID');
  const current = candidate(value.candidate, actor);
  const history = value.history.map(engagement);
  for (const event of history) {
    if (event.candidate_id !== current.id) return failure('NATIVE_MARKETING_LEAD_RESPONSE_INVALID');
  }
  return {
    candidate: current,
    history,
    canWrite: false,
    canHandoff: false,
    purchase: purchase(value.purchase),
  };
}

function asset(value: unknown, actor: NativeMarketingLeadPageContext): NativeMarketingAssetProfile {
  if (
    !exact(value, [
      'id', 'company_id', 'kind', 'name', 'status', 'version',
      'configuration_json', 'preview_json',
    ]) ||
    !identifier(value.id) ||
    value.company_id !== actor.companyId ||
    !['form', 'content', 'domain', 'provider'].includes(String(value.kind)) ||
    !text(value.name, 160) ||
    !['draft', 'active', 'rejected', 'retired'].includes(String(value.status)) ||
    !integer(value.version, 1) ||
    !text(value.configuration_json, 65536) ||
    !text(value.preview_json, 65536)
  )
    return failure('NATIVE_MARKETING_LEAD_RESPONSE_INVALID');
  return value as NativeMarketingAssetProfile;
}

function context(
  value: unknown,
  actor: NativeMarketingLeadPageContext,
  input: NativeMarketingLeadInput,
): Context {
  if (
    !exact(value, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug',
      'departmentSlug', 'assignmentId', 'assignmentVersion', 'revision', 'role',
      'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ])
  )
    return failure('NATIVE_MARKETING_LEAD_RESPONSE_INVALID');
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
    !sameSet(actor.iam?.permissions ?? [], parsed.permissions) ||
    parsed.companyId !== input.expected.companyId ||
    parsed.assignmentId !== input.expected.assignmentId ||
    parsed.revision !== input.expected.revision ||
    parsed.assignmentVersion !== input.expected.assignmentVersion
  )
    return failure('NATIVE_MARKETING_LEAD_CONTEXT_CHANGED');
  return parsed;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativeMarketingLeadConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_MARKETING_LEAD_CONFIGURATION_INVALID');
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
    raw.timeoutMs > 30_000 ||
    raw.holdings.length < 1 ||
    raw.holdings.length > 64
  )
    return failure('NATIVE_MARKETING_LEAD_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_MARKETING_LEAD_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      (origin.port !== '' &&
        !(
          origin.port.length >= 1 &&
          origin.port.length <= 5 &&
          [...origin.port].every((character) => character >= '0' && character <= '9')
        )) ||
      origin.username ||
      origin.password ||
      !identifier(holding.holdingId) ||
      !text(holding.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug) ||
      holdings.has(origin.host)
    )
      return failure('NATIVE_MARKETING_LEAD_CONFIGURATION_INVALID');
    holdings.set(origin.host, { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativeMarketingLeadTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  input: NativeMarketingLeadInput,
  actor: NativeMarketingLeadPageContext,
  timeout: number,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== input.holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(new NativeMarketingLeadError('NATIVE_MARKETING_LEAD_AUTHORITY_INVALID'));
  const headers = new Headers({ host: host!, accept: 'application/json', 'cache-control': 'no-cache' });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeMarketingLeadError('NATIVE_MARKETING_LEAD_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  for (const [key, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return Promise.reject(new NativeMarketingLeadError('NATIVE_MARKETING_LEAD_REQUEST_INVALID'));
      headers.set(key, slug);
    }
  }
  headers.set('x-erpx-context-company', input.expected.companyId);
  headers.set('x-erpx-context-assignment', input.expected.assignmentId);
  headers.set('x-erpx-context-revision', String(input.expected.revision));
  headers.set('x-erpx-context-assignment-version', String(input.expected.assignmentVersion));
  const path = input.candidateId
    ? `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/marketing/leads/${encodeURIComponent(input.candidateId)}`
    : `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/marketing/assets`;
  const url = new URL(path, base);
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

function commandCall(
  transport: NativeMarketingLeadTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  input: NativeMarketingLeadInput,
  actor: NativeMarketingLeadPageContext,
  key: string,
  body: string,
  timeout: number,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (
    !host ||
    !holding ||
    holding.holdingSlug !== input.holdingSlug ||
    holding.holdingId !== actor.holdingId ||
    !key ||
    key.length > 256 ||
    !input.candidateId
  )
    return Promise.reject(new NativeMarketingLeadError('NATIVE_MARKETING_LEAD_AUTHORITY_INVALID'));
  const headers = new Headers({
    host,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'content-type': 'application/json',
    'idempotency-key': key,
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeMarketingLeadError('NATIVE_MARKETING_LEAD_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  for (const [name, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return Promise.reject(new NativeMarketingLeadError('NATIVE_MARKETING_LEAD_REQUEST_INVALID'));
      headers.set(name, slug);
    }
  }
  headers.set('x-erpx-context-company', input.expected.companyId);
  headers.set('x-erpx-context-assignment', input.expected.assignmentId);
  headers.set('x-erpx-context-revision', String(input.expected.revision));
  headers.set('x-erpx-context-assignment-version', String(input.expected.assignmentVersion));
  const path = `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/marketing/leads/${encodeURIComponent(input.candidateId)}/changes`;
  const url = new URL(path, base);
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, timeout);
  return transport('POST', url, headers, body, controller.signal).finally(() => {
    clearTimeout(timer);
    signal?.removeEventListener('abort', cancel);
  });
}

function receipt(value: unknown): NativeMarketingLeadCommandReceipt {
  if (
    !exact(value, ['id', 'status', 'version', 'crmLeadId', 'code', 'name']) ||
    !identifier(value.id) ||
    !text(value.status, 64) ||
    !integer(value.version, 1) ||
    (value.crmLeadId !== null && !identifier(value.crmLeadId)) ||
    !(value.code === null || identifier(value.code)) ||
    !(value.name === null || identifier(value.name))
  )
    return failure('NATIVE_MARKETING_LEAD_RESPONSE_INVALID');
  return value as NativeMarketingLeadCommandReceipt;
}

const defaultTransport: NativeMarketingLeadTransport = (method, url, headers, body, signal) =>
  new Promise((resolve, reject) => {
    const outbound = request(
      {
        method,
        hostname: url.hostname.replace(/^\[|\]$/g, ''),
        port: url.port,
        path: `${url.pathname}${url.search}`,
        headers: Object.fromEntries(headers),
        agent: new Agent({ keepAlive: false }),
        signal,
      },
      (incoming) => {
        const chunks: Buffer[] = [];
        incoming.on('data', (chunk: Buffer) => chunks.push(chunk));
        incoming.on('end', () => {
          const body = Buffer.concat(chunks);
          resolve(
            new Response(body, {
              status: incoming.statusCode ?? 500,
              headers: incoming.headers as Record<string, string>,
            }),
          );
        });
      },
    );
    outbound.on('error', reject);
    outbound.end(body);
  });

async function boundedJson(response: Response): Promise<unknown> {
  if (response.redirected) return failure('NATIVE_MARKETING_LEAD_RESPONSE_INVALID');
  if (response.headers.get('content-type')?.split(';')[0] !== 'application/json')
    return failure('NATIVE_MARKETING_LEAD_RESPONSE_INVALID');
  const reader = response.body?.getReader();
  if (!reader) return failure('NATIVE_MARKETING_LEAD_RESPONSE_INVALID');
  const chunks: Uint8Array[] = [];
  let size = 0;
  for (;;) {
    const next = await reader.read();
    if (next.done) break;
    size += next.value.byteLength;
    if (size > 2 * 1024 * 1024) {
      await reader.cancel();
      return failure('NATIVE_MARKETING_LEAD_RESPONSE_LIMIT');
    }
    chunks.push(next.value);
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
  } catch {
    return failure('NATIVE_MARKETING_LEAD_RESPONSE_INVALID');
  }
}

export function createNativeMarketingLeadClient(
  raw: NativeMarketingLeadConfiguration,
  transport: NativeMarketingLeadTransport = defaultTransport,
) {
  const parsed = configuration(raw);
  async function read(
    incoming: Pick<Headers, 'get'>,
    actor: NativeMarketingLeadPageContext,
    input: NativeMarketingLeadInput,
    candidateId: string | undefined,
    signal?: AbortSignal,
  ): Promise<unknown> {
    const response = await transportCall(
      transport,
      parsed.base,
      parsed.holdings,
      incoming,
      { ...input, candidateId: candidateId ?? '' },
      actor,
      parsed.timeout,
      signal,
    );
    const value = await boundedJson(response);
    if (response.status !== 200) {
      const error = record(value) ? value.error : undefined;
      const code = record(error) ? String(error.code ?? '') : '';
      const message = record(error) ? String(error.message ?? '') : '';
      return failure(
        code || 'NATIVE_MARKETING_LEAD_UNAVAILABLE',
        response.status,
        message || undefined,
      );
    }
    if (!exact(value, ['data', 'context']) || !record(value.data))
      return failure('NATIVE_MARKETING_LEAD_RESPONSE_INVALID');
    context(value.context, actor, { ...input, candidateId: candidateId ?? '' });
    return value.data;
  }
  return {
    async detail(
      incoming: Pick<Headers, 'get'>,
      actor: NativeMarketingLeadPageContext,
      input: NativeMarketingLeadInput,
      signal?: AbortSignal,
    ): Promise<NativeMarketingLeadDetail> {
      const data = await read(incoming, actor, input, input.candidateId, signal);
      return detail(data, actor);
    },
    async assets(
      incoming: Pick<Headers, 'get'>,
      actor: NativeMarketingLeadPageContext,
      input: NativeMarketingLeadInput,
      signal?: AbortSignal,
    ): Promise<NativeMarketingAssetProfiles> {
      const data = await read(incoming, actor, input, undefined, signal);
      if (!exact(data, ['assets']) || !Array.isArray(data.assets) || data.assets.length > 500)
        return failure('NATIVE_MARKETING_LEAD_RESPONSE_INVALID');
      return { assets: data.assets.map((value) => asset(value, actor)) };
    },
    async recordEngagement(
      incoming: Pick<Headers, 'get'>,
      actor: NativeMarketingLeadPageContext,
      input: NativeMarketingLeadInput,
      engagement: Readonly<{
        expectedVersion: number;
        eventType: string;
        channel: string;
        reason: string;
      }>,
      key: string,
      signal?: AbortSignal,
    ): Promise<NativeMarketingLeadCommandReceipt> {
      if (
        !integer(engagement.expectedVersion, 1) ||
        !text(engagement.eventType, 80) ||
        engagement.eventType.length < 2 ||
        !text(engagement.channel, 80) ||
        engagement.channel.length < 2 ||
        !text(engagement.reason, 3000) ||
        engagement.reason.trim().length < 3
      )
        return failure('NATIVE_MARKETING_LEAD_REQUEST_INVALID');
      const body = JSON.stringify({
        action: 'record-engagement',
        expectedVersion: engagement.expectedVersion,
        eventType: engagement.eventType,
        channel: engagement.channel,
        reason: engagement.reason,
      });
      const response = await commandCall(
        transport,
        parsed.base,
        parsed.holdings,
        incoming,
        input,
        actor,
        key,
        body,
        parsed.timeout,
        signal,
      );
      const value = await boundedJson(response);
      if (response.status !== 200) {
        const error = record(value) ? value.error : undefined;
        const code = record(error) ? String(error.code ?? '') : '';
        const message = record(error) ? String(error.message ?? '') : '';
        return failure(
          code || 'NATIVE_MARKETING_LEAD_UNAVAILABLE',
          response.status,
          message || undefined,
        );
      }
      if (!exact(value, ['data']) || !record(value.data))
        return failure('NATIVE_MARKETING_LEAD_RESPONSE_INVALID');
      const data = receipt(value.data);
      if (data.id !== input.candidateId || data.version !== engagement.expectedVersion + 1)
        return failure('NATIVE_MARKETING_LEAD_RESPONSE_INVALID');
      return data;
    },
  };
}

export function nativeMarketingLeadPageContext(actor: NativeMarketingLeadActor): NativeMarketingLeadPageContext {
  return {
    userId: actor.userId,
    holdingId: actor.holdingId,
    holdingSlug: actor.holdingSlug,
    companyId: actor.companyId,
    branchId: actor.branchId,
    departmentId: actor.departmentId,
    assignmentId: actor.assignmentId,
    role: actor.role,
    assignmentScope: actor.assignmentScope,
    scope: actor.scope,
    authority: actor.authority,
    modules: actor.modules,
    iam: actor.iam,
  };
}

function scope(actor: NativeMarketingLeadPageContext) {
  return {
    branchSlug: actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug,
    departmentSlug: actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined,
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_MARKETING_LEAD_CONFIG;
  if (raw === undefined) return undefined;
  let parsed: NativeMarketingLeadConfiguration;
  try {
    parsed = JSON.parse(raw) as NativeMarketingLeadConfiguration;
  } catch {
    return failure('NATIVE_MARKETING_LEAD_CONFIGURATION_INVALID');
  }
  return createNativeMarketingLeadClient(parsed);
}

function commandClient() {
  const raw =
    process.env.ERPX_NATIVE_MARKETING_COMMAND_CONFIG ??
    process.env.ERPX_NATIVE_MARKETING_LEAD_CONFIG;
  if (raw === undefined) return undefined;
  let parsed: NativeMarketingLeadConfiguration;
  try {
    parsed = JSON.parse(raw) as NativeMarketingLeadConfiguration;
  } catch {
    return failure('NATIVE_MARKETING_LEAD_CONFIGURATION_INVALID');
  }
  return createNativeMarketingLeadClient(parsed);
}

export async function nativeMarketingLeadDetail(
  incoming: Pick<Headers, 'get'>,
  actor: NativeMarketingLeadActor,
  candidateId: string,
): Promise<NativeMarketingLeadDetail | undefined> {
  const native = client();
  if (!native) return undefined;
  const context = nativeMarketingLeadPageContext(actor);
  const input: NativeMarketingLeadInput = {
    holdingSlug: actor.holdingSlug,
    candidateId,
    ...scope(context),
    expected: {
      companyId: actor.companyId,
      assignmentId: actor.assignmentId,
      revision: actor.authority?.revision ?? 0,
      assignmentVersion: actor.authority?.assignmentVersion ?? 0,
    },
  };
  return native.detail(incoming, context, input);
}

export async function nativeMarketingAssetProfiles(
  incoming: Pick<Headers, 'get'>,
  actor: NativeMarketingLeadActor,
): Promise<NativeMarketingAssetProfiles | undefined> {
  const native = client();
  if (!native) return undefined;
  const context = nativeMarketingLeadPageContext(actor);
  const input: NativeMarketingLeadInput = {
    holdingSlug: actor.holdingSlug,
    candidateId: '',
    ...scope(context),
    expected: {
      companyId: actor.companyId,
      assignmentId: actor.assignmentId,
      revision: actor.authority?.revision ?? 0,
      assignmentVersion: actor.authority?.assignmentVersion ?? 0,
    },
  };
  return native.assets(incoming, context, input);
}

export async function nativeMarketingLeadRecordEngagement(
  incoming: Pick<Headers, 'get'>,
  actor: NativeMarketingLeadActor,
  candidateId: string,
  engagement: Readonly<{
    expectedVersion: number;
    eventType: string;
    channel: string;
    reason: string;
  }>,
  key: string,
  signal?: AbortSignal,
): Promise<NativeMarketingLeadCommandReceipt | undefined> {
  const native = commandClient();
  if (!native) return undefined;
  const context = nativeMarketingLeadPageContext(actor);
  const input: NativeMarketingLeadInput = {
    holdingSlug: actor.holdingSlug,
    candidateId,
    ...scope(context),
    expected: {
      companyId: actor.companyId,
      assignmentId: actor.assignmentId,
      revision: actor.authority?.revision ?? 0,
      assignmentVersion: actor.authority?.assignmentVersion ?? 0,
    },
  };
  return native.recordEngagement(incoming, context, input, engagement, key, signal);
}

export function nativeMarketingLeadFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeMarketingLeadError)) return undefined;
  if (error.code === 'NATIVE_MARKETING_LEAD_CONTEXT_CHANGED')
    return {
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before retrying.',
    };
  if (error.status === 401)
    return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
  if (error.status === 403)
    return { status: 403, code: error.code, message: error.message };
  if (error.status === 404)
    return {
      status: 404,
      code: 'NOT_FOUND',
      message: 'The Marketing lead is not available in this scope.',
    };
  if (error.status === 409 || error.status === 422)
    return { status: error.status, code: error.code, message: error.message };
  if (error.code === 'NATIVE_MARKETING_LEAD_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_MARKETING_LEAD_UNAVAILABLE',
      message: 'The native Marketing lead reader is unavailable.',
    };
  if (error.code === 'NATIVE_MARKETING_LEAD_REQUEST_INVALID')
    return { status: 422, code: error.code, message: error.message };
  return {
    status: 503,
    code: 'NATIVE_MARKETING_LEAD_UNAVAILABLE',
    message: 'The native Marketing lead reader is unavailable.',
  };
}
