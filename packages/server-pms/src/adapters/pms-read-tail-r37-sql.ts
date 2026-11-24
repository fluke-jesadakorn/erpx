/** Byte-identical PMS read-tail r37 id-projection heads shared across owner-local sources.
 * Text only: every consumer keeps its own predicate, ORDER/LIMIT, binds, result type
 * and transaction; nothing is cached or executed here. Locking reads, the r17-r36
 * fragments and the native read registry stay untouched. */

/** Rank 1: stripe payment id projection head shared by four call sites. */
export const PMS_STRIPE_PAYMENT_ID_HEAD_SQL = 'SELECT id FROM pms_stripe_payments WHERE';

/** Rank 1: booking payment id projection head shared by three call sites (the r29
 * parameterized fragment stays inline at its caller). */
export const PMS_BOOKING_PAYMENT_ID_HEAD_SQL = 'SELECT id FROM pms_booking_payments WHERE';
