import 'server-only';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import { nativeRequest } from '@erpx/platform-runtime-server/public/native-http';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';

/**
 * Strict private transport for the native Finance lane 1 reads. The axum
 * owner executes the SQL, applies the Company scope and produces the complete
 * row arrays. This bridge emits no SQL, sends no caller predicate, and accepts
 * only the exact registered envelope. A Holding is selected solely by the
 * caller's WorkContext holdingId + holdingSlug; there is no first-holding
 * default. Missing configuration, an unknown target or any native failure
 * fails closed with no TypeScript fallback.
 */
export type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
export type Settings = Readonly<{ privateBaseUrl: string; timeoutMs: number; holdings: readonly Holding[] }>;
type ColumnType = 'string' | 'number' | 'nullable-string';
type RowSpec = Readonly<Record<string, ColumnType>>;

/** The caller's trusted Holding identity. A target is never inferred from a
 * bare slug, a Company id or the first configured Holding: both ids must
 * match. `companyId` may come from the WorkContext or the registered target. */
export type Lane1Actor = Pick<WorkContext, 'holdingId' | 'holdingSlug'> &
  Partial<Pick<WorkContext, 'companyId'>>;

export type ExpenseLedgerRow = Record<string, string | number | null>;

export const LANE_01_JOURNAL_LINES = 'journal-lines';
export const LANE_01_POSTING_ACCOUNTS = 'posting-accounts';

export const LANE_01_CONFIG_KEY = 'ERPX_NATIVE_LANE_1_READ_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

const JOURNAL_SPEC: RowSpec = {
  id: 'string', code: 'string', name: 'string', debit_minor: 'number', credit_minor: 'number',
};
const ACCOUNTS_SPEC: RowSpec = {
  id: 'string', code: 'string', name: 'string', status: 'string', chart_id: 'string', currency: 'nullable-string',
};

const unavailable = (code = 'LANE_01_NATIVE_READ_UNAVAILABLE'): never => {
  throw new ApiError(503, code, 'The native Finance lane 1 read service is unavailable.');
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 512): value is string =>
  typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= max;
const slug = (value: unknown): value is string =>
  text(value, 128) && /^[a-z0-9_-]+$/.test(value as string);
const identifier = (value: unknown): value is string =>
  text(value, 200) && /^[A-Za-z0-9_-]+$/.test(value as string);

export function lane1Settings(): Settings {
  const raw = process.env[LANE_01_CONFIG_KEY];
  if (!raw) return unavailable('LANE_01_READ_CONFIGURATION_REQUIRED');
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return unavailable('LANE_01_READ_CONFIGURATION_INVALID'); }
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
  ) return unavailable('LANE_01_READ_CONFIGURATION_INVALID');
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { return unavailable('LANE_01_READ_CONFIGURATION_INVALID'); }
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
  ) return unavailable('LANE_01_READ_CONFIGURATION_INVALID');
  const holdings: Holding[] = [];
  for (const item of value.holdings) {
    if (
      !record(item) ||
      !text(item.origin, 512) ||
      !text(item.host, 512) ||
      !identifier(item.holdingId) ||
      !slug(item.holdingSlug) ||
      !(item.companyId === undefined || identifier(item.companyId))
    ) return unavailable('LANE_01_READ_CONFIGURATION_INVALID');
    let origin: URL;
    try { origin = new URL(String(item.origin)); } catch { return unavailable('LANE_01_READ_CONFIGURATION_INVALID'); }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password ||
      origin.host !== item.host
    ) return unavailable('LANE_01_READ_CONFIGURATION_INVALID');
    holdings.push({
      origin: origin.origin,
      host: String(item.host),
      holdingId: String(item.holdingId),
      holdingSlug: String(item.holdingSlug),
      ...(item.companyId === undefined ? {} : { companyId: String(item.companyId) }),
    });
  }
  if (
    new Set(holdings.map(h => `${h.holdingId}\u0000${h.holdingSlug}`)).size !== holdings.length ||
    new Set(holdings.map(h => `${h.host}\u0000${h.holdingSlug}`)).size !== holdings.length
  ) return unavailable('LANE_01_READ_CONFIGURATION_INVALID');
  return { privateBaseUrl: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** Resolve the exact registered target from the caller's WorkContext. */
export function resolveLane1Target(actor: Lane1Actor): { config: Settings; holding: Holding; companyId: string } {
  const config = lane1Settings();
  if (!record(actor) || !identifier(actor.holdingId) || !slug(actor.holdingSlug))
    return unavailable('LANE_01_ACTOR_INVALID');
  const holding = config.holdings.find(
    h => h.holdingId === actor.holdingId && h.holdingSlug === actor.holdingSlug,
  );
  if (!holding) return unavailable('LANE_01_TARGET_UNAVAILABLE');
  const companyId = actor.companyId ?? holding.companyId;
  if (!identifier(companyId)) return unavailable('LANE_01_COMPANY_SCOPE_UNAVAILABLE');
  return { config, holding, companyId };
}

function validRows(rows: unknown, spec: RowSpec): rows is ExpenseLedgerRow[] {
  const keys = Object.keys(spec);
  return Array.isArray(rows) && rows.every(row => {
    if (!record(row) || Object.keys(row).length !== keys.length) return false;
    return keys.every(key => {
      const value = row[key];
      switch (spec[key]) {
        case 'string': return typeof value === 'string';
        case 'number': return typeof value === 'number' && Number.isFinite(value);
        case 'nullable-string': return value === null || typeof value === 'string';
        default: return false;
      }
    });
  });
}

async function read(
  kind: typeof LANE_01_JOURNAL_LINES | typeof LANE_01_POSTING_ACCOUNTS,
  actor: Lane1Actor,
  journalId?: string,
): Promise<ExpenseLedgerRow[]> {
  const { config, holding, companyId } = resolveLane1Target(actor);
  if (kind === LANE_01_JOURNAL_LINES && (typeof journalId !== 'string' || !journalId))
    return unavailable('LANE_01_JOURNAL_INVALID');
  if (kind === LANE_01_POSTING_ACCOUNTS && journalId !== undefined)
    return unavailable('LANE_01_JOURNAL_INVALID');

  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/finance/expense-posting-projection`,
    config.privateBaseUrl,
  );
  const headers = new Headers({
    host: holding.host,
    origin: holding.origin,
    'content-type': 'application/json',
    accept: 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-company': companyId,
  });
  const body = JSON.stringify(kind === LANE_01_JOURNAL_LINES ? { kind, journal_id: journalId } : { kind });
  if (new TextEncoder().encode(body).length > 4096) return unavailable('LANE_01_READ_INPUT_INVALID');

  let response: Response;
  try {
    response = await nativeRequest(
      url,
      {
        method: 'POST',
        headers: Object.fromEntries(headers),
        body,
        signal: AbortSignal.timeout(config.timeoutMs),
      },
      config.timeoutMs,
    );
  } catch { return unavailable(); }
  if (response.status !== 200) {
    const payload = await response.json().catch((): unknown => null);
    const error = record(payload) && record(payload.error) ? payload.error : undefined;
    if (response.status >= 400 && response.status < 500) {
      throw new ApiError(
        response.status,
        text(error?.code, 100) ? String(error!.code) : 'LANE_01_NATIVE_READ_FAILED',
        text(error?.message, 300) ? String(error!.message) : 'The native Finance lane 1 read was rejected.',
      );
    }
    return unavailable();
  }
  const payload = await response.json().catch((): unknown => null);
  const data = record(payload) ? payload.data : undefined;
  if (!record(data) || data.kind !== kind) return unavailable('LANE_01_NATIVE_READ_RESPONSE_INVALID');
  const spec = kind === LANE_01_JOURNAL_LINES ? JOURNAL_SPEC : ACCOUNTS_SPEC;
  if (!validRows(data.rows, spec)) return unavailable('LANE_01_NATIVE_READ_RESPONSE_INVALID');
  return data.rows;
}

/**
 * Native successor of `expenseJournalProjection`. The caller's open unit is
 * accepted for signature parity only; it is never prepared or bound here.
 */
export async function expenseJournalProjection(
  _db: SqlDatabase,
  actor: Lane1Actor,
  journalId: string,
): Promise<ExpenseLedgerRow[]> {
  void _db;
  return read(LANE_01_JOURNAL_LINES, actor, journalId);
}

/** Native successor of `expensePostingAccounts`, with the same row shape. */
export async function expensePostingAccounts(
  _db: SqlDatabase,
  actor: Lane1Actor,
): Promise<ExpenseLedgerRow[]> {
  void _db;
  return read(LANE_01_POSTING_ACCOUNTS, actor);
}

/** True when the native lane 1 consumer switch is configured. */
export function nativeLane1Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[LANE_01_CONFIG_KEY] !== undefined;
}
