import "server-only";
/** Closed, byte-identical hold-guard SQL fragments shared by legal owner adapters.
 * No table, SQL, predicate or alias is accepted from a caller. */
export const legalHoldGuardPrefix = "NOT EXISTS(SELECT 1 FROM legal_hold_targets t JOIN legal_holds h ON h.id=t.hold_id WHERE t.item_id=";
export const legalHoldGuardSuffix = " AND h.state!='released')";
