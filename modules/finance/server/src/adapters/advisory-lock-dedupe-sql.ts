/** Byte-identical transaction advisory-lock text shared by the Finance owner adapters.
 * Text only: every call site still prepares and executes with its own lock key. */
export const FINANCE_ADVISORY_XACT_LOCK_SQL = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))";
