import "server-only";
/** Owner-local child-row JSON aggregate shared by Quality attachment and
 * package-obligation reads. Args: table, row alias, foreign-key column, parent
 * expression, optional ordering expression. The generated SQL is byte-identical
 * to the former inline copies when the same aliases are supplied. */
export const qualityChildAggSql = (
  table: string,
  alias: string,
  key: string,
  parent: string,
  order = `${alias}.id`,
) => `(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${order}) FROM ${table} ${alias} WHERE ${alias}.${key}=${parent})`;
