import "server-only";
import {legalPurchaseClockCteSql} from './legal-read-tail-r21-sources';
/** Byte-identical purchased-capability clock EXISTS wrapper shared by Legal owner adapters (round 29).
 * Text only: every call site still prepares and executes its own statement with unchanged binds. */
/** Caller supplies the fixed clock CTE name, its composed WHERE predicate and the
 * original MATERIALIZED/CTE separator; the r21 inner clock CTE is reused unchanged.
 * No caller SQL or predicate is invented. */
export const legalPurchaseClockExistsSql = (clock: string, predicate: string, separator = ' ') =>
  `EXISTS(WITH ${clock} AS MATERIALIZED${separator}${legalPurchaseClockCteSql} SELECT 1 FROM ${clock} WHERE ${predicate})`;
