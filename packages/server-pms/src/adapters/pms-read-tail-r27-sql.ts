/** Byte-identical PMS read-tail r27 SQL fragment shared across owner-local readers.
 * Text only: every call site still prepares/executes its own statement with its own
 * binds, filters and result type; nothing is cached or executed here.
 * Locking reads (FOR UPDATE / FOR SHARE / advisory) stay inline at call sites. */

import { packageClockSql } from "@erpx/server-commercial/public/purchased-capability-sql";

/** Materialized purchase-clock CTE reused by enforced package-capability reads.
 * The CROSS JOIN and purchased-capability predicate stay call-site owned. */
export const PMS_PURCHASE_CLOCK_CTE_SQL = `WITH purchase_clock AS MATERIALIZED (SELECT ${packageClockSql} observed_at) `;
