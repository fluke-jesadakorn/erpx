/** Byte-identical PMS read-tail r35 SQL heads shared across owner-local readers.
 * Text only: every consumer keeps its own WHERE/ORDER/LIMIT, binds, result
 * type and transaction; nothing is cached or executed here. Both heads are
 * byte-identical projections owned by this module, never caller SQL.
 * Locking reads, r17-r34 fragments and the native read registry are untouched. */

import { pmsSourcesSql } from "@erpx/server-pms/adapters/pms/source-policy";
import { PMS_RECIPIENT_CROSS_JOIN_SQL } from "./pms-read-tail-r21-sql";

/** Rank 1: byte-identical property-scoped id point-read head (five sites). */
export const PMS_RECORD_ID_PROPERTY_HEAD_SQL = 'SELECT id FROM pms_records WHERE property_id=?';

/** Rank 2: byte-identical source + recipient probe head (two owner consumers). */
export const pmsSourceRecipientProbeSql = (): string =>
  `SELECT 1 FROM (${pmsSourcesSql}) pr ${PMS_RECIPIENT_CROSS_JOIN_SQL}`;
