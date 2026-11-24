import "server-only";
/** Byte-identical parameterized feature-search read head shared by the Legal
 * feature count and list reads. Callers supply only the fixed projection and
 * the trusted fact table; no table, alias or SQL is accepted from a request. */
export const legalFeatureSearchSql = (projection: string, table: string) =>
  `SELECT ${projection} FROM ${table} WHERE item_id=? AND (?='' OR strpos(lower(title||' '||data::text),lower(?))>0)`;
