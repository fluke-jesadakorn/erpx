import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';

/** Rendering input from retained page authorization, never a grant. */
export type NativeMarketingCampaignPageContext = {
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

export type NativeMarketingCampaignConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;
type Context = Row & { modules: string[]; permissions: string[] };

export type NativeMarketingCampaignReadInput = Readonly<{
  holdingSlug: string;
  branchSlug?: string;
  departmentSlug?: string;
  expected?: Readonly<{
    companyId: string;
    assignmentId: string;
    revision: number;
    assignmentVersion: number;
  }>;
}>;

export type NativeMarketingCampaign = Readonly<{
  id: string;
  company_id: string;
  code: string;
  name: string;
  version: number;
  created_at: number;
}>;

export type NativeMarketingAttribution = Readonly<{
  campaign: string;
  source: string;
  captured: number;
  pending: number;
  handed_off: number;
  rejected: number;
}>;

export type NativeMarketingPurchaseControls = Readonly<{
  newWork: boolean;
  settleExisting: boolean;
  history: boolean;
  revision: string;
}>;

export type NativeMarketingCampaignReport = Readonly<{
  campaigns: NativeMarketingCampaign[];
  attribution: NativeMarketingAttribution[];
  purchase: NativeMarketingPurchaseControls | null;
}>;

export type NativeMarketingCampaignTransport = (
  method: 'GET',
  url: URL,
  headers: Headers,
  body: undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeMarketingCampaignError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Marketing request could not be completed.');
    this.name = 'NativeMarketingCampaignError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeMarketingCampaignError(code, status, message);
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
const codePattern = /^[A-Za-z0-9_-]{2,60}$/;
const trimName = (value: unknown): value is string =>
  typeof value === 'string' && value.length <= 160 &&
  !value.includes('\u0000') && value.trim().length >= 2;
const attributionKey = (value: unknown): value is string =>
  text(value, 300) && value.length > 0 && value === value.trim();

const compareBytes = (left: string, right: string): number => {
  const a = new TextEncoder().encode(left);
  const b = new TextEncoder().encode(right);
  const length = Math.min(a.length, b.length);
  for (let index = 0; index < length; index += 1) {
    if (a[index]! !== b[index]!) return a[index]! - b[index]!;
  }
  return a.length - b.length;
};

const campaign = (value: unknown, actor: NativeMarketingCampaignPageContext): value is NativeMarketingCampaign => {
  if (
    !exact(value, ['id', 'company_id', 'code', 'name', 'version', 'created_at']) ||
    !identifier(value.id) ||
    value.company_id !== actor.companyId ||
    typeof value.code !== 'string' ||
    !codePattern.test(value.code) ||
    !trimName(value.name) ||
    !integer(value.version, 1) ||
    !integer(value.created_at)
  )
    return false;
  return true;
};

const attribution = (value: unknown): value is NativeMarketingAttribution => {
  if (
    !exact(value, ['campaign', 'source', 'captured', 'pending', 'handed_off', 'rejected']) ||
    !attributionKey(value.campaign) ||
    !attributionKey(value.source) ||
    !integer(value.captured, 1) ||
    !integer(value.pending) ||
    !integer(value.handed_off) ||
    !integer(value.rejected) ||
    value.pending + value.handed_off + value.rejected !== value.captured
  )
    return false;
  return true;
};

function purchase(value: unknown): NativeMarketingPurchaseControls | null {
  if (value === null) return null;
  if (
    !exact(value, ['newWork', 'settleExisting', 'history', 'revision']) ||
    typeof value.newWork !== 'boolean' ||
    typeof value.settleExisting !== 'boolean' ||
    typeof value.history !== 'boolean' ||
    !text(value.revision, 200) ||
    value.revision.length === 0
  )
    return failure('NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID');
  return {
    newWork: value.newWork,
    settleExisting: value.settleExisting,
    history: value.history,
    revision: value.revision,
  };
}

function pageData(
  value: unknown,
  actor: NativeMarketingCampaignPageContext,
): NativeMarketingCampaignReport {
  if (
    !exact(value, ['campaigns', 'attribution', 'purchase']) ||
    !Array.isArray(value.campaigns) ||
    !Array.isArray(value.attribution)
  )
    return failure('NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID');
  if (value.campaigns.length > 500 || value.attribution.length > 1_000)
    return failure('NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID');
  const campaignIds = new Set<string>();
  let previousCampaign: NativeMarketingCampaign | null = null;
  for (const item of value.campaigns) {
    if (!campaign(item, actor)) return failure('NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID');
    if (campaignIds.has(item.id)) return failure('NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID');
    if (
      previousCampaign &&
      (compareBytes(previousCampaign.code, item.code) > 0 ||
        (previousCampaign.code === item.code && previousCampaign.id >= item.id))
    )
      return failure('NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID');
    previousCampaign = item;
    campaignIds.add(item.id);
  }
  const attributionKeys = new Set<string>();
  let previousAttribution: NativeMarketingAttribution | null = null;
  for (const item of value.attribution) {
    if (!attribution(item)) return failure('NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID');
    const key = `${item.campaign}\u0000${item.source}`;
    if (attributionKeys.has(key)) return failure('NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID');
    if (
      previousAttribution &&
      (previousAttribution.captured < item.captured ||
        (previousAttribution.captured === item.captured &&
          (compareBytes(previousAttribution.campaign, item.campaign) > 0 ||
            (previousAttribution.campaign === item.campaign &&
              compareBytes(previousAttribution.source, item.source) >= 0))))
    )
      return failure('NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID');
    previousAttribution = item;
    attributionKeys.add(key);
  }
  return {
    campaigns: value.campaigns as NativeMarketingCampaign[],
    attribution: value.attribution as NativeMarketingAttribution[],
    purchase: purchase(value.purchase),
  };
}

function context(
  value: unknown,
  actor: NativeMarketingCampaignPageContext,
  input: NativeMarketingCampaignReadInput,
): Context {
  if (
    !exact(value, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug',
      'departmentSlug', 'assignmentId', 'assignmentVersion', 'revision', 'role',
      'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ])
  )
    return failure('NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID');
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
    return failure('NATIVE_MARKETING_CAMPAIGN_CONTEXT_CHANGED');
  if (
    input.expected &&
    (parsed.companyId !== input.expected.companyId ||
      parsed.assignmentId !== input.expected.assignmentId ||
      parsed.revision !== input.expected.revision ||
      parsed.assignmentVersion !== input.expected.assignmentVersion)
  )
    return failure('NATIVE_MARKETING_CAMPAIGN_CONTEXT_CHANGED');
  return parsed;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativeMarketingCampaignConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_MARKETING_CAMPAIGN_CONFIGURATION_INVALID');
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
    return failure('NATIVE_MARKETING_CAMPAIGN_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_MARKETING_CAMPAIGN_CONFIGURATION_INVALID');
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
      return failure('NATIVE_MARKETING_CAMPAIGN_CONFIGURATION_INVALID');
    holdings.set(origin.host, { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativeMarketingCampaignTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  input: NativeMarketingCampaignReadInput,
  actor: NativeMarketingCampaignPageContext,
  timeout: number,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== input.holdingSlug || holding.holdingId !== actor.holdingId)
    return Promise.reject(new NativeMarketingCampaignError('NATIVE_MARKETING_CAMPAIGN_AUTHORITY_INVALID'));
  const headers = new Headers({ host: host!, accept: 'application/json', 'cache-control': 'no-cache' });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeMarketingCampaignError('NATIVE_MARKETING_CAMPAIGN_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  for (const [key, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return Promise.reject(new NativeMarketingCampaignError('NATIVE_MARKETING_CAMPAIGN_REQUEST_INVALID'));
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
      return Promise.reject(new NativeMarketingCampaignError('NATIVE_MARKETING_CAMPAIGN_REQUEST_INVALID'));
    headers.set('x-erpx-context-company', input.expected.companyId);
    headers.set('x-erpx-context-assignment', input.expected.assignmentId);
    headers.set('x-erpx-context-revision', String(input.expected.revision));
    headers.set('x-erpx-context-assignment-version', String(input.expected.assignmentVersion));
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/marketing/campaign-attribution`,
    base,
  );
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
        return failure('NATIVE_MARKETING_CAMPAIGN_RESPONSE_LIMIT');
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
    return failure('NATIVE_MARKETING_CAMPAIGN_UNAVAILABLE', response.status);
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
    return failure(code ?? 'NATIVE_MARKETING_CAMPAIGN_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID');
  }
}

export function nativeMarketingCampaignAvailable(): boolean {
  return process.env.ERPX_NATIVE_MARKETING_CAMPAIGN_CONFIG !== undefined;
}

export function nativeMarketingCampaignConfiguration(
  raw: string,
): NativeMarketingCampaignConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeMarketingCampaignError('NATIVE_MARKETING_CAMPAIGN_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    !Array.isArray(value.holdings)
  )
    return failure('NATIVE_MARKETING_CAMPAIGN_CONFIGURATION_INVALID');
  if (
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) &&
        text(holding.origin) &&
        text(holding.holdingSlug) &&
        text(holding.holdingId),
    )
  )
    return failure('NATIVE_MARKETING_CAMPAIGN_CONFIGURATION_INVALID');
  return value as unknown as NativeMarketingCampaignConfiguration;
}

export function createNativeMarketingCampaignClient(
  raw: NativeMarketingCampaignConfiguration,
  transport: NativeMarketingCampaignTransport = directHttp,
) {
  const config = configuration(raw);
  const report = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeMarketingCampaignPageContext,
    input: NativeMarketingCampaignReadInput,
    signal?: AbortSignal,
  ): Promise<NativeMarketingCampaignReport> => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return failure('NATIVE_MARKETING_CAMPAIGN_CONTEXT_CHANGED');
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
      if (error instanceof NativeMarketingCampaignError) throw error;
      return failure('NATIVE_MARKETING_CAMPAIGN_UNAVAILABLE');
    });
    const value = await boundedJson(response);
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID');
    context(value.context, actor, input);
    return pageData(value.data, actor);
  };
  return { report };
}

/** Fresh bounded socket; bypasses patched fetch/HMR and ambient proxies. */
export function directHttp(
  method: 'GET',
  url: URL,
  headers: Headers,
  body: undefined,
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
