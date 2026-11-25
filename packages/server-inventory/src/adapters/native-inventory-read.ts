import 'server-only';
import { createHash } from 'node:crypto';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { SqlDatabase, SqlResult } from '@erpx/server-runtime/public/runtime/types';

/**
 * Strict owner transport for the native Inventory `command-plan` read route.
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

/** Read-kind ids shared with the Rust read registry in `native_command_sql.rs`. */
export const INVENTORY_RD_LEDGER_SUPPLEMENT_ROWS = 'inventory_rd_ledger_supplement_rows';
export const INVENTORY_RD_LOT_BALANCE_SUPPLEMENT_ROWS = 'inventory_rd_lot_balance_supplement_rows';
export const INVENTORY_RD_LEDGER_SUPPLEMENT_ROW = 'inventory_rd_ledger_supplement_row';
export const INVENTORY_RD_LOT_SUPPLEMENT_ROW = 'inventory_rd_lot_supplement_row';
export const INVENTORY_RD_STOCK_SUPPLEMENT_ROW = 'inventory_rd_stock_supplement_row';
export const INVENTORY_RD_STOCK_COUNT_ROW = 'inventory_rd_stock_count_row';
export const INVENTORY_RD_COUNT_SOURCE_ROW = 'inventory_rd_count_source_row';
export const INVENTORY_RD_STOCK_COUNT_DELTAS = 'inventory_rd_stock_count_deltas';
export const INVENTORY_RD_ACTIVE_RESERVATIONS_TOTAL = 'inventory_rd_active_reservations_total';
export const INVENTORY_RD_LEDGER_MOVEMENT_AFTER = 'inventory_rd_ledger_movement_after';
export const INVENTORY_RD_ATTACHMENT_OPERATIONAL_TRANSFER = 'inventory_rd_attachment_operational_transfer';
export const INVENTORY_RD_TRANSIT_ATTACHMENT_DESCRIPTOR = 'inventory_rd_transit_attachment_descriptor';
export const INVENTORY_RD_ATTACHMENT_DESCRIPTOR_LANDED_COST = 'inventory_rd_attachment_descriptor_landed_cost';
export const INVENTORY_RD_ATTACHMENT_DESCRIPTOR_STOCK_COUNT = 'inventory_rd_attachment_descriptor_stock_count';
export const INVENTORY_RD_ATTACHMENT_DESCRIPTOR_TRANSFER = 'inventory_rd_attachment_descriptor_transfer';
export const INVENTORY_RD_ATTACHMENT_DESCRIPTOR_TRANSIT = 'inventory_rd_attachment_descriptor_transit';
export const INVENTORY_RD_ATTACHMENT_DESCRIPTOR_LOT = 'inventory_rd_attachment_descriptor_lot';
export const INVENTORY_RD_COMMERCE_ATTACHMENT_STOCK_IMAGE = 'inventory_rd_commerce_attachment_stock_image';
export const INVENTORY_RD_IN_TRANSIT_CURRENCY_TOTALS = 'inventory_rd_in_transit_currency_totals';
export const INVENTORY_RD_FULFILLMENT_LOT_ALLOCATION = 'inventory_rd_fulfillment_lot_allocation';
export const INVENTORY_RD_POS_STARTUP_INVENTORY_STATE = 'inventory_rd_pos_startup_inventory_state';
export const INVENTORY_RD_WORKFLOW_DOCUMENT_STOCK_COUNT = 'inventory_rd_workflow_document_stock_count';
export const INVENTORY_RD_WORKFLOW_DOCUMENT_TRANSFER = 'inventory_rd_workflow_document_transfer';
export const INVENTORY_RD_WORKFLOW_DOCUMENT_PAGE_STOCK_COUNT = 'inventory_rd_workflow_document_page_stock_count';
export const INVENTORY_RD_WORKFLOW_DOCUMENT_PAGE_TRANSFER = 'inventory_rd_workflow_document_page_transfer';
export const INVENTORY_RD_ORDER_RESERVATIONS_BY_LINE = 'inventory_rd_order_reservations_by_line';
export const INVENTORY_RD_LOT_BALANCE_BY_LOT_WAREHOUSE = 'inventory_rd_lot_balance_by_lot_warehouse';
export const INVENTORY_RD_TRANSIT_EVENTS_WITH_RESOLUTION = 'inventory_rd_transit_events_with_resolution';
export const INVENTORY_RD_POSTED_TRANSIT_TRANSFER = 'inventory_rd_posted_transit_transfer';
export const INVENTORY_RD_TRANSFER_BY_ID = 'inventory_rd_transfer_by_id';
export const INVENTORY_RD_TRANSIT_DISPATCH_LINES = 'inventory_rd_transit_dispatch_lines';
export const INVENTORY_RD_LANDED_COST_DETAIL_LINES = 'inventory_rd_landed_cost_detail_lines';
export const INVENTORY_RD_LANDED_COST_DETAIL_LOTS = 'inventory_rd_landed_cost_detail_lots';
export const INVENTORY_RD_LANDED_COST_DETAIL_EVENTS = 'inventory_rd_landed_cost_detail_events';
export const INVENTORY_RD_TRANSIT_DETAIL_LOTS = 'inventory_rd_transit_detail_lots';
export const INVENTORY_RD_SELECTABLE_TRANSIT_LOTS = 'inventory_rd_selectable_transit_lots';

const unavailable = (): never => {
  throw new ApiError(503, 'INVENTORY_NATIVE_READ_UNAVAILABLE', 'The native Inventory read service is unavailable.');
};
const record = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v);
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_INVENTORY_CONFIG;
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
export async function nativeInventoryReadAll<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
): Promise<SqlResult<T>> {
  const config = configuration();
  const holding = config.holdings[0]!;
  const key = 'inv-read-' + createHash('sha256').update(kind).update('\u0000').update(JSON.stringify(values)).digest('hex').slice(0, 32);
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/inventory/command-plan`, config.privateBaseUrl);
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
      throw new ApiError(response.status, err?.code ?? 'INVENTORY_NATIVE_READ_FAILED', err?.message ?? 'The native Inventory read was rejected.');
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
export async function nativeInventoryReadFirst<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
): Promise<T | null> {
  const result = await nativeInventoryReadAll<T>(kind, values, db);
  return result.results[0] ?? null;
}
