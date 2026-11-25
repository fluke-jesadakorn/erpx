import 'server-only';
import { createHash } from 'node:crypto';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { SqlDatabase, SqlResult } from '@erpx/server-runtime/public/runtime/types';

/**
 * Strict owner transport for the native Finance `command-plan` read route.
 *
 * The axum owner plans the exact statement for a registered read kind; this
 * bridge emits no SQL, accepts no SQL from a caller, and executes the returned
 * statement on the caller's already-open unit so the read observes uncommitted
 * effects in the caller's serializable transaction. Read kinds carry no
 * receipt/audit/outbox ledger. Missing configuration or any configured failure
 * fails closed with no TypeScript fallback.
 */
type Holding = Readonly<{ origin: string; holdingSlug: string; holdingId: string; companyId: string }>;
type Settings = Readonly<{ privateBaseUrl: string; timeoutMs: number; holdings: readonly Holding[] }>;

/** Read-kind ids shared with the Rust read registry in `native_finance_read_sql.rs`. */
export const FINANCE_RD_ASSET_POLICY_ROW = 'finance_rd_asset_policy_row';
export const FINANCE_RD_ASSET_POLICY_RUNS = 'finance_rd_asset_policy_runs';
export const FINANCE_RD_INSTRUMENT_EDITION = 'finance_rd_instrument_edition';
export const FINANCE_RD_INSTRUMENT_RUNS = 'finance_rd_instrument_runs';
export const FINANCE_RD_COLLECTION_CASE_ROW = 'finance_rd_collection_case_row';
export const FINANCE_RD_BUSINESS_BILL_ROW = 'finance_rd_business_bill_row';
export const FINANCE_RD_PERIOD_END_DATES = 'finance_rd_period_end_dates';
export const FINANCE_RD_ALL_PERIODS = 'finance_rd_all_periods';
export const FINANCE_RD_SUPPLIER_BATCH_ROW = 'finance_rd_supplier_batch_row';
export const FINANCE_RD_SUPPLIER_INSTRUCTION_ROW = 'finance_rd_supplier_instruction_row';
export const FINANCE_RD_SUPPLIER_BATCH_LIST = 'finance_rd_supplier_batch_list';
export const FINANCE_RD_SUPPLIER_BATCH_WORK_LIST = 'finance_rd_supplier_batch_work_list';
export const FINANCE_RD_SUPPLIER_BATCH_ITEMS = 'finance_rd_supplier_batch_items';
export const FINANCE_RD_SUPPLIER_BATCH_INVOICES = 'finance_rd_supplier_batch_invoices';
export const FINANCE_RD_SUPPLIER_BATCH_INVOICE = 'finance_rd_supplier_batch_invoice';
export const FINANCE_RD_SUPPLIER_BATCH_ACCOUNTS = 'finance_rd_supplier_batch_accounts';
export const FINANCE_RD_SUPPLIER_BATCH_PERIODS = 'finance_rd_supplier_batch_periods';
export const FINANCE_RD_SUPPLIER_BATCH_BANK = 'finance_rd_supplier_batch_bank';
export const FINANCE_RD_TAX_PROFILE = 'finance_rd_tax_profile';
export const FINANCE_RD_TAX_PACKAGE = 'finance_rd_tax_package';
export const FINANCE_RD_TAX_CORRECTION = 'finance_rd_tax_correction';
export const FINANCE_RD_TAX_ENTRY = 'finance_rd_tax_entry';
export const FINANCE_RD_TAX_PREVIOUS_ENTRY = 'finance_rd_tax_previous_entry';
export const FINANCE_RD_TAX_ORIGINAL_ENTRY = 'finance_rd_tax_original_entry';
export const FINANCE_RD_TAX_PENDING_FAMILY = 'finance_rd_tax_pending_family';
export const FINANCE_RD_TAX_PROFILES = 'finance_rd_tax_profiles';
export const FINANCE_RD_TAX_CHART = 'finance_rd_tax_chart';
export const FINANCE_RD_TAX_CURRENT_ENTRY = 'finance_rd_tax_current_entry';
export const FINANCE_RD_TAX_PENDING_CORRECTION_COUNT = 'finance_rd_tax_pending_correction_count';
export const FINANCE_RD_TAX_CLOSE_PROFILE = 'finance_rd_tax_close_profile';
export const FINANCE_RD_BANK_IMPORTED_FILE = 'finance_rd_bank_imported_file';
export const FINANCE_RD_TAX_IDENTITY_ACTIVE_PARTY = 'finance_rd_tax_identity_active_party';
export const FINANCE_RD_TAX_IDENTITY_PREDECESSOR = 'finance_rd_tax_identity_predecessor';
export const FINANCE_RD_TAX_IDENTITY_ROW = 'finance_rd_tax_identity_row';
export const FINANCE_RD_TAX_IDENTITY_REVIEWED = 'finance_rd_tax_identity_reviewed';
export const FINANCE_RD_TAX_IDENTITY_LIST = 'finance_rd_tax_identity_list';
export const FINANCE_RD_TAX_IDENTITY_PARTY_LIST = 'finance_rd_tax_identity_party_list';
export const FINANCE_RD_ASSET_ACCOUNT_ROWS = 'finance_rd_asset_account_rows';
export const FINANCE_RD_ASSET_EVENT_LIST = 'finance_rd_asset_event_list';
export const FINANCE_RD_ASSET_DISPOSAL_LIST = 'finance_rd_asset_disposal_list';
export const FINANCE_RD_ASSET_REVIEWED_POLICY = 'finance_rd_asset_reviewed_policy';
export const FINANCE_RD_ASSET_DEPRECIATION_HISTORY = 'finance_rd_asset_depreciation_history';
export const FINANCE_RD_ASSET_LATEST_DEPRECIATION = 'finance_rd_asset_latest_depreciation';
export const FINANCE_RD_ASSET_RETAINED_POLICY_HASH = 'finance_rd_asset_retained_policy_hash';
export const FINANCE_RD_ASSET_DIMENSION_VALUES = 'finance_rd_asset_dimension_values';

const unavailable = (): never => {
  throw new ApiError(503, 'FINANCE_NATIVE_READ_UNAVAILABLE', 'The native Finance read service is unavailable.');
};
const record = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v);
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_FINANCE_READ_CONFIG;
  if (!raw) return unavailable();
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return unavailable(); }
  if (!record(value) || typeof value.privateBaseUrl !== 'string' || !Array.isArray(value.holdings)) return unavailable();
  if (typeof value.timeoutMs !== 'number' || !Number.isSafeInteger(value.timeoutMs) || value.timeoutMs < 250 || value.timeoutMs > 30_000) return unavailable();
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { return unavailable(); }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port ||
    reservedPorts.has(port) || (port >= 3200 && port <= 3299) || base.username || base.password ||
    value.privateBaseUrl !== base.origin
  ) return unavailable();
  const holdings = value.holdings.map((item: unknown) => {
    if (!record(item) || typeof item.origin !== 'string' || typeof item.holdingSlug !== 'string' ||
        typeof item.holdingId !== 'string' || typeof item.companyId !== 'string') return unavailable();
    let origin: URL;
    try { origin = new URL(item.origin); } catch { return unavailable(); }
    if (origin.protocol !== 'https:' || item.origin !== origin.origin || origin.username || origin.password ||
        !/^[a-z0-9_-]{1,128}$/.test(item.holdingSlug)) return unavailable();
    return { origin: item.origin, holdingSlug: item.holdingSlug, holdingId: item.holdingId, companyId: item.companyId };
  });
  if (holdings.length < 1 || holdings.length > 64) return unavailable();
  return { privateBaseUrl: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** Plan an owner read in Rust and run it on the caller's open unit. Pure read:
 * the kind selects the SQL; only routing-scoped values cross the wire. */
export async function nativeFinanceReadAll<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
): Promise<SqlResult<T>> {
  const config = configuration();
  const holding = config.holdings[0]!;
  const key = 'fin-read-' + createHash('sha256').update(kind).update('\u0000').update(JSON.stringify(values)).digest('hex').slice(0, 32);
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/finance/command-plan`, config.privateBaseUrl);
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'POST',
      headers: {
        host: new URL(holding.origin).host,
        origin: holding.origin,
        'content-type': 'application/json',
        accept: 'application/json',
        'cache-control': 'no-cache',
        'idempotency-key': key,
        'x-erpx-context-company': holding.companyId,
      },
      body: JSON.stringify({ kind, values, now: Date.now() }),
      cache: 'no-store',
      redirect: 'error',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch { return unavailable(); }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    const payload = await response.json().catch((): unknown => null);
    const err = record(payload) ? (payload as { error?: { code?: string; message?: string } }).error : undefined;
    if (response.status >= 400 && response.status < 500) {
      throw new ApiError(response.status, err?.code ?? 'FINANCE_NATIVE_READ_FAILED', err?.message ?? 'The native Finance read was rejected.');
    }
    return unavailable();
  }
  const payload = await response.json().catch((): unknown => null);
  const statements = record(payload) ? (payload as { data?: { statements?: unknown } }).data?.statements : undefined;
  if (!Array.isArray(statements) || statements.length !== 1) return unavailable();
  const planned = statements.map((statement: unknown) => {
    if (!record(statement) || typeof statement.sql !== 'string' || !Array.isArray(statement.values)) return unavailable();
    return db.prepare(statement.sql).bind(...statement.values);
  });
  const results = await db.batch<T>(planned);
  return results[0] ?? { results: [], success: true, meta: { changes: 0, duration: 0 } };
}

/** Planned owner read returning a single row. The retired TypeScript
 * `.first()` ran the same statement on the caller's unit and took `rows[0]`;
 * the native plan is executed the same way and the first row is returned (or
 * null), so the shape and fail-closed behavior are unchanged. */
export async function nativeFinanceReadFirst<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
): Promise<T | null> {
  const result = await nativeFinanceReadAll<T>(kind, values, db);
  return result.results[0] ?? null;
}

/** Planned owner read returning one named column of the first row. Mirrors the
 * retired `.first<T>(column)` used by the VAT count ports: a count row is
 * `{count: ...}`, so extracting the column yields the scalar rather than
 * the whole row object. A zero-row result mirrors `.first(column)`'s null. The
 * registered SQL bytes are unchanged; only the caller-side projection differs. */
export async function nativeFinanceReadScalarFirst<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
  column: string,
): Promise<T | null> {
  const result = await nativeFinanceReadAll<Record<string, unknown>>(kind, values, db);
  const row = result.results[0];
  if (!row || typeof row !== 'object') return null;
  const value = (row as Record<string, unknown>)[column];
  return value === undefined ? null : value as T;
}
