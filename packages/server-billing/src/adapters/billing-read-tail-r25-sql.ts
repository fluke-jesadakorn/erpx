/** Byte-identical Billing advisory-lock SQL text shared by the owner adapters.
 * Text only: every call site still prepares and executes with its own bind. */
const billingAdvisoryLockSql = (fn: "pg_advisory_xact_lock" | "pg_advisory_unlock") =>
  `SELECT ${fn}(hashtextextended($1,0))`;
export const BILLING_ADVISORY_XACT_LOCK_SQL = billingAdvisoryLockSql("pg_advisory_xact_lock");
export const BILLING_ADVISORY_UNLOCK_SQL = billingAdvisoryLockSql("pg_advisory_unlock");
