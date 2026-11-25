import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { SqlValueRow } from '@erpx/server-runtime/public/runtime/sql';
import { nativePosAll, nativePosOne } from './native-owner-write';

/**
 * Owner-issued POS sale-child detail reads keyed by sale id for caller-owned
 * transactions. The SQL for every kind lives in the native POS owner registry
 * (`erpx-server-pos::adapters::sale_child_reads`); this bridge sends only a
 * typed kind and the sale id. The caller executes the statement on its own
 * unit, so no TypeScript SQL, mirror or fallback reader remains for this
 * family, including the two day-planner sale-child reads. Lock-bearing,
 * advisory/LOCK, PMS/cross-owner, sale-document, room-charge-reversal and
 * refund-child reads stay inline in the callers.
 */

const LINES_WITH_FULFILLMENT = 'pos_sale_child_lines_with_fulfillment';
const PAYMENTS_ORDERED = 'pos_sale_child_payments_ordered';
const LINES_ALL = 'pos_sale_child_lines_all';
const PAYMENTS_SUCCEEDED_TOTAL = 'pos_sale_child_payments_succeeded_total';
const PAYMENT_PENDING_ID = 'pos_sale_child_payment_pending_id';
const PAYMENTS_REFUNDABLE = 'pos_sale_child_payments_refundable';
const PAYMENT_ID = 'pos_sale_child_payment_id';
const LINES_FULFILLMENT_DETAIL = 'pos_sale_child_lines_fulfillment_detail';
const PAYMENT_FIRST_ID = 'pos_sale_child_payment_first_id';
const LINES_TOTAL = 'pos_sale_child_lines_total';
const PAYMENT_OPEN_ID = 'pos_sale_child_payment_open_id';
const DAY_PLANNER_LINES = 'pos_sale_child_day_planner_lines';
const DAY_PLANNER_PAYMENTS = 'pos_sale_child_day_planner_payments';

/** Sale lines joined to their fulfillment (workspace detail). */
export async function nativePosSaleLinesStatement(db: SqlDatabase, saleId: unknown): Promise<SqlValueRow[]> {
  return nativePosAll(db, '', LINES_WITH_FULFILLMENT, [saleId]);
}

/** Sale payments in created order. */
export async function nativePosSalePaymentsStatement(db: SqlDatabase, saleId: unknown): Promise<SqlValueRow[]> {
  return nativePosAll(db, '', PAYMENTS_ORDERED, [saleId]);
}

/** All plain sale lines. */
export async function nativePosLinesBySaleStatement(db: SqlDatabase, saleId: unknown): Promise<SqlValueRow[]> {
  return nativePosAll(db, '', LINES_ALL, [saleId]);
}

/** Succeeded payment net-total for a sale. */
export async function nativePosSucceededPaymentTotalStatement<T = SqlValueRow>(db: SqlDatabase, saleId: unknown): Promise<T | null> {
  return nativePosOne<T>(db, '', PAYMENTS_SUCCEEDED_TOTAL, [saleId]);
}

/** Pending/exception payment id for a sale. */
export async function nativePosPendingPaymentIdStatement<T = SqlValueRow>(db: SqlDatabase, saleId: unknown): Promise<T | null> {
  return nativePosOne<T>(db, '', PAYMENT_PENDING_ID, [saleId]);
}

/** Succeeded payments still carrying refundable value. */
export async function nativePosRefundablePaymentsStatement(db: SqlDatabase, saleId: unknown): Promise<SqlValueRow[]> {
  return nativePosAll(db, '', PAYMENTS_REFUNDABLE, [saleId]);
}

/** Plain payment id for a sale. */
export async function nativePosPaymentIdBySaleStatement<T = SqlValueRow>(db: SqlDatabase, saleId: unknown): Promise<T | null> {
  return nativePosOne<T>(db, '', PAYMENT_ID, [saleId]);
}

/** Sale lines with fulfillment cost/quantity/warehouse detail. */
export async function nativePosFulfilledLinesStatement(db: SqlDatabase, saleId: unknown): Promise<SqlValueRow[]> {
  return nativePosAll(db, '', LINES_FULFILLMENT_DETAIL, [saleId]);
}

/** First payment id for a sale. */
export async function nativePosFirstPaymentIdStatement<T = SqlValueRow>(db: SqlDatabase, saleId: unknown): Promise<T | null> {
  return nativePosOne<T>(db, '', PAYMENT_FIRST_ID, [saleId]);
}

/** Sale lines gross total. */
export async function nativePosLinesTotalStatement<T = SqlValueRow>(db: SqlDatabase, saleId: unknown): Promise<T | null> {
  return nativePosOne<T>(db, '', LINES_TOTAL, [saleId]);
}

/** Open or refundable payment id for a sale. */
export async function nativePosOpenPaymentIdStatement<T = SqlValueRow>(db: SqlDatabase, saleId: unknown): Promise<T | null> {
  return nativePosOne<T>(db, '', PAYMENT_OPEN_ID, [saleId]);
}

/** Day-planner sale lines with receipt detail. */
export async function nativePosDayPlannerLinesStatement(db: SqlDatabase, saleId: unknown): Promise<SqlValueRow[]> {
  return nativePosAll(db, '', DAY_PLANNER_LINES, [saleId]);
}

/** Day-planner succeeded sale payments with receipt detail. */
export async function nativePosDayPlannerPaymentsStatement(db: SqlDatabase, saleId: unknown): Promise<SqlValueRow[]> {
  return nativePosAll(db, '', DAY_PLANNER_PAYMENTS, [saleId]);
}
