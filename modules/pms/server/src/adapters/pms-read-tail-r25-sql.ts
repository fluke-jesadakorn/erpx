/** Byte-identical PMS read-tail r25 fragments shared across owner-local readers.
 * Text only: every call site still prepares/executes its own statement with its
 * own binds, filters and result type; nothing is cached or executed here.
 * Locking reads (FOR UPDATE / FOR SHARE / advisory) stay inline at call sites. */

/** Guest stays/chosen member predicate reused by record repository projections. */
export const PMS_GUEST_STAYS_CHOSEN_MEMBER_SQL = "OR (kind='guest' AND id IN (SELECT data->>'guestId' FROM stays UNION SELECT data->>'guestId' FROM chosen))";
