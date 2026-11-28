/** Byte-identical PMS read-tail r17 SQL shared across owner-local context readers.
 * Text only: every call site still prepares and executes its own statement with
 * its own binds and result type; nothing is cached or executed here.
 * Locking reads (FOR UPDATE / advisory) are intentionally not part of this set. */

/** Write-capable transaction context guard used by point reads before mutations. */
export const PMS_WRITE_CONTEXT_SQL = "SELECT set_config('erpx.pms_write','on',true)";
