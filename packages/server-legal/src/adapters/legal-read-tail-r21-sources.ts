import "server-only";
import {packageClockSql} from '@erpx/server-commercial/public/purchased-capability-sql';
/** Byte-identical purchased-capability clock CTE shared by Legal owner adapters (round 21).
 * Text only: every call site still prepares and executes its own statement with unchanged binds. */
export const legalPurchaseClockCteSql = `(SELECT ${packageClockSql} now_ms)`;
