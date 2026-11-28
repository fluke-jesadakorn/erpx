import 'server-only';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import type { SqlValueRow } from '@erpx/platform-runtime-server/public/runtime/sql';
import { nativePosOne } from './native-owner-write';

/**
 * Owner-issued POS child-tail reads (round 30) on caller-owned transactions.
 * The SQL for every kind lives in the native POS owner registry
 * (`erpx-server-pos::adapters::child_tail_reads`); this bridge sends only a
 * typed kind and the positional values. The caller executes the statement on
 * its own unit, so no TypeScript SQL, mirror or fallback reader remains for
 * this family. Row-locked reads, PMS fragments, the day-report and Stripe
 * registries stay inline in their callers and modules.
 */

const LINES_SUM = 'pos_child_tail_lines_sum';
const CREATED_BY = 'pos_child_tail_created_by';
const COMPLETED_LINE = 'pos_child_tail_completed_line';
const REFUND_PAYMENT = 'pos_child_tail_refund_payment';

/** Gross quantity sum for lines before the given id in a fulfillment. */
export async function nativePosChildTailLinesSumStatement<T = SqlValueRow>(
  db: SqlDatabase,
  facts: { fulfillment_id: string | number | boolean | null; id: string | number | boolean | null },
): Promise<T | null> {
  return nativePosOne<T>(db, '', LINES_SUM, [facts.fulfillment_id, facts.id]);
}

/** Sale creator for a fulfillment line, bounded by the caller. */
export async function nativePosChildTailCreatedByStatement<T = SqlValueRow>(
  db: SqlDatabase,
  facts: { id: string | number | boolean | null; user: string },
): Promise<T | null> {
  return nativePosOne<T>(db, '', CREATED_BY, [facts.id, facts.user]);
}

/** Unreturned line of a completed sale in a fulfillment. */
export async function nativePosChildTailCompletedLineStatement<T = SqlValueRow>(
  db: SqlDatabase,
  id: string | number | boolean | null,
): Promise<T | null> {
  return nativePosOne<T>(db, '', COMPLETED_LINE, [id]);
}

/** Unsettled refund payment for a sale. */
export async function nativePosChildTailRefundPaymentStatement<T = SqlValueRow>(
  db: SqlDatabase,
  saleId: string,
): Promise<T | null> {
  return nativePosOne<T>(db, '', REFUND_PAYMENT, [saleId]);
}
