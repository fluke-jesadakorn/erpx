import 'server-only';
import { headers } from 'next/headers';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

/**
 * Strict private transport for the native Collaboration lane 16 sent-notice
 * reads. The axum owner executes the SQL, applies the Company fence and produces
 * the complete rows. This bridge emits no SQL, sends no caller predicate and
 * holds no fallback: absent configuration, an unknown target, a rejected
 * response or a malformed payload fails closed with ApiError(503, ...).
 *
 * Every registered Holding is supported. When the caller supplies a WorkContext
 * the target is selected by its exact (holdingId, holdingSlug); a legacy fact
 * caller without one is pinned by the exact incoming Host. There is no
 * first-Holding default and an unknown target never reaches the private service.
 */
export type Lane16Actor = Pick<import('@erpx/server-identity_access/public/authorization').WorkContext, 'holdingId' | 'holdingSlug' | 'companyId'>;
export type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
export type Settings = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: readonly Holding[];
}>;
export type NativeLane16Transport = (
  url: URL,
  headers: Record<string, string>,
  body: string,
  signal: AbortSignal,
) => Promise<Response>;

const ENV = 'ERPX_NATIVE_LANE_16_READ_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const RESPONSE_LIMIT = 2 * 1024 * 1024;

const unavailable = (code = 'LANE_16_NATIVE_READ_UNAVAILABLE'): never => {
  throw new ApiError(503, code, 'The native Collaboration sent-notice read is unavailable.');
};
const changed = (): never => {
  throw new ApiError(409, 'LANE_16_WORK_CONTEXT_CHANGED', 'The registered Holding selection changed.');
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 512): value is string =>
  typeof value === 'string' && Buffer.byteLength(value) <= max;
const integer = (value: unknown): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
const scopeId = (value: unknown): value is string =>
  text(value, 200) && value.length > 0 && /^[A-Za-z0-9_.:+-]+$/.test(String(value));
const hostId = (value: unknown): value is string =>
  text(value, 253) && value.length > 0 && !/[\s/?#@]/.test(String(value));
const eventKey = (value: unknown): value is string =>
  text(value, 400) && /^[A-Za-z0-9_.:+@-]+$/.test(value);

type Target = Readonly<{ holding: Holding; company: string | undefined }>;

function configuration(raw: string | undefined): Settings {
  if (raw === undefined) return unavailable('LANE_16_READ_CONFIGURATION_REQUIRED');
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return unavailable('LANE_16_READ_CONFIGURATION_INVALID'); }
  if (
    !record(value) ||
    !text(value.privateBaseUrl, 512) ||
    typeof value.timeoutMs !== 'number' ||
    !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  ) return unavailable('LANE_16_READ_CONFIGURATION_INVALID');
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { return unavailable('LANE_16_READ_CONFIGURATION_INVALID'); }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    base.origin !== value.privateBaseUrl
  ) return unavailable('LANE_16_READ_CONFIGURATION_INVALID');
  const holdings: Holding[] = [];
  const hosts = new Set<string>();
  const ids = new Set<string>();
  const slugs = new Set<string>();
  for (const item of value.holdings) {
    if (
      !record(item) ||
      !text(item.origin, 512) ||
      !hostId(item.host) ||
      !scopeId(item.holdingId) ||
      !text(item.holdingSlug, 128) ||
      !/^[a-z0-9_-]{1,128}$/.test(String(item.holdingSlug)) ||
      (item.companyId !== undefined && !scopeId(item.companyId))
    ) return unavailable('LANE_16_READ_CONFIGURATION_INVALID');
    let origin: URL;
    try { origin = new URL(String(item.origin)); } catch { return unavailable('LANE_16_READ_CONFIGURATION_INVALID'); }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password ||
      origin.host !== String(item.host) ||
      hosts.has(String(item.host)) ||
      ids.has(String(item.holdingId)) ||
      slugs.has(String(item.holdingSlug))
    ) return unavailable('LANE_16_READ_CONFIGURATION_INVALID');
    hosts.add(String(item.host));
    ids.add(String(item.holdingId));
    slugs.add(String(item.holdingSlug));
    holdings.push({
      origin: origin.origin,
      host: String(item.host),
      holdingId: String(item.holdingId),
      holdingSlug: String(item.holdingSlug),
      ...(item.companyId === undefined ? {} : { companyId: String(item.companyId) }),
    });
  }
  return { privateBaseUrl: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** Resolve exactly one registered Holding. A WorkContext caller must name a
 * registered (holdingId, holdingSlug); a legacy fact caller is pinned by the
 * exact incoming Host. A configured companyId additionally pins the Company. */
async function targetFor(config: Settings, actor?: Lane16Actor): Promise<Target> {
  const incoming = await headers().catch(() => null);
  const host = incoming?.get('host') ?? null;
  const contextCompany = incoming?.get('x-erpx-context-company') ?? undefined;
  if (actor) {
    const holding = config.holdings.find(target =>
      target.holdingId === actor.holdingId &&
      target.holdingSlug === actor.holdingSlug &&
      (target.companyId === undefined || target.companyId === actor.companyId),
    );
    if (!holding || (host !== null && host !== holding.host)) return changed();
    return { holding, company: actor.companyId };
  }
  const holding = host === null ? undefined : config.holdings.find(target => target.host === host);
  if (!holding) return changed();
  return {
    holding,
    company: contextCompany ?? holding.companyId,
  };
}

async function directHttp(url: URL, headers: Record<string, string>, body: string, signal: AbortSignal): Promise<Response> {
  const response = await globalThis.fetch(url, {
    method: 'POST', headers, body, cache: 'no-store', redirect: 'error', signal,
  });
  if (response.redirected) return unavailable();
  return response;
}

const exactKeys = (value: Record<string, unknown>, keys: readonly string[]): boolean => {
  const actual = Object.keys(value).sort();
  return actual.length === keys.length && keys.every((key, index) => actual[index] === key);
};

const noticeRowKeys = ['accepted_count', 'created_at', 'delivered_count', 'id', 'title'];
const detailKeys = ['acceptedCount', 'body', 'completedCount', 'created_at', 'id', 'recipient_count', 'title'];

type NoticeRow = Readonly<{ id: string; title: string; accepted_count: number; created_at: number; delivered_count: number }>;
type NoticeDetail = Readonly<{
  id: string; title: string; body: string; recipient_count: number;
  created_at: number; acceptedCount: number; completedCount: number;
}>;

function rows(value: unknown): NoticeRow[] {
  if (!Array.isArray(value) || value.length > 50) return unavailable('LANE_16_NATIVE_READ_RESPONSE_INVALID');
  return value.map(item => {
    if (
      !record(item) || !exactKeys(item, noticeRowKeys) ||
      !text(item.id, 200) || !text(item.title, 8192) ||
      !integer(item.accepted_count) || !integer(item.created_at) || !integer(item.delivered_count)
    ) return unavailable('LANE_16_NATIVE_READ_RESPONSE_INVALID');
    return item as unknown as NoticeRow;
  });
}

function detail(value: unknown): NoticeDetail | null {
  if (value === null) return null;
  if (
    !record(value) || !exactKeys(value, detailKeys) ||
    !text(value.id, 200) || !text(value.title, 8192) || !text(value.body, 1_048_576) ||
    !integer(value.recipient_count) || !integer(value.created_at) ||
    !integer(value.acceptedCount) || !integer(value.completedCount)
  ) return unavailable('LANE_16_NATIVE_READ_RESPONSE_INVALID');
  return value as unknown as NoticeDetail;
}

async function read(
  body: Record<string, unknown>,
  target: Target,
  config: Settings,
  transport: NativeLane16Transport,
): Promise<Record<string, unknown>> {
  const { holding } = target;
  const company = body.companyId ?? target.company;
  if (!scopeId(company)) return unavailable('LANE_16_COMPANY_SCOPE_UNAVAILABLE');
  if (holding.companyId !== undefined && company !== holding.companyId) return changed();
  const route = String(body.kind).startsWith('push-') ? 'push-settings' : 'sent-notices';
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/collaboration/native-lane-16/${route}`,
    config.privateBaseUrl,
  );
  const headers: Record<string, string> = {
    host: holding.host,
    origin: holding.origin,
    accept: 'application/json',
    'content-type': 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-company': company,
    'x-erpx-context-holding': holding.holdingId,
  };
  if (body.kind === 'notices' || body.kind === 'notice') {
    if (!scopeId(body.senderId)) return unavailable('LANE_16_READ_INPUT_INVALID');
    headers['x-erpx-context-user'] = String(body.senderId);
  }
  const raw = JSON.stringify(body);
  if (Buffer.byteLength(raw) > 4096) return unavailable('LANE_16_READ_INPUT_INVALID');
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  try {
    const response = await transport(url, headers, raw, controller.signal);
    if (response.status !== 200 || !response.body) {
      void response.body?.cancel().catch(() => undefined);
      if (response.status >= 400 && response.status < 500) {
        const payload = await response.json().catch((): unknown => null);
        const error = record(payload) && record(payload.error) ? payload.error : undefined;
        throw new ApiError(
          response.status,
          text(error?.code, 100) ? String(error!.code) : 'LANE_16_NATIVE_READ_REJECTED',
          text(error?.message, 300) ? String(error!.message) : 'The native Collaboration read rejected the request.',
        );
      }
      return unavailable();
    }
    const payloadText = await response.text();
    if (Buffer.byteLength(payloadText) > RESPONSE_LIMIT) return unavailable();
    const payload: unknown = JSON.parse(payloadText);
    if (!record(payload) || !record(payload.data)) return unavailable('LANE_16_NATIVE_READ_RESPONSE_INVALID');
    const data = payload.data;
    if (data.kind !== body.kind) return unavailable('LANE_16_NATIVE_READ_RESPONSE_INVALID');
    return data;
  } catch (error) {
    if (error instanceof ApiError) throw error;
    return unavailable();
  } finally {
    clearTimeout(timer);
  }
}

async function request(
  body: Record<string, unknown>,
  actor: Lane16Actor | undefined,
  transport: NativeLane16Transport,
): Promise<Record<string, unknown>> {
  const config = configuration(process.env[ENV]);
  const target = await targetFor(config, actor);
  return read(body, target, config, transport);
}

/** Native successor of `taskSocialOrigin`. The caller's open unit is accepted
 * for signature parity only; it is never prepared or bound here. */
export async function taskSocialOrigin(
  _db: SqlDatabase,
  taskId: string,
  actor?: Lane16Actor,
  transport: NativeLane16Transport = directHttp,
): Promise<{ post_id: string } | undefined> {
  void _db;
  if (!scopeId(taskId)) return unavailable('LANE_16_READ_INPUT_INVALID');
  const data = await request({ kind: 'task-origin', taskId }, actor, transport);
  if (!('row' in data)) return unavailable('LANE_16_NATIVE_READ_RESPONSE_INVALID');
  if (data.row === null) return undefined;
  if (!record(data.row) || Object.keys(data.row).length !== 1 || !text(data.row.post_id, 200))
    return unavailable('LANE_16_NATIVE_READ_RESPONSE_INVALID');
  return { post_id: String(data.row.post_id) };
}

/** Native successor of `sentNoticeRows`, preserving the retained `.results` shape. */
export async function sentNoticeRows(
  _db: SqlDatabase,
  companyId: string,
  senderId: string,
  actor?: Lane16Actor,
  transport: NativeLane16Transport = directHttp,
): Promise<{ results: NoticeRow[] }> {
  void _db;
  if (!scopeId(companyId) || !scopeId(senderId)) return unavailable('LANE_16_READ_INPUT_INVALID');
  const data = await request({ kind: 'notices', companyId, senderId }, actor, transport);
  if (!('rows' in data)) return unavailable('LANE_16_NATIVE_READ_RESPONSE_INVALID');
  return { results: rows(data.rows) };
}

/** Native successor of `sentNoticeRow`, with the merged accepted/completed counts. */
export async function sentNoticeRow(
  _db: SqlDatabase,
  noticeId: string,
  companyId: string,
  senderId: string,
  actor?: Lane16Actor,
  transport: NativeLane16Transport = directHttp,
): Promise<NoticeDetail | undefined> {
  void _db;
  if (!scopeId(noticeId) || !scopeId(companyId) || !scopeId(senderId))
    return unavailable('LANE_16_READ_INPUT_INVALID');
  const data = await request({ kind: 'notice', noticeId, companyId, senderId }, actor, transport);
  if (!('row' in data)) return unavailable('LANE_16_NATIVE_READ_RESPONSE_INVALID');
  return detail(data.row) ?? undefined;
}

/** Native successor of `sentNoticeDeliveryCount`. */
export async function sentNoticeDeliveryCount(
  _db: SqlDatabase,
  companyId: string,
  eventKeyValue: string,
  actor?: Lane16Actor,
  transport: NativeLane16Transport = directHttp,
): Promise<{ n: number } | undefined> {
  void _db;
  if (!scopeId(companyId) || !eventKey(eventKeyValue)) return unavailable('LANE_16_READ_INPUT_INVALID');
  const data = await request({ kind: 'delivery-count', companyId, eventKey: eventKeyValue }, actor, transport);
  if (!('n' in data) || !integer(data.n)) return unavailable('LANE_16_NATIVE_READ_RESPONSE_INVALID');
  return { n: data.n };
}

const preferenceKeys = [
  'calendar_enabled', 'enabled', 'mention_enabled', 'notice_enabled', 'quiet_end_minute',
  'quiet_start_minute', 'system_enabled', 'work_enabled',
];
const boolean = (value: unknown): value is boolean => typeof value === 'boolean';
const nullableMinute = (value: unknown): value is number | null =>
  value === null || (typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 && value <= 1439);

function preference(value: unknown): Record<string, boolean | number | null> {
  if (!record(value) || !exactKeys(value, preferenceKeys)) return unavailable('LANE_16_NATIVE_READ_RESPONSE_INVALID');
  if (
    !boolean(value.enabled) || !boolean(value.work_enabled) || !boolean(value.mention_enabled) ||
    !boolean(value.system_enabled) || !boolean(value.calendar_enabled) || !boolean(value.notice_enabled) ||
    !nullableMinute(value.quiet_start_minute) || !nullableMinute(value.quiet_end_minute)
  ) return unavailable('LANE_16_NATIVE_READ_RESPONSE_INVALID');
  return value as Record<string, boolean | number | null>;
}

function endpointOwner(value: unknown): { id: string; user_id: string; session_id: string } | null {
  if (value === null) return null;
  if (!record(value) || Object.keys(value).length !== 3 ||
    !scopeId(value.id) || !scopeId(value.user_id) || !scopeId(value.session_id))
    return unavailable('LANE_16_NATIVE_READ_RESPONSE_INVALID');
  return value as unknown as { id: string; user_id: string; session_id: string };
}

/** Native successor of the push `preferences` read. */
export async function pushPreferences(
  _db: SqlDatabase,
  companyId: string,
  userId: string,
  actor?: Lane16Actor,
  transport: NativeLane16Transport = directHttp,
): Promise<Record<string, boolean | number | null> | null> {
  void _db;
  if (!scopeId(companyId) || !scopeId(userId)) return unavailable('LANE_16_READ_INPUT_INVALID');
  const data = await request({ kind: 'push-preferences', companyId, userId }, actor, transport);
  if (!('row' in data)) return unavailable('LANE_16_NATIVE_READ_RESPONSE_INVALID');
  return data.row === null ? null : preference(data.row);
}

/** Native successor of the push `sessionSubscriptionCount` read. */
export async function pushSessionSubscriptionCount(
  _db: SqlDatabase,
  companyId: string,
  userId: string,
  sessionId: string,
  actor?: Lane16Actor,
  transport: NativeLane16Transport = directHttp,
): Promise<{ n: number } | null> {
  void _db;
  if (!scopeId(companyId) || !scopeId(userId) || !scopeId(sessionId))
    return unavailable('LANE_16_READ_INPUT_INVALID');
  const data = await request({ kind: 'push-session-count', companyId, userId, sessionId }, actor, transport);
  if (!('n' in data) || !integer(data.n)) return unavailable('LANE_16_NATIVE_READ_RESPONSE_INVALID');
  return { n: data.n };
}

/** Native successor of the push `endpointOwner` read. */
export async function pushEndpointOwner(
  _db: SqlDatabase,
  companyId: string,
  endpointHash: string,
  actor?: Lane16Actor,
  transport: NativeLane16Transport = directHttp,
): Promise<{ id: string; user_id: string; session_id: string } | null> {
  void _db;
  if (!scopeId(companyId) || !scopeId(endpointHash)) return unavailable('LANE_16_READ_INPUT_INVALID');
  const data = await request({ kind: 'push-endpoint-owner', companyId, endpointHash }, actor, transport);
  if (!('row' in data)) return unavailable('LANE_16_NATIVE_READ_RESPONSE_INVALID');
  return endpointOwner(data.row);
}

/** True when the native lane 16 consumer switch is configured. */
export function nativeLane16Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}
