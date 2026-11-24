import "server-only";
/** Byte-identical read fragments shared by Legal owner adapters (round 18).
 * Text only: every call site still prepares and executes with its own predicate and binds. */
export const legalRetentionPolicySelectSql = "SELECT * FROM legal_retention_policies WHERE id=? AND company_id=?";
export const legalActorScopeProjectionSql = "SELECT ?::text company_id,?::text branch_id,?::text department_id";
export const legalEventsByItemSql = "SELECT * FROM legal_events WHERE item_id=? ORDER BY created_at";
