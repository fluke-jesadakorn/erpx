import "server-only";
/** Owner-local parameterized ordered JSONB aggregate subquery shared by the Planning
 * attachment lineage source. Every argument is a fixed server-owned projection,
 * ordering, FROM or predicate fragment; no bind value or caller SQL is
 * interpolated. The FROM fragment carries its own leading whitespace so each
 * emitted subquery is byte-identical to the former inline literal it replaces. */
export const planningJsonbAggOrdered = (projection: string, order: string, from: string, predicate: string) =>
  `(SELECT jsonb_agg(${projection} ORDER BY ${order})${from} WHERE ${predicate})`;
