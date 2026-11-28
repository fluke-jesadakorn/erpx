/** Byte-identical PMS read-tail r21 SQL fragment shared across owner-local source readers.
 * Text only: every call site still prepares its own statement with its own binds,
 * filters and result type; nothing is cached or executed here.
 * Locking reads (FOR UPDATE / advisory) are intentionally not part of this set. */

/** Parameterized recipient join used by source-scoped point and overview reads. */
export const PMS_RECIPIENT_CROSS_JOIN_SQL = "CROSS JOIN (SELECT ?::text user_id) recipient";
