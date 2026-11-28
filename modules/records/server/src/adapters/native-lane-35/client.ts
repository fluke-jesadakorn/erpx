import 'server-only';
import { ApiError } from '@erpx/platform-runtime-server/public/http';

/**
 * Strict private transport for the native Records work-report read (lane 35).
 * The axum owner executes the SQL, owns the Company fence, the kind predicate,
 * the submitted-time calculation and the response shape. This bridge emits no
 * SQL and holds no fallback: an absent/invalid configuration, a rejected
 * response or a malformed payload fails closed with ApiError(503, ...).
 */
export type NativeLane35Config = Readonly<{
  privateBaseUrl: string;
  holdingSlug: string;
  origin: string;
  timeoutMs: number;
}>;

export type WorkReportCandidateRow = Readonly<{
  approval_json: string | null;
  items_json: string | null;
  id: string;
  kind: 'request';
  type: 'purchase' | 'service';
  title: string;
  status: string;
  requester: string;
  branch: string | null;
  department: string | null;
  currency: string | null;
  amount: number | null;
  units: number | null;
  submitted_at: number | null;
  created_at: number;
  updated_at: number;
}>;

export type WorkReportEventRow = Readonly<{
  record_id: string;
  action: string;
  actor: string;
  at: number;
  reason: string;
}>;

export type NativeLane35Transport = (
  url: URL,
  headers: Record<string, string>,
  body: string,
  signal: AbortSignal,
) => Promise<Response>;

const ENV = 'ERPX_NATIVE_RECORDS_LANE35_OWNER_READ_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const RESPONSE_LIMIT = 8 * 1024 * 1024;

const unavailable = (): never => {
  throw new ApiError(
    503,
    'NATIVE_RECORDS_WORK_REPORT_UNAVAILABLE',
    'The native Records work-report read is unavailable.',
  );
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 8192): value is string =>
  typeof value === 'string' && Buffer.byteLength(value) <= max;
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;

function configuration(raw: string | undefined): NativeLane35Config {
  if (raw === undefined) return unavailable();
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable();
  }
  if (!record(value)) return unavailable();
  const timeoutMs = value.timeoutMs;
  if (
    !text(value.privateBaseUrl, 512) ||
    !text(value.holdingSlug, 128) ||
    !/^[a-z0-9_-]+$/.test(value.holdingSlug) ||
    !text(value.origin, 512) ||
    typeof timeoutMs !== 'number' ||
    !Number.isSafeInteger(timeoutMs) ||
    timeoutMs < 250 ||
    timeoutMs > 30_000
  )
    return unavailable();
  let base: URL;
  let origin: URL;
  try {
    base = new URL(value.privateBaseUrl);
    origin = new URL(value.origin);
  } catch {
    return unavailable();
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    base.origin !== value.privateBaseUrl ||
    origin.protocol !== 'https:' ||
    origin.username ||
    origin.password ||
    origin.origin !== value.origin
  )
    return unavailable();
  return {
    privateBaseUrl: base.origin,
    holdingSlug: value.holdingSlug,
    origin: origin.origin,
    timeoutMs,
  };
}

async function directHttp(
  url: URL,
  headers: Record<string, string>,
  body: string,
  signal: AbortSignal,
): Promise<Response> {
  const response = await globalThis.fetch(url, {
    method: 'POST',
    headers,
    body,
    cache: 'no-store',
    redirect: 'error',
    signal,
  });
  if (response.redirected) return unavailable();
  return response;
}

const candidateKeys = [
  'amount', 'approval_json', 'branch', 'created_at', 'currency', 'department',
  'id', 'items_json', 'kind', 'requester', 'status', 'submitted_at', 'title',
  'type', 'units', 'updated_at',
];

const eventKeys = ['action', 'actor', 'at', 'reason', 'record_id'];

function exactKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  return actual.length === keys.length && keys.every((key, index) => actual[index] === key);
}

const nullableText = (value: unknown, max = 8192): value is string | null =>
  value === null || text(value, max);
const nullableInteger = (value: unknown, min = 0): value is number | null =>
  value === null || integer(value, min);

export function validateCandidates(value: unknown): WorkReportCandidateRow[] {
  if (!Array.isArray(value) || value.length > 5_000) return unavailable();
  return value.map((item) => {
    if (
      !record(item) ||
      !exactKeys(item, candidateKeys) ||
      !nullableText(item.approval_json) ||
      !nullableText(item.items_json) ||
      !text(item.id, 200) ||
      item.kind !== 'request' ||
      (item.type !== 'purchase' && item.type !== 'service') ||
      !text(item.title) ||
      !text(item.status, 128) ||
      !text(item.requester, 200) ||
      !nullableText(item.branch, 200) ||
      !nullableText(item.department, 200) ||
      !nullableText(item.currency, 64) ||
      !nullableInteger(item.amount) ||
      !nullableInteger(item.units) ||
      !nullableInteger(item.submitted_at) ||
      !integer(item.created_at) ||
      !integer(item.updated_at)
    )
      return unavailable();
    return item as unknown as WorkReportCandidateRow;
  });
}

export function validateEvents(value: unknown): WorkReportEventRow[] {
  if (!Array.isArray(value) || value.length > 5_000) return unavailable();
  return value.map((item) => {
    if (
      !record(item) ||
      !exactKeys(item, eventKeys) ||
      !text(item.record_id, 200) ||
      !text(item.action, 128) ||
      !text(item.actor, 200) ||
      !integer(item.at) ||
      !text(item.reason)
    )
      return unavailable();
    return item as unknown as WorkReportEventRow;
  });
}

/** True when the owner-native consumer switch is configured. */
export function nativeLane35Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}

async function read(
  body: Record<string, unknown>,
  config: NativeLane35Config,
  transport: NativeLane35Transport,
): Promise<unknown> {
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(config.holdingSlug)}/records/native-lane-35/work-report`,
    config.privateBaseUrl,
  );
  const headers: Record<string, string> = {
    host: new URL(config.origin).host,
    origin: config.origin,
    accept: 'application/json',
    'content-type': 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-company': String(body.companyId),
  };
  const raw = JSON.stringify(body);
  if (Buffer.byteLength(raw) > 128 * 1024) return unavailable();
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
          text(error?.code, 100) ? String(error!.code) : 'NATIVE_RECORDS_WORK_REPORT_REJECTED',
          text(error?.message, 300) ? String(error!.message) : 'The native Records read rejected the request.',
        );
      }
      return unavailable();
    }
    const payloadText = await response.text();
    if (Buffer.byteLength(payloadText) > RESPONSE_LIMIT) return unavailable();
    const payload: unknown = JSON.parse(payloadText);
    if (!record(payload) || !record(payload.data)) return unavailable();
    return payload.data;
  } catch (error) {
    if (error instanceof ApiError) throw error;
    return unavailable();
  } finally {
    clearTimeout(timer);
  }
}

/** Native owner candidates for one Company. No SQL and no fallback. */
export async function readNativeWorkReportCandidates(
  companyId: string,
  transport: NativeLane35Transport = directHttp,
): Promise<WorkReportCandidateRow[]> {
  if (!text(companyId, 200) || !/^[A-Za-z0-9_.:-]+$/.test(companyId)) return unavailable();
  const config = configuration(process.env[ENV]);
  const data = await read({ kind: 'candidates', companyId }, config, transport);
  if (!record(data) || !('candidates' in data)) return unavailable();
  return validateCandidates(data.candidates);
}

/** Native owner events for one Company and a bounded id list. */
export async function readNativeWorkReportEvents(
  companyId: string,
  ids: string,
  transport: NativeLane35Transport = directHttp,
): Promise<WorkReportEventRow[]> {
  if (
    !text(companyId, 200) ||
    !/^[A-Za-z0-9_.:-]+$/.test(companyId) ||
    !text(ids, 65_536)
  )
    return unavailable();
  const config = configuration(process.env[ENV]);
  const data = await read({ kind: 'events', companyId, ids }, config, transport);
  if (!record(data) || !('events' in data)) return unavailable();
  return validateEvents(data.events);
}
