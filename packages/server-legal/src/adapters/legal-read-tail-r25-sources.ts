import "server-only";
/** Byte-identical Legal read fragments shared by owner adapters (round 25).
 * Text only: every call site still prepares and executes its own statement with unchanged binds. */
/** Edit-member existence witness; the caller chooses the fixed table alias. */
export const legalMemberEditExistsSql = (alias: string) =>
  `EXISTS(SELECT 1 FROM legal_members ${alias} WHERE ${alias}.item_id=i.id AND ${alias}.user_id=? AND ${alias}.access='edit')`;
/** Item-scoped jsonb aggregate projection; alias, table and order column are fixed by the caller. */
export const legalJsonbAggByItemSql = (alias: string, table: string, order = "id") =>
  `(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.${order}) FROM ${table} ${alias} WHERE ${alias}.item_id=i.id)`;
