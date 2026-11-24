/** Byte-identical PMS read-tail r26 SQL fragments shared across owner-local readers.
 * Text only: every call site still prepares/executes its own statement with its
 * own binds, filters and result type; nothing is cached or executed here.
 * Locking reads (FOR UPDATE / FOR SHARE / advisory) stay inline at call sites. */

/** JSON-array membership subquery reused by id-source readers. */
export const PMS_IDS_JSONB_MEMBER_SQL = "(SELECT jsonb_array_elements_text(?::jsonb))";

/** Flat pms_records chosen-source projection reused by record repository CTEs. */
export const PMS_CHOSEN_RECORDS_SQL = `SELECT id,kind,data_json::jsonb data FROM pms_records WHERE property_id=? AND id IN ${PMS_IDS_JSONB_MEMBER_SQL}`;

/** Guest-id source subqueries reused by scoped command readers. */
export const PMS_GUEST_ID_FROM_ACCOUNTS_SQL = "SELECT data_json::jsonb->>'guestId' FROM accounts";
export const PMS_GUEST_ID_FROM_STAYS_SQL = "SELECT data_json::jsonb->>'guestId' FROM stays";
