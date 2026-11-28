import 'server-only';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import type { SqlValueRow } from '@erpx/platform-runtime-server/public/runtime/sql';
import { nativePosAll, nativePosOne } from './native-owner-write';

/**
 * Owner-issued POS day-report business-date list reads for caller-owned
 * transactions. The SQL for every kind lives in the native POS owner registry
 * (`erpx-server-pos::adapters::day_report_reads`); this bridge sends only a
 * typed kind and positional values. The caller executes the statement on its
 * own unit, so no TypeScript SQL, mirror or fallback reader remains for this
 * family. The lock-bearing `readPosDayCloses2` (`FOR UPDATE`) and the PMS
 * point-lookup `readPmsSources1` stay inline and are not part of this family.
 */

const DAY_CLOSES = 'pos_day_report_day_closes';
const SALES = 'pos_day_report_sales';
const SHIFTS = 'pos_day_report_shifts';
const PAYMENTS = 'pos_day_report_payments';
const REFUNDS = 'pos_day_report_refunds';
const REFUND_PAYMENTS = 'pos_day_report_refund_payments';
const STOCK_MOVES = 'pos_day_report_stock_moves';
const FULFILLMENTS = 'pos_day_report_fulfillments';
const CASH_MOVEMENTS = 'pos_day_report_cash_movements';
const SETTLEMENTS = 'pos_day_report_settlements';
const PAYMENT_MIX = 'pos_day_report_payment_mix';

type DateScope = { company: string; branch: string; date: string };

/** Owner-issued day-close existence row (`one` parity). */
export async function nativePosDayClosesStatement<T = SqlValueRow>(
  db: SqlDatabase, scope: DateScope,
): Promise<T | null> {
  return nativePosOne<T>(db, scope.company, DAY_CLOSES, [scope.company, scope.branch, scope.date]);
}

/** Owner-issued business-date sale list with PMS room-charge/reversal projection. */
export async function nativePosDaySalesStatement(db: SqlDatabase, scope: DateScope): Promise<SqlValueRow[]> {
  return nativePosAll(db, scope.company, SALES, [scope.company, scope.branch, scope.date]);
}

/** Owner-issued business-date shift list. */
export async function nativePosDayShiftsStatement(db: SqlDatabase, scope: DateScope): Promise<SqlValueRow[]> {
  return nativePosAll(db, scope.company, SHIFTS, [scope.company, scope.branch, scope.date]);
}

/** Owner-issued business-date payment list joined to its sale number. */
export async function nativePosDayPaymentsStatement(db: SqlDatabase, scope: DateScope): Promise<SqlValueRow[]> {
  return nativePosAll(db, scope.company, PAYMENTS, [scope.company, scope.branch, scope.date]);
}

/** Owner-issued accounting-date refund list joined to its sale number. */
export async function nativePosDayRefundsStatement(db: SqlDatabase, scope: DateScope): Promise<SqlValueRow[]> {
  return nativePosAll(db, scope.company, REFUNDS, [scope.company, scope.branch, scope.date]);
}

/** Owner-issued accounting-date refund-payment list joined to its payment method. */
export async function nativePosDayRefundPaymentsStatement(db: SqlDatabase, scope: DateScope): Promise<SqlValueRow[]> {
  return nativePosAll(db, scope.company, REFUND_PAYMENTS, [scope.company, scope.branch, scope.date]);
}

/** Owner-issued movement-date stock list scoped to the Company's warehouses. */
export async function nativePosDayStockMovesStatement(
  db: SqlDatabase, company: string, date: string, company2: string, branch: string,
): Promise<SqlValueRow[]> {
  return nativePosAll(db, company, STOCK_MOVES, [company, date, company2, branch]);
}

/** Owner-issued business-date fulfillments still in the kitchen queue. */
export async function nativePosDayFulfillmentsStatement(db: SqlDatabase, scope: DateScope): Promise<SqlValueRow[]> {
  return nativePosAll(db, scope.company, FULFILLMENTS, [scope.company, scope.branch, scope.date]);
}

/** Owner-issued business-date unapproved cash-movement ids. */
export async function nativePosDayCashMovementsStatement(db: SqlDatabase, scope: DateScope): Promise<SqlValueRow[]> {
  return nativePosAll(db, scope.company, CASH_MOVEMENTS, [scope.company, scope.branch, scope.date]);
}

/** Owner-issued Company settlement register (payout/balance exceptions). */
export async function nativePosDaySettlementsStatement(db: SqlDatabase, company: string): Promise<SqlValueRow[]> {
  return nativePosAll(db, company, SETTLEMENTS, [company]);
}

/** Owner-issued business-date payment-mix aggregate with refunded sums. */
export async function nativePosPaymentMixStatement(
  db: SqlDatabase,
  facts: { company: string; branch: string | null; branch2: string | null },
): Promise<SqlValueRow[]> {
  return nativePosAll(db, facts.company, PAYMENT_MIX, [
    facts.company, facts.branch, facts.branch2,
  ]);
}
