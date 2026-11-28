/** Byte-identical PMS read-tail r24 SQL fragment shared across owner-local readers.
 * Text only: every call site still prepares/executes its own statement with its own
 * binds, filters and result type; nothing is cached or executed here.
 * Locking reads (FOR UPDATE / FOR SHARE / advisory) stay inline at call sites. */

/** Flat pms_records point/set projection reused by dashboard, configuration and command readers.
 * The trailing WHERE/UNION/filters stay call-site owned. */
export const PMS_RECORDS_PROJECTION_PREFIX_SQL = "SELECT id,kind,data_json,version,updated_at FROM pms_records";
