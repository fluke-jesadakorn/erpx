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
/** Owner-local child-row JSON aggregate that reaches its parent through a
 * JOIN on inventory_quality_inspections. Derived from qualityChildAggSql so the
 * canonical aggregate shape stays single-sourced; only the predicate changes.
 * Args: table, row alias, ordering expression, inspection join key, hold
 * parent expression. Byte-identical to the former inline joined copies. */
export const qualityJoinedChildAggSql = (
  table: string,
  alias: string,
  order: string,
  joinKey: string,
  parent: string,
) => qualityChildAggSql(table, alias, joinKey, parent, order)
  .replace(` WHERE ${alias}.${joinKey}=${parent})`,
    ` JOIN inventory_quality_inspections i ON i.id=${alias}.${joinKey} WHERE i.hold_id=${parent})`);
