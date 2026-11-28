import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { headers } from 'next/headers';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';

/**
 * Strict private transport for the owner-local Collaboration beta-facts reads.
 * The mounted axum route
 * `/api/v1/holdings/<holding>/collaboration/native-lane-15/beta-facts` is the
 * sole read authority: it owns the four `social_*` SQL statements and the
 * projection. This bridge emits no SQL and accepts no SQL from a caller.
 *
 * Every registered Holding is supported (1..64 entries). The target is
 * selected by the caller's verified WorkContext `holdingId` + `holdingSlug`,
 * never by a first/default entry; an unknown or ambiguous target fails closed.
 * Missing configuration, rejected requests and malformed replies also fail
 * closed with ApiError(503, ...); there is no TypeScript SQL fallback.
 */
type Row = Record<string, unknown>;
type Holding = {
  origin: string;
  host: string;
  holdingSlug: string;
  holdingId: string;
  companyId?: string;
};
export type HoldingActor = Pick<WorkContext, 'holdingId' | 'holdingSlug'> & {
  companyId?: string;
};
type Settings = { privateBaseUrl: string; timeoutMs: number; holdings: Holding[] };

export const NATIVE_COLLABORATION_LANE15_CONFIG_ENV = 'ERPX_NATIVE_COLLABORATION_LANE15_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

export type PostDraft = {
  id: string; body: string; branch_id: string | null; department_id: string | null;
  audience_kind: string | null; audience_id: string | null; participants_json: string;
  sources_json: string; purpose: string; version: number; mentions_json: string;
  content_json: string | null; rich_content_json: string | null;
};
export type PostReport = { id: string; reason: string; post_id: string; state: string };
export type TaskOrigin = { task_id: string };

const fail = (status: number, code: string, message = 'The native Collaboration beta-facts read is unavailable.'): never => {
  throw new ApiError(status, code, message);
};
const unavailable = (): never => fail(503, 'NATIVE_COLLABORATION_LANE15_UNAVAILABLE');
const record = (value: unknown): value is Row => value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) && Object.keys(value).length === keys.length && keys.every(key => Object.hasOwn(value, key));
const text = (value: unknown, max = 8192): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string =>
  text(value, 200) && /^[A-Za-z0-9._:-]+$/.test(value);
const slug = (value: unknown): value is string =>
  text(value, 128) && /^[a-z0-9_-]+$/.test(value);
const nullableText = (value: unknown, max = 8192): boolean => value === null || text(value, max);
const integer = (value: unknown, min = Number.MIN_SAFE_INTEGER): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;

const draftKeys = [
  'id', 'body', 'branch_id', 'department_id', 'audience_kind', 'audience_id',
  'participants_json', 'sources_json', 'purpose', 'version', 'mentions_json',
  'content_json', 'rich_content_json',
] as const;
const reportKeys = ['id', 'reason', 'post_id', 'state'] as const;
const taskKeys = ['task_id'] as const;

const isDraft = (row: unknown): row is PostDraft =>
  exact(row, draftKeys) && identifier(row.id) && text(row.body, 30_000) &&
  nullableText(row.branch_id, 200) && nullableText(row.department_id, 200) &&
  nullableText(row.audience_kind, 40) && nullableText(row.audience_id, 200) &&
  text(row.participants_json) && text(row.sources_json) &&
  text(row.purpose, 40) && integer(row.version, 1) && text(row.mentions_json) &&
  nullableText(row.content_json) && nullableText(row.rich_content_json);
const isReport = (row: unknown): row is PostReport =>
  exact(row, reportKeys) && identifier(row.id) && text(row.reason, 2000) &&
  identifier(row.post_id) && text(row.state, 40);
const isTask = (row: unknown): row is TaskOrigin =>
  exact(row, taskKeys) && identifier(row.task_id);

function settings(raw: string): Settings {
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return fail(503, 'NATIVE_COLLABORATION_LANE15_CONFIG_INVALID'); }
  if (!record(value) || !text(value.privateBaseUrl, 512) ||
    typeof value.timeoutMs !== 'number' || !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 || value.timeoutMs > 30_000 ||
    !Array.isArray(value.holdings) || value.holdings.length < 1 || value.holdings.length > 64)
    return fail(503, 'NATIVE_COLLABORATION_LANE15_CONFIG_INVALID');
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { return fail(503, 'NATIVE_COLLABORATION_LANE15_CONFIG_INVALID'); }
  const port = Number(base.port);
  if (base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port || RESERVED_PORTS.has(port) || (port >= 3200 && port <= 3299) ||
    base.username || base.password || base.origin !== value.privateBaseUrl)
    return fail(503, 'NATIVE_COLLABORATION_LANE15_CONFIG_INVALID');
  const hosts = new Set<string>();
  const ids = new Set<string>();
  const slugs = new Set<string>();
  const holdings: Holding[] = [];
  for (const item of value.holdings) {
    if (!record(item) || !text(item.origin, 512) || !text(item.host, 512) ||
      !identifier(item.holdingId) || !slug(item.holdingSlug) ||
      (item.companyId !== undefined && !identifier(item.companyId)))
      return fail(503, 'NATIVE_COLLABORATION_LANE15_CONFIG_INVALID');
    let origin: URL;
    try { origin = new URL(String(item.origin)); } catch { return fail(503, 'NATIVE_COLLABORATION_LANE15_CONFIG_INVALID'); }
    if (origin.protocol !== 'https:' || origin.origin !== item.origin || origin.username || origin.password ||
      origin.host !== item.host ||
      hosts.has(String(item.host)) || ids.has(String(item.holdingId)) || slugs.has(String(item.holdingSlug)))
      return fail(503, 'NATIVE_COLLABORATION_LANE15_CONFIG_INVALID');
    hosts.add(String(item.host));
    ids.add(String(item.holdingId));
    slugs.add(String(item.holdingSlug));
    holdings.push({
      origin: origin.origin,
      host: String(item.host),
      holdingSlug: String(item.holdingSlug),
      holdingId: String(item.holdingId),
      ...(item.companyId !== undefined ? { companyId: String(item.companyId) } : {}),
    });
  }
  return { privateBaseUrl: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** Selector presence only; an absent configuration never probes the service. */
export function nativeCollaborationLane15Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[NATIVE_COLLABORATION_LANE15_CONFIG_ENV] !== undefined;
}

/** Resolve the target by the caller's WorkContext identity only. No first or
 * host-default entry: exactly one configured holding must match both ids. */
function target(config: Settings, actor: HoldingActor): Holding {
  if (!record(actor as unknown as Row) || !identifier(actor.holdingId) || !slug(actor.holdingSlug))
    return fail(409, 'WORK_CONTEXT_CHANGED');
  const matches = config.holdings.filter(
    item => item.holdingId === actor.holdingId && item.holdingSlug === actor.holdingSlug,
  );
  if (matches.length !== 1) return fail(409, 'WORK_CONTEXT_CHANGED');
  const holding = matches[0];
  if (holding.companyId !== undefined && actor.companyId !== undefined &&
    holding.companyId !== actor.companyId)
    return fail(409, 'WORK_CONTEXT_CHANGED');
  return holding;
}

/** Derive the caller's WorkContext from the incoming request. The Host header
 * only supplies the slug used to resolve the session; the WorkContext that is
 * returned is then used by {@link target} to select the per-Holding target. */
async function callerActor(config: Settings): Promise<HoldingActor> {
  const incoming = await headers();
  const host = incoming.get('host');
  if (!host || !text(host, 512)) return fail(409, 'WORK_CONTEXT_CHANGED');
  const holding = config.holdings.find(item => item.host === host);
  if (!holding) return fail(409, 'WORK_CONTEXT_CHANGED');
  let actor: WorkContext;
  try {
    const identity = await import('@erpx/identity-access-server/public/authorization');
    actor = await identity.requireWorkContext(holding.holdingSlug);
  } catch {
    return fail(409, 'WORK_CONTEXT_CHANGED');
  }
  if (!actor || actor.holdingId !== holding.holdingId || actor.holdingSlug !== holding.holdingSlug)
    return fail(409, 'WORK_CONTEXT_CHANGED');
  return {
    holdingId: actor.holdingId,
    holdingSlug: actor.holdingSlug,
    ...(actor.companyId !== undefined ? { companyId: actor.companyId } : {}),
  };
}

/** POST with an explicit Host header; global fetch forbids overriding Host. */
function post(
  url: URL,
  headersValue: Record<string, string>,
  body: string,
  timeoutMs: number,
): Promise<{ status: number; value: unknown; contentType: string | undefined }> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import('node:http').ProxyEnv,
    });
    const outgoing = httpRequest(
      url,
      {
        method: 'POST',
        agent,
        headers: {
          ...headersValue,
          'content-type': 'application/json',
          'content-length': Buffer.byteLength(body),
        },
        signal: AbortSignal.timeout(timeoutMs),
        maxHeaderSize: 8192,
      },
      incoming => {
        const chunks: Buffer[] = [];
        incoming.on('data', chunk => chunks.push(Buffer.from(chunk)));
        incoming.on('end', () => {
          agent.destroy();
          const raw = Buffer.concat(chunks).toString('utf8');
          let value: unknown = null;
          try { value = raw ? JSON.parse(raw) : null; } catch { value = null; }
          const header = incoming.headers['content-type'];
          resolve({
            status: incoming.statusCode ?? 502,
            value,
            contentType: Array.isArray(header) ? header[0] : header,
          });
        });
        incoming.on('error', error => {
          agent.destroy();
          reject(error);
        });
      },
    );
    outgoing.on('error', error => {
      agent.destroy();
      reject(error);
    });
    outgoing.end(body);
  });
}

async function read(kind: string, fields: Row, actor?: HoldingActor): Promise<unknown> {
  const raw = process.env[NATIVE_COLLABORATION_LANE15_CONFIG_ENV];
  if (raw === undefined) return unavailable();
  const config = settings(raw);
  const resolved = actor ?? await callerActor(config);
  const holding = target(config, resolved);
  const body = JSON.stringify({ kind, ...fields });
  if (new TextEncoder().encode(body).length > 16 * 1024)
    return fail(503, 'NATIVE_COLLABORATION_LANE15_INPUT_INVALID');
  const headersValue: Record<string, string> = {
    host: holding.host,
    origin: holding.origin,
    accept: 'application/json',
    'cache-control': 'no-cache',
  };
  const company = fields.companyId;
  if (typeof company === 'string') {
    if (!identifier(company)) return fail(503, 'NATIVE_COLLABORATION_LANE15_INPUT_INVALID');
    headersValue['x-erpx-context-company'] = company;
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/collaboration/native-lane-15/beta-facts`,
    config.privateBaseUrl,
  );
  let response: { status: number; value: unknown; contentType: string | undefined };
  try {
    response = await post(url, headersValue, body, config.timeoutMs);
  } catch { return unavailable(); }
  if (response.status !== 200) {
    const error = record(response.value) && record(response.value.error)
      ? response.value.error
      : undefined;
    if (response.status >= 400 && response.status < 500)
      throw new ApiError(
        response.status,
        text(error?.code, 100) ? String(error!.code) : 'NATIVE_COLLABORATION_LANE15_READ_FAILED',
        text(error?.message, 300) ? String(error!.message) : 'The native beta-facts read was rejected.',
      );
    return unavailable();
  }
  if (response.contentType?.split(';')[0]?.trim() !== 'application/json') return unavailable();
  const payload = response.value;
  if (!record(payload) || !record(payload.data)) return fail(503, 'NATIVE_COLLABORATION_LANE15_RESPONSE_INVALID');
  return payload.data;
}

/** Drafts for one Company. An explicit WorkContext actor selects the Holding;
 * without one, the caller's verified WorkContext is derived from the request. */
export function nativeBetaPostDrafts(actor: HoldingActor, userId: string, companyId: string): Promise<PostDraft[]>;
export function nativeBetaPostDrafts(userId: string, companyId: string): Promise<PostDraft[]>;
export async function nativeBetaPostDrafts(
  first: HoldingActor | string,
  second: string,
  third?: string,
): Promise<PostDraft[]> {
  const actor = typeof first === 'string' ? undefined : first;
  const userId = typeof first === 'string' ? first : second;
  const companyId = typeof first === 'string' ? second : third;
  if (!identifier(userId) || !identifier(companyId ?? ''))
    return fail(503, 'NATIVE_COLLABORATION_LANE15_INPUT_INVALID');
  const data = await read('post_drafts', { userId, companyId }, actor);
  if (!record(data) || !Array.isArray(data.drafts) || data.drafts.length > 20 ||
    !data.drafts.every(isDraft))
    return fail(503, 'NATIVE_COLLABORATION_LANE15_RESPONSE_INVALID');
  return data.drafts as PostDraft[];
}

/** Reports for one Company, selected by WorkContext. */
export function nativeBetaReports(actor: HoldingActor, companyId: string): Promise<PostReport[]>;
export function nativeBetaReports(companyId: string): Promise<PostReport[]>;
export async function nativeBetaReports(
  first: HoldingActor | string,
  second?: string,
): Promise<PostReport[]> {
  const actor = typeof first === 'string' ? undefined : first;
  const companyId = typeof first === 'string' ? first : second;
  if (!identifier(companyId ?? '')) return fail(503, 'NATIVE_COLLABORATION_LANE15_INPUT_INVALID');
  const data = await read('reports', { companyId }, actor);
  if (!record(data) || !Array.isArray(data.reports) || data.reports.length > 100 ||
    !data.reports.every(isReport))
    return fail(503, 'NATIVE_COLLABORATION_LANE15_RESPONSE_INVALID');
  return data.reports as PostReport[];
}

/** Task origin by post id, selected by WorkContext. */
export function nativeBetaPostTask(actor: HoldingActor, postId: string | undefined): Promise<TaskOrigin | null>;
export function nativeBetaPostTask(postId: string | undefined): Promise<TaskOrigin | null>;
export async function nativeBetaPostTask(
  first: HoldingActor | string | undefined,
  second?: string | undefined,
): Promise<TaskOrigin | null> {
  const explicit = typeof first === 'object' && first !== null;
  const actor = explicit ? (first as HoldingActor) : undefined;
  const postId = explicit ? second : (first as string | undefined);
  return readTask('post_task', postId, actor);
}

/** Recovered task origin by post id, selected by WorkContext. */
export function nativeBetaRecoverPostTask(actor: HoldingActor, postId: string | undefined): Promise<TaskOrigin | null>;
export function nativeBetaRecoverPostTask(postId: string | undefined): Promise<TaskOrigin | null>;
export async function nativeBetaRecoverPostTask(
  first: HoldingActor | string | undefined,
  second?: string | undefined,
): Promise<TaskOrigin | null> {
  const explicit = typeof first === 'object' && first !== null;
  const actor = explicit ? (first as HoldingActor) : undefined;
  const postId = explicit ? second : (first as string | undefined);
  return readTask('recover_post_task', postId, actor);
}

async function readTask(
  kind: 'post_task' | 'recover_post_task',
  postId: string | undefined,
  actor?: HoldingActor,
): Promise<TaskOrigin | null> {
  if (typeof postId !== 'string' || !identifier(postId))
    return fail(503, 'NATIVE_COLLABORATION_LANE15_INPUT_INVALID');
  const data = await read(kind, { postId }, actor);
  if (!record(data) || !Object.hasOwn(data, 'task') ||
    (data.task !== null && !isTask(data.task)))
    return fail(503, 'NATIVE_COLLABORATION_LANE15_RESPONSE_INVALID');
  return data.task as TaskOrigin | null;
}
