import "server-only";
/** Owner-local parameterized single-JSON-row subquery shared by the Planning attachment
 * lineage source. Every argument is a fixed server-owned identifier or predicate fragment;
 * no bind value or caller SQL is interpolated. Each emitted subquery is byte-identical to
 * the former inline literal it replaces. */
export const planningJsonbRowByKey = (table: string, alias: string, predicate: string) =>
  `(SELECT to_jsonb(${alias}) FROM ${table} ${alias} WHERE ${predicate})`;
