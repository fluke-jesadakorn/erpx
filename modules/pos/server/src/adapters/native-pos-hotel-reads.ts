import 'server-only';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import type { SqlValueRow } from '@erpx/platform-runtime-server/public/runtime/sql';
import { nativePosAll, nativePosOne } from './native-owner-write';

/**
 * Owner-issued POS hotel day-assertion property reads (round 32) on the
 * caller-owned transaction. The SQL for each kind lives in the native POS
 * owner registry (`erpx-server-pos::adapters::hotel_reads`); this bridge sends
 * only a typed kind and the positional Company/id values. Both statements are
 * SELECT-only, Company-scoped and lock-free. The cross-owner PMS day assertion
 * remains in the caller; only the POS-owned property lookup is native.
 */

const REGISTER_PROPERTY = 'pos_hotel_register_property';
const FULFILLMENT_PROPERTY = 'pos_hotel_fulfillment_property';

/** Register `property_id,branch_id` for a Company's day assertion. */
export async function nativePosRegisterPropertyStatement<T = SqlValueRow>(
  db: SqlDatabase,
  company: string,
  registerId: unknown,
): Promise<T | null> {
  return nativePosOne<T>(db, company, REGISTER_PROPERTY, [company, registerId]);
}

/** Distinct `property_id,branch_id` rows behind a Company's fulfillment. */
export async function nativePosFulfillmentPropertyStatement(
  db: SqlDatabase,
  company: string,
  fulfillmentId: unknown,
): Promise<SqlValueRow[]> {
  return nativePosAll(db, company, FULFILLMENT_PROPERTY, [company, fulfillmentId]);
}
