/** Byte-identical PMS read-tail r23 SQL fragment shared across owner-local readers.
 * Text only: every call site still prepares/executes its own statement with its own
 * binds, filters and result type; nothing is cached or executed here.
 * Locking reads (FOR UPDATE / FOR SHARE / advisory) stay inline at call sites. */

/** Scoped pms_records point/set projection reused by the command CTE readers.
 * The `scope q` CTE and the trailing WHERE/filters stay call-site owned. */
export const PMS_SCOPED_RECORDS_SQL = "SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q";
