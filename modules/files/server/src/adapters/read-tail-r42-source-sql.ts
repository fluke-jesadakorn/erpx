/** r42: byte-identical, read-only SQL heads shared across files owner adapters.
 * Text only: every call site still prepares and executes its own statement in
 * its supplied database with its own binds; no SQL text is accepted from a
 * caller and no result is cached here. Advisory locks, row locks, LOCK TABLE,
 * native writes and every r1-r41 helper are left untouched. */

/** Shared set-row JOIN head used by the compatibility-set witness (app sources)
 * and the two leave-review evidence guard witnesses. Each site passes its own
 * joined-table fragment, trailing predicate and binds, so the original SQL
 * tokens and all parameters stay at the call sites. */
export const SET_ROW_JOIN_HEAD_SQL = 'SELECT 1 FROM attachment_sets set_row JOIN ';

/** Shared storage-ledger object-key projection head used by the reconciliation
 * duplicate-key witness and the service repository ledger read. Trailing
 * predicates and binds stay at each site. Distinct from the r41 object_key=?
 * presence head, which remains at its own sites. */
export const LEDGER_OBJECT_KEY_PROJECTION_HEAD_SQL = 'SELECT object_key FROM attachment_storage_ledger WHERE ';
