import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { Row } from '@erpx/server-commerce/public/commerce';

/**
 * Finance-owned record supplement facts. The four statements, projections and
 * row shapes are owned by the native lane 5 axum/sqlx reader; this adapter is a
 * strict transport with no SQL and no TypeScript fallback.
 *
 * `context` is optional only so existing call sites keep compiling: lane 35
 * must thread the caller's WorkContext `(holdingId, holdingSlug, companyId?)`
 * for these reads to work. Without it the adapter fails closed with 503 rather
 * than falling back to a first configured Holding or to local SQL.
 */
export type FinanceRecordFactsContext = Readonly<{
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;

type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
type Settings = Readonly<{ base: string; timeoutMs: number; holdings: readonly Holding[] }>;
type Args = Readonly<Record<string, string | number | null>>;

export const FINANCE_LANE5B_RD_PARTY = 'record_facts_party';
export const FINANCE_LANE5B_RD_ACCOUNT = 'record_facts_account';
export const FINANCE_LANE5B_RD_PERIOD = 'record_facts_period';
export const FINANCE_LANE5B_RD_ROWS = 'record_facts_rows';

const ENV = 'ERPX_NATIVE_FINANCE_CUSTOMER_READ_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

const unavailable = (code = 'NATIVE_FINANCE_RECORD_FACTS_UNAVAILABLE'): never => {
  throw new ApiError(503, code, 'The native Finance record facts service is unavailable.');
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 8192): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;

function configuration(): Settings {
  const raw = process.env[ENV];
  if (!raw) return unavailable('NATIVE_FINANCE_RECORD_FACTS_CONFIGURATION_REQUIRED');
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable('NATIVE_FINANCE_RECORD_FACTS_CONFIGURATION_INVALID');
  }
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
  )
    return unavailable('NATIVE_FINANCE_RECORD_FACTS_CONFIGURATION_INVALID');
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return unavailable('NATIVE_FINANCE_RECORD_FACTS_CONFIGURATION_INVALID');
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
    base.origin !== value.privateBaseUrl
  )
    return unavailable('NATIVE_FINANCE_RECORD_FACTS_CONFIGURATION_INVALID');
  const holdings: Holding[] = [];
  for (const item of value.holdings) {
    if (
      !record(item) ||
      !text(item.holdingId, 200) ||
      !text(item.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(String(item.holdingSlug)) ||
      !text(item.host, 512) ||
      item.host === '' ||
      (item.companyId !== undefined && !text(item.companyId, 200)) ||
      !text(item.origin, 512)
    )
      return unavailable('NATIVE_FINANCE_RECORD_FACTS_CONFIGURATION_INVALID');
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return unavailable('NATIVE_FINANCE_RECORD_FACTS_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password ||
      item.host !== origin.host
    )
      return unavailable('NATIVE_FINANCE_RECORD_FACTS_CONFIGURATION_INVALID');
    holdings.push({
      origin: origin.origin,
      host: String(item.host),
      holdingId: String(item.holdingId),
      holdingSlug: String(item.holdingSlug),
      ...(item.companyId !== undefined ? { companyId: String(item.companyId) } : {}),
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

function validValue(kind: string, value: unknown): boolean {
  const row = (v: unknown) => record(v) && text(v.id, 200);
  if (kind === FINANCE_LANE5B_RD_ROWS)
    return Array.isArray(value) && value.every(row);
  if (kind === FINANCE_LANE5B_RD_PERIOD)
    return (
      value === null ||
      (record(value) && text(value.start_date, 40) && text(value.end_date, 40))
    );
  return (
    (kind === FINANCE_LANE5B_RD_PARTY || kind === FINANCE_LANE5B_RD_ACCOUNT) &&
    (value === null || row(value))
  );
}

async function nativeRecordFacts<T>(
  kind: string,
  context: FinanceRecordFactsContext | undefined,
  companyId: string,
  args: Args,
): Promise<T> {
  if (!context || !text(context.holdingId, 200) || !text(context.holdingSlug, 128))
    return unavailable('NATIVE_FINANCE_RECORD_FACTS_CONTEXT_REQUIRED');
  if (!text(companyId, 200)) return unavailable('NATIVE_FINANCE_RECORD_FACTS_COMPANY_INVALID');
  if (context.companyId !== undefined && context.companyId !== companyId)
    throw new ApiError(409, 'WORK_CONTEXT_CHANGED', 'The selected Holding is unavailable.');
  const config = configuration();
  // No first-holding default: only the exact (holdingId, holdingSlug) from the
  // caller's WorkContext may select a native target.
  const target = config.holdings.find(
    (holding) =>
      holding.holdingId === context.holdingId && holding.holdingSlug === context.holdingSlug,
  );
  if (!target) throw new ApiError(409, 'WORK_CONTEXT_CHANGED', 'The selected Holding is unavailable.');
  if (target.companyId !== undefined && target.companyId !== companyId)
    throw new ApiError(409, 'WORK_CONTEXT_CHANGED', 'The selected Holding is unavailable.');

  const headers = new Headers({
    host: target.host,
    origin: target.origin,
    accept: 'application/json',
    'content-type': 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-company': companyId,
  });
  const body = JSON.stringify({ kind, ...args });
  if (new TextEncoder().encode(body).length > 16 * 1024)
    throw new ApiError(422, 'FINANCE_RECORD_FACTS_INPUT_INVALID', 'The read request is too large.');
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/finance/record-facts`,
    config.base,
  );
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
  } catch {
    return unavailable();
  }
  if (response.status !== 200) {
    const payload = await response.json().catch((): unknown => null);
    const error = record(payload) && record(payload.error) ? payload.error : undefined;
    if (response.status >= 400 && response.status < 500)
      throw new ApiError(
        response.status,
        text(error?.code, 100) ? String(error!.code) : 'NATIVE_FINANCE_RECORD_FACTS_FAILED',
        text(error?.message, 300) ? String(error!.message) : 'The native Finance read was rejected.',
      );
    return unavailable();
  }
  const payload = await response.json().catch((): unknown => null);
  if (!record(payload) || payload.kind !== kind || !validValue(kind, payload.value))
    return unavailable('NATIVE_FINANCE_RECORD_FACTS_RESPONSE_INVALID');
  return payload.value as T;
}

export function RecordDetailsFinanceOwnedFacts(
  database: SqlDatabase,
  context?: FinanceRecordFactsContext,
) {
  void database;
  return {
    readCommerceMasterDetailRow2(id: string, companyId: string): Promise<Row | null> {
      return nativeRecordFacts<Row | null>(FINANCE_LANE5B_RD_PARTY, context, companyId, { id });
    },
    readRecordSupplementDataAccount(id: string, companyId: string): Promise<Row | null> {
      return nativeRecordFacts<Row | null>(FINANCE_LANE5B_RD_ACCOUNT, context, companyId, { id });
    },
    readRecordSupplementDataPeriod(periodId: string, companyId: string): Promise<Row | null> {
      return nativeRecordFacts<Row | null>(FINANCE_LANE5B_RD_PERIOD, context, companyId, { id: periodId });
    },
    readRecordSupplementDataRows3(
      id: string,
      companyId: string,
      chart_id: string | number | null,
      value: '0' | '1',
      value2: string | null,
      value3: string | null,
      value4: string | null,
      value5: string | null,
      offset: number,
    ): Promise<{ results: Row[] }> {
      return nativeRecordFacts<Row[]>(FINANCE_LANE5B_RD_ROWS, context, companyId, {
        id,
        chart_id: chart_id === null ? null : String(chart_id),
        value,
        value2,
        value3,
        value4,
        value5,
        offset,
      }).then((results) => ({ results }));
    },
  };
}
