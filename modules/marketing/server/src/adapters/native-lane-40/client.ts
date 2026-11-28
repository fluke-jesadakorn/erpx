import 'server-only';
import { createHash } from 'node:crypto';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import type { Row } from '@erpx/commerce-server/public/commerce';

/**
 * Strict private transport for the native Marketing lane-40 read family.
 *
 * The axum owner executes one closed statement and returns the projected rows.
 * This bridge emits no SQL, accepts no SQL from a caller and owns no business
 * calculation. Missing configuration or any configured failure fails closed
 * with 503; there is no TypeScript fallback read.
 */
export const LANE40_CANDIDATE_LIST = 'candidate-list';
export const LANE40_CANDIDATE_DETAIL = 'candidate-detail';
export const LANE40_CANDIDATE_HISTORY = 'candidate-history';
export const LANE40_CAMPAIGN_SUMMARY = 'campaign-summary';
export const LANE40_ACTIVE_FORM_SOURCE = 'active-form-source-v2';
export const LANE40_FORM_CONTENT_ROW = 'form-content-row-v2';
export const LANE40_PUBLIC_RECEIPT = 'public-receipt-v2';

const KINDS = new Set<string>([
  LANE40_CANDIDATE_LIST,
  LANE40_CANDIDATE_DETAIL,
  LANE40_CANDIDATE_HISTORY,
  LANE40_CAMPAIGN_SUMMARY,
  LANE40_ACTIVE_FORM_SOURCE,
  LANE40_FORM_CONTENT_ROW,
  LANE40_PUBLIC_RECEIPT,
]);

export type Lane40Actor = Pick<WorkContext, 'holdingId' | 'holdingSlug' | 'companyId'>;
export type Lane40Scope = Readonly<{
  branchId?: string | null;
  departmentId?: string | null;
  recordId?: string | null;
  assetId?: string | null;
  version?: number | null;
  enforced?: boolean;
  limit?: number;
}>;

/** One registered Holding. `companyId` is optional holding metadata (the
 * default Company of that Holding); it never substitutes for the caller's
 * current WorkContext Company, which is always sent as the scope header. */
type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
type Settings = Readonly<{ base: string; timeoutMs: number; holdings: readonly Holding[] }>;

const ENV = 'ERPX_NATIVE_MARKETING_LANE40_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const MAX_ROWS = 5000;
const MAX_ID = 200;

const unavailable = (code = 'MARKETING_LANE40_READ_UNAVAILABLE'): never => {
  throw new ApiError(503, code, 'The native Marketing read service is unavailable.');
};
const invalid = (): never => {
  throw new ApiError(503, 'MARKETING_LANE40_CONFIG_INVALID', 'The native Marketing read transport is misconfigured.');
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max: number): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const id = (value: unknown): value is string =>
  text(value, MAX_ID) && /^[A-Za-z0-9_.:-]+$/.test(value);

/** Selector presence only; an absent configuration never probes the service. */
export function nativeLane40Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}

function configuration(): Settings {
  const raw = process.env[ENV];
  if (raw === undefined) return unavailable('MARKETING_LANE40_CONFIGURATION_REQUIRED');
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return invalid(); }
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
  ) return invalid();
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { return invalid(); }
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
  ) return invalid();
  const holdings: Holding[] = [];
  const seen = new Set<string>();
  const seenCompany = new Set<string>();
  for (const item of value.holdings) {
    if (
      !record(item) ||
      !text(item.origin, 512) ||
      !text(item.host, 512) ||
      !id(item.holdingId) ||
      !id(item.holdingSlug) ||
      !/^[a-z0-9_-]{1,128}$/.test(String(item.holdingSlug)) ||
      (item.companyId !== undefined && !id(item.companyId))
    ) return invalid();
    let origin: URL;
    try { origin = new URL(String(item.origin)); } catch { return invalid(); }
    if (
      origin.protocol !== 'https:' || origin.origin !== item.origin ||
      origin.username || origin.password || origin.host !== item.host
    ) return invalid();
    const holding: Holding = {
      origin: origin.origin,
      host: String(item.host),
      holdingId: String(item.holdingId),
      holdingSlug: String(item.holdingSlug),
      ...(item.companyId === undefined ? {} : { companyId: String(item.companyId) }),
    };
    const key = `${holding.holdingId}\u0000${holding.holdingSlug}`;
    if (seen.has(key)) return invalid();
    seen.add(key);
    if (holding.companyId !== undefined) {
      if (seenCompany.has(holding.companyId)) return invalid();
      seenCompany.add(holding.companyId);
    }
    holdings.push(holding);
  }
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** Exact WorkContext selection only. There is no first-holding default and no
 * fallback by slug alone: either both `holdingId` and `holdingSlug` match one
 * registered Holding or the read fails closed. */
function target(config: Settings, actor: Lane40Actor): Holding {
  if (!id(actor.holdingId) || !id(actor.holdingSlug)) {
    throw new ApiError(409, 'WORK_CONTEXT_CHANGED', 'The selected Holding is not registered.');
  }
  const match = config.holdings.find(
    (holding) =>
      holding.holdingId === actor.holdingId && holding.holdingSlug === actor.holdingSlug,
  );
  if (!match) throw new ApiError(409, 'WORK_CONTEXT_CHANGED', 'The selected Holding is not registered.');
  return match;
}

function validateRows(kind: string, companyId: string, rows: readonly unknown[]): Row[] {
  if (rows.length > MAX_ROWS) return unavailable();
  const result: Row[] = [];
  for (const item of rows) {
    if (!record(item) || item.company_id !== companyId) return unavailable();
    for (const value of Object.values(item)) {
      if (value !== null && typeof value !== 'string' && typeof value !== 'number') return unavailable();
    }
    if (kind === LANE40_CAMPAIGN_SUMMARY) {
      const captured = item.captured;
      const pending = item.pending;
      const handedOff = item.handed_off;
      const rejected = item.rejected;
      if (
        typeof captured !== 'number' || typeof pending !== 'number' ||
        typeof handedOff !== 'number' || typeof rejected !== 'number' ||
        !Number.isSafeInteger(captured) || !Number.isSafeInteger(pending) ||
        !Number.isSafeInteger(handedOff) || !Number.isSafeInteger(rejected) ||
        captured < 1 || pending < 0 || handedOff < 0 || rejected < 0 ||
        pending + handedOff + rejected !== captured ||
        !text(item.campaign, 300) || !text(item.source, 300)
      ) return unavailable();
    } else if (kind === LANE40_ACTIVE_FORM_SOURCE) {
      if (!text(item.configuration_json, 1_048_576)) return unavailable();
    } else if (kind === LANE40_PUBLIC_RECEIPT) {
      if (
        !text(item.request_hash, 64) || item.request_hash.length !== 64 ||
        !/^[a-f0-9]{64}$/.test(String(item.request_hash)) || !id(item.candidate_id)
      ) return unavailable();
    } else if (kind === LANE40_FORM_CONTENT_ROW) {
      if (
        !id(item.id) ||
        typeof item.version !== 'number' || !Number.isSafeInteger(item.version) || item.version < 1 ||
        !text(item.preview_json, 1_048_576)
      ) return unavailable();
    } else if (!id(item.id)) {
      return unavailable();
    }
    result.push(item as Row);
  }
  return result;
}

function validateScope(kind: string, companyId: string, scope: Lane40Scope): void {
  if (!KINDS.has(kind)) throw new ApiError(503, 'MARKETING_LANE40_READ_REQUEST_INVALID', 'Review the Marketing read kind.');
  if (!id(companyId)) throw new ApiError(503, 'MARKETING_LANE40_READ_REQUEST_INVALID', 'Review the Marketing read scope.');
  for (const value of [scope.branchId, scope.departmentId, scope.recordId, scope.assetId]) {
    if (value !== undefined && value !== null && !id(value))
      throw new ApiError(503, 'MARKETING_LANE40_READ_REQUEST_INVALID', 'Review the Marketing read scope.');
  }
  if (scope.version !== undefined && scope.version !== null && (!Number.isSafeInteger(scope.version) || scope.version < 1))
    throw new ApiError(503, 'MARKETING_LANE40_READ_REQUEST_INVALID', 'Review the Marketing read scope.');
  if (scope.limit !== undefined && (!Number.isSafeInteger(scope.limit) || scope.limit < 1 || scope.limit > MAX_ROWS))
    throw new ApiError(503, 'MARKETING_LANE40_READ_REQUEST_INVALID', 'Review the Marketing read scope.');
  if ((kind === LANE40_CANDIDATE_DETAIL || kind === LANE40_CANDIDATE_HISTORY || kind === LANE40_PUBLIC_RECEIPT) && !scope.recordId)
    throw new ApiError(503, 'MARKETING_LANE40_READ_REQUEST_INVALID', 'Review the Marketing read scope.');
  if ((kind === LANE40_ACTIVE_FORM_SOURCE || kind === LANE40_FORM_CONTENT_ROW) &&
      (!scope.assetId || scope.version === undefined || scope.version === null))
    throw new ApiError(503, 'MARKETING_LANE40_READ_REQUEST_INVALID', 'Review the Marketing read scope.');
}

async function execute(
  kind: string,
  companyId: string,
  config: Settings,
  targetHolding: Holding,
  scope: Lane40Scope,
): Promise<Row[]> {
  const body = JSON.stringify({
    kind,
    branchId: scope.branchId ?? null,
    departmentId: scope.departmentId ?? null,
    recordId: scope.recordId ?? null,
    assetId: scope.assetId ?? null,
    version: scope.version ?? null,
    enforced: scope.enforced === true,
    ...(scope.limit === undefined ? {} : { limit: scope.limit }),
  });
  if (new TextEncoder().encode(body).length > 8 * 1024) return unavailable('MARKETING_LANE40_READ_REQUEST_INVALID');
  const key = 'mkt-lane40-' + createHash('sha256').update(kind).update('\u0000').update(body).digest('hex').slice(0, 32);
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(targetHolding.holdingSlug)}/marketing/lane-40/read`, config.base);
  const headers = new Headers({
    host: targetHolding.host,
    origin: targetHolding.origin,
    'content-type': 'application/json',
    accept: 'application/json',
    'cache-control': 'no-cache',
    'idempotency-key': key,
    'x-erpx-context-company': companyId,
  });
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'POST',
      headers: Object.fromEntries(headers),
      body,
      cache: 'no-store',
      redirect: 'error',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch { return unavailable(); }
  if (response.status !== 200) {
    const payload: unknown = await response.json().catch((): unknown => null);
    void response.body?.cancel().catch(() => undefined);
    const err = record(payload) && record(payload.error) ? payload.error : undefined;
    if (response.status >= 400 && response.status < 500)
      throw new ApiError(
        response.status,
        text(err?.code, 100) ? String(err!.code) : 'MARKETING_LANE40_READ_FAILED',
        text(err?.message, 300) ? String(err!.message) : 'The native Marketing read was rejected.',
      );
    return unavailable();
  }
  const payload: unknown = await response.json().catch((): unknown => null);
  const data = record(payload) ? payload.data : undefined;
  if (!record(data) || data.kind !== kind || data.companyId !== companyId || !Array.isArray(data.rows))
    return unavailable();
  return validateRows(kind, companyId, data.rows);
}

/** Execute one closed Marketing read on the owner and validate the exact
 * response shape before any row reaches the caller. */
export async function readMarketingLane40(
  kind: string,
  actor: Lane40Actor,
  scope: Lane40Scope = {},
): Promise<Row[]> {
  validateScope(kind, actor.companyId, scope);
  const config = configuration();
  return execute(kind, actor.companyId, config, target(config, actor), scope);
}

/** Anonymous owner reads (public form content) select their Holding by the
 * exact configured `companyId`. This is explicit selection, never a default:
 * no match or an ambiguous company fails closed. */
export async function readMarketingLane40ForCompany(
  kind: string,
  companyId: string,
  scope: Lane40Scope = {},
): Promise<Row[]> {
  validateScope(kind, companyId, scope);
  const config = configuration();
  const matches = config.holdings.filter((holding) => holding.companyId === companyId);
  if (matches.length !== 1) throw new ApiError(409, 'WORK_CONTEXT_CHANGED', 'The selected Holding is not registered.');
  return execute(kind, companyId, config, matches[0]!, scope);
}
