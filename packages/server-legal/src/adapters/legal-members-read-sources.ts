import "server-only";

/** Closed, byte-identical legal_members read fragments shared by owner adapters.
 * No table, SQL, predicate or alias is accepted from a caller. */
export const legalMembersSnapshotSource = "FROM (SELECT \"item_id\", \"user_id\", \"access\" FROM legal_members) m";
export const legalMembershipSql = "SELECT 1 FROM legal_members WHERE item_id=? AND user_id=?";
export const legalMembershipExistsSql = `EXISTS(${legalMembershipSql})`;
