import "server-only";
/** Owner-local id-existence probe shared by the Quality record-scope readers.
 * Callers supply only fixed owner-owned table names, an optional server-owned
 * alias and an id expression; no caller SQL is interpolated. One statement
 * template emits the exact fixed SQL each call site used before consolidation:
 * the aliased interpolated form for record/notification scope predicates and
 * the unaliased bind form for direct existence checks. */
export const qualitySourceIdExistsSql = (
  table: string,
  alias: string,
  id: string,
) => `SELECT 1 FROM ${table}${alias ? ` ${alias}` : ""} WHERE ${alias ? `${alias}.` : ""}id=${id}`;
