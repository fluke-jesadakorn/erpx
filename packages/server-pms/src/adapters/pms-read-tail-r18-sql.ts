/** Byte-identical PMS read-tail r18 SQL shared across owner-local point readers.
 * Text only: every call site still prepares and executes its own statement with
 * its own binds and result type; nothing is cached or executed here.
 * Locking reads (FOR UPDATE / advisory) are intentionally not part of this set. */

/** Plain booking-payment point read; paired FOR UPDATE locks stay inline at call sites. */
export const PMS_BOOKING_PAYMENT_BY_ID_SQL = "SELECT * FROM pms_booking_payments WHERE id=?";
