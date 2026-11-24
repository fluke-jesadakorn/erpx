/** Byte-identical PMS read-tail r25 fragments shared across owner-local readers.
 * Text only: every call site still prepares/executes its own statement with its
 * own binds, filters and result type; nothing is cached or executed here.
 * Locking reads (FOR UPDATE / FOR SHARE / advisory) stay inline at call sites. */

/** Guest-account member predicate reused by scoped command readers. */
export const PMS_GUEST_ACCOUNT_ID_MEMBER_SQL = "AND r.data_json::jsonb->>'guestAccountId' IN (SELECT id FROM account_ids)";

/** Guest-account cashier accounts CTE member reused across scoped command readers. */
export const PMS_GUEST_ACCOUNT_CASHIER_MEMBER_SQL = "WHERE r.property_id=q.property_id AND r.kind='guest_account' AND r.id IN (SELECT id FROM account_ids)";

/** Reservation member predicate reused by scoped booking command readers. */
export const PMS_RESERVATION_ID_MEMBER_SQL = "AND r.data_json::jsonb->>'reservationId' IN (SELECT id FROM booking_ids)";

/** Guest stays/chosen member predicate reused by record repository projections. */
export const PMS_GUEST_STAYS_CHOSEN_MEMBER_SQL = "OR (kind='guest' AND id IN (SELECT data->>'guestId' FROM stays UNION SELECT data->>'guestId' FROM chosen))";
