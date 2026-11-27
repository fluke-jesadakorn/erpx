import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

/** Allow-listed settings read kinds resolved by the mounted owner command-plan route. */
export const RD_SETTINGS_BY_COMPANY = 'commerce_rd_settings_by_company';
export const RD_CURRENCY_BY_COMPANY = 'commerce_rd_currency_by_company';
export const RD_POSTING_MAPPING_STATE = 'commerce_rd_posting_mapping_state';

/** Allow-listed product-config read kinds resolved by the mounted owner command-plan route. */
export const RD_PRODUCT_PAGE_ROWS = 'commerce_rd_product_page_rows';
export const RD_PRODUCT_MASTER_ROWS = 'commerce_rd_product_master_rows';
export const RD_PRODUCT_ACTIVE = 'commerce_rd_product_active';
export const RD_PRODUCT_ACTIVE_OPTIONS = 'commerce_rd_product_active_options';
export const RD_PRODUCT_MASTER_DETAIL = 'commerce_rd_product_master_detail';
export const RD_PRODUCT_POINT = 'commerce_rd_product_point';

/** Allow-listed r28 Inventory count product selectors resolved by the mounted owner command-plan route. */
export const RD_COUNT_ACTIVE_STOCK_PRODUCTS = 'commerce_rd_count_active_stock_products';
export const RD_COUNT_ACTIVE_PRODUCTS = 'commerce_rd_count_active_products';

/** Allow-listed r27 posting-setup settings-image reads resolved by the mounted owner command-plan route. */
export const RD_EXPENSE_SETTINGS_IMAGE = 'commerce_rd_expense_settings_image';
export const RD_MFG_EXISTING_SETTINGS_IMAGE = 'commerce_rd_mfg_existing_settings_image';

/** Allow-listed r30 posting-setup receipt-image reads resolved by the mounted owner command-plan route. */
export const RD_EXPENSE_RECEIPT_IMAGE = 'commerce_rd_expense_receipt_image';
export const RD_MFG_EXISTING_RECEIPT_IMAGE = 'commerce_rd_mfg_existing_receipt_image';

/** Allow-listed r26 attachment source-image reads resolved by the mounted owner command-plan route. */
export const RD_SUPPORT_ORDER_SOURCE_IMAGE = 'commerce_rd_support_order_source_image';
export const RD_CRM_ORDER_SOURCE_IMAGE = 'commerce_rd_crm_order_source_image';

/** Owner kinds for the r25 settlement-line composite source fragments. */
export const RD_SALES_SETTLEMENT_LINES = 'commerce_rd_sales_settlement_lines';
export const RD_SUPPLY_SETTLEMENT_LINES = 'commerce_rd_supply_settlement_lines';

/** Allow-listed r15 owner schema-ready probes resolved by the mounted command-plan route. */
export const RD_BILLING_TEST_FINANCE_SCHEMA = 'commerce_rd_billing_test_finance_schema';
export const RD_MFG_EXISTING_POSTING_SCHEMA = 'commerce_rd_mfg_existing_posting_schema';

/** Allow-listed legacy attachment owner scope read. */
export const RD_LEGACY_ATTACHMENT_SCOPE = 'commerce_rd_legacy_attachment_scope';

/** Allow-listed expense-posting prerequisite settings-source read. */
export const RD_EXPENSE_PREREQUISITE_SETTINGS_SOURCE = 'commerce_rd_expense_prerequisite_settings_source';

/** Allow-listed supplier-bill attachment receipt-id read. */
export const RD_SUPPLIER_BILL_RECEIPT_IDS = 'commerce_rd_supplier_bill_receipt_ids';

/** Allow-listed POS private-image product reads. */
export const RD_POS_PRODUCT_VERSION = 'commerce_rd_pos_product_version';
export const RD_POS_PRODUCT_IMAGE = 'commerce_rd_pos_product_image';

/** Allow-listed owner-local landed-cost reads. */
export const RD_LANDED_COST_RECEIPT_LINES = 'commerce_rd_landed_cost_receipt_lines';
export const RD_LANDED_COST_POSTING_CONFIG = 'commerce_rd_landed_cost_posting_config';

/** Allow-listed transit guard predicates owned by the native Commerce plan registry. */
export const RD_TRANSIT_ACTIVE_LOCATION = 'commerce_rd_transit_active_location';
export const RD_TRANSIT_PRODUCT_VERSION = 'commerce_rd_transit_product_version';

/** Allow-listed POS full active-warehouses read. */
export const RD_POS_WAREHOUSES_ALL = 'commerce_rd_pos_warehouses_all';

/** Allow-listed existing Manufacturing posting confirmation-image read. */
export const RD_MFG_EXISTING_POSTING_CONFIRMATION_IMAGE = 'commerce_rd_mfg_existing_posting_confirmation_image';

/** Allow-listed sales delivery-notification line read. */
export const RD_SALES_NOTIFICATION_LINES = 'commerce_rd_sales_notification_lines';

/** Allow-listed purchase-control source/return-line reads. */
export const RD_PURCHASE_CONTROL_ORDER = 'commerce_rd_purchase_control_order';
export const RD_PURCHASE_CONTROL_INVOICE = 'commerce_rd_purchase_control_invoice';
export const RD_PURCHASE_CONTROL_RETURN_LINES = 'commerce_rd_purchase_control_return_lines';

/** Allow-listed initial physical-count master reads. */
export const RD_INITIAL_COUNT_WAREHOUSES = 'commerce_rd_initial_count_warehouses';
export const RD_INITIAL_COUNT_STOCK_PRODUCT = 'commerce_rd_initial_count_stock_product';

/** Allow-listed PMS owner-local product/warehouse reads. */
export const RD_PMS_PRODUCT_OPTIONS = 'commerce_rd_pms_product_options';
export const RD_PMS_WAREHOUSE_ID = 'commerce_rd_pms_warehouse_id';
export const RD_PMS_PRODUCT_WAREHOUSE_SCOPE = 'commerce_rd_pms_product_warehouse_scope';

/** Allow-listed attachment product source image read. */
export const RD_ATTACHMENT_PRODUCT_IMAGE = 'commerce_rd_attachment_product_image';

/** Allow-listed executive order-evidence plans (closed metric variants). */
export const RD_EXEC_SALES_ORDERS_EVIDENCE = 'commerce_rd_exec_sales_orders_evidence';
export const RD_EXEC_SALES_CANCELLATIONS_EVIDENCE = 'commerce_rd_exec_sales_cancellations_evidence';
export const RD_EXEC_SALES_CANCELLATION_VALUE_EVIDENCE = 'commerce_rd_exec_sales_cancellation_value_evidence';
export const RD_EXEC_SALES_FULFILLMENT_EVIDENCE = 'commerce_rd_exec_sales_fulfillment_evidence';

/** Owner kinds for the inline Inventory/executive projection fragments. */
export const RD_PROJECTION_ORDER_LINES = 'commerce_rd_projection_order_lines';
export const RD_PROJECTION_PRODUCTS = 'commerce_rd_projection_products';
export const RD_PROJECTION_RETURN_LINES = 'commerce_rd_projection_return_lines';
export const RD_PROJECTION_RETURN_LOT_ALLOCATIONS = 'commerce_rd_projection_return_lot_allocations';

/** Allow-listed attachment policy descriptor reads. */
export const RD_POLICY_PURCHASE_RECEIPT = 'commerce_rd_policy_purchase_receipt';
export const RD_POLICY_FULFILLMENT = 'commerce_rd_policy_fulfillment';
export const RD_POLICY_PRODUCT = 'commerce_rd_policy_product';

/** Allow-listed Commerce posting-prerequisite state witness. */
export const RD_POSTING_PREREQUISITE_STATE = 'commerce_rd_posting_prerequisite_state';

/** Allow-listed inventory attachment image reads. */
export const RD_INVENTORY_ATTACHMENT_WAREHOUSES = 'commerce_rd_inventory_attachment_warehouses';
export const RD_INVENTORY_ATTACHMENT_PRODUCTS = 'commerce_rd_inventory_attachment_products';
export const RD_INVENTORY_ATTACHMENT_STOCK = 'commerce_rd_inventory_attachment_stock';
export const RD_INVENTORY_ATTACHMENT_RECEIPT = 'commerce_rd_inventory_attachment_receipt';

/** Allow-listed workspace-record Commerce order existence probe. */
export const RD_WORKSPACE_ORDER_EXISTS = 'commerce_rd_workspace_order_exists';

/** Allow-listed order source-id lookup. */
export const RD_ORDER_BY_SOURCE_ID = 'commerce_rd_order_by_source_id';

/** Allow-listed warehouse-config read kinds resolved by the mounted owner command-plan route. */
export const RD_WAREHOUSE_PAGE_ROWS = 'commerce_rd_warehouse_page_rows';
export const RD_WAREHOUSE_MASTER_ROWS = 'commerce_rd_warehouse_master_rows';
export const RD_WAREHOUSE_ACTIVE = 'commerce_rd_warehouse_active';
export const RD_WAREHOUSES_ACTIVE = 'commerce_rd_warehouses_active';
export const RD_WAREHOUSE_BRANCH_ACTIVE = 'commerce_rd_warehouse_branch_active';

/**
 * Strict owner transport for the mounted native Commerce `command-plan` read
 * route. The axum owner plans the exact statement for a registered read kind;
 * this bridge emits no SQL, accepts no SQL from a caller and executes the
 * returned statement on the caller's already-open unit so the read observes
 * uncommitted effects in the caller's serializable transaction. Read kinds
 * carry no receipt/audit/outbox ledger. Missing configuration or any failed
 * configured request fails closed with no TypeScript fallback.
 */
type NativeConfig = Readonly<{
  base: string;
  host: string;
  holdingSlug: string;
  origin: string;
  timeoutMs: number;
}>;
function config(): NativeConfig {
  const raw = process.env.ERPX_NATIVE_COMMERCE_OWNER_WRITE_CONFIG;
  if (raw === undefined)
    throw new NativeCommerceReadError('COMMERCE_READ_UNAVAILABLE', 503, 'The native Commerce read service is not configured.');
  let value: {
    privateBaseUrl?: unknown;
    host?: unknown;
    holdingSlug?: unknown;
    origin?: unknown;
    timeoutMs?: unknown;
  };
  try {
    value = JSON.parse(raw) as typeof value;
  } catch {
    throw new NativeCommerceReadError('COMMERCE_READ_CONFIG_INVALID', 503, 'The native Commerce read transport is misconfigured.');
  }
  let base: URL, origin: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
    origin = new URL(String(value.origin));
  } catch {
    throw new NativeCommerceReadError('COMMERCE_READ_CONFIG_INVALID', 503, 'The native Commerce read transport is misconfigured.');
  }
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    origin.protocol !== 'https:' ||
    typeof value.host !== 'string' ||
    value.host !== origin.host ||
    typeof value.holdingSlug !== 'string' ||
    !/^[a-z0-9_-]{1,128}$/.test(value.holdingSlug) ||
    !Number.isInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30_000
  )
    throw new NativeCommerceReadError('COMMERCE_READ_CONFIG_INVALID', 503, 'The native Commerce read transport is misconfigured.');
  return {
    base: base.origin,
    host: value.host,
    holdingSlug: value.holdingSlug,
    origin: origin.origin,
    timeoutMs: value.timeoutMs as number,
  };
}

export class NativeCommerceReadError extends Error {
  readonly code: string;
  readonly status: number;
  constructor(code: string, status: number, message: string) {
    super(message);
    this.name = 'NativeCommerceReadError';
    this.code = code;
    this.status = status;
  }
}

const isRecord = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === 'object' && !Array.isArray(v);

/** POST with an explicit Host header; fetch forbids overriding Host. */
function post(
  url: URL,
  headers: Record<string, string>,
  body: string,
  timeoutMs: number,
): Promise<{ status: number; value: unknown }> {
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
        headers: { ...headers, 'content-length': String(Buffer.byteLength(body)) },
        signal: AbortSignal.timeout(timeoutMs),
        maxHeaderSize: 8192,
      },
      incoming => {
        const chunks: Buffer[] = [];
        incoming.on('data', chunk => chunks.push(Buffer.from(chunk)));
        incoming.on('end', () => {
          agent.destroy();
          const text = Buffer.concat(chunks).toString('utf8');
          let value: unknown = null;
          try {
            value = text ? JSON.parse(text) : null;
          } catch {
            value = null;
          }
          resolve({ status: incoming.statusCode ?? 502, value });
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

type Planned = { sql: string; values: unknown[] };

/** Ask the owner to plan one read; only the typed kind and positional values
 * cross the wire. The reply is validated to carry exactly one owner statement. */
async function plan(kind: string, values: readonly unknown[]): Promise<Planned> {
  const cfg = config();
  let response: { status: number; value: unknown };
  try {
    response = await post(
      new URL(`/api/v1/holdings/${encodeURIComponent(cfg.holdingSlug)}/commerce/command-plan`, cfg.base),
      {
        host: cfg.host,
        origin: cfg.origin,
        accept: 'application/json',
        'content-type': 'application/json',
      },
      JSON.stringify({ kind, values }),
      cfg.timeoutMs,
    );
  } catch {
    throw new NativeCommerceReadError('COMMERCE_READ_UNAVAILABLE', 503, 'The native Commerce read service is unavailable.');
  }
  const value: unknown = response.value;
  if (response.status !== 200) {
    const error = isRecord(value) && isRecord(value.error) ? value.error : {};
    throw new NativeCommerceReadError(
      typeof error.code === 'string' ? error.code : 'COMMERCE_READ_FAILED',
      response.status,
      typeof error.message === 'string' ? error.message : 'The owned read could not be planned.',
    );
  }
  const statements = isRecord(value) && isRecord(value.data) ? value.data.statements : undefined;
  if (!Array.isArray(statements) || statements.length !== 1)
    throw new NativeCommerceReadError('COMMERCE_READ_RESPONSE_INVALID', 503, 'The native Commerce plan reply was invalid.');
  const entry = statements[0];
  if (
    !isRecord(entry) ||
    typeof entry.sql !== 'string' ||
    !Array.isArray(entry.values) ||
    entry.values.length > 64
  )
    throw new NativeCommerceReadError('COMMERCE_READ_RESPONSE_INVALID', 503, 'The native Commerce plan reply was invalid.');
  return { sql: entry.sql, values: entry.values as unknown[] };
}

/** Planned owner read returning all rows, executed on the caller's unit. */
export async function nativeCommerceReadAll<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
): Promise<T[]> {
  const statement = await plan(kind, values);
  return (await db.prepare(statement.sql).bind(...statement.values).all<T>()).results;
}

/** Planned owner read returning a single row (retired `.first()` parity). */
export async function nativeCommerceReadFirst<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
): Promise<T | null> {
  const statement = await plan(kind, values);
  return db.prepare(statement.sql).bind(...statement.values).first<T>();
}

/** Owner-planned statement for a caller that already owns its unit (for
 * example a workflow assertion). No SQL is accepted from the caller; only the
 * typed kind and positional values cross the wire. */
export async function nativeCommerceReadPlan(
  kind: string,
  values: readonly unknown[],
): Promise<{ sql: string; values: unknown[] }> {
  return plan(kind, values);
}
