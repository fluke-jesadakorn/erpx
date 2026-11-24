/** Owner-local canonical SQL fragments shared by the Customer Authority TS read tail.
 *  One text source keeps repeated projection subselects and binding probes
 *  byte-identical across every caller. A builder's only input is a closed
 *  alias or predicate chosen in this package, never an untrusted value.
 *  Control-plane lock statements and the integrated portal read port are not
 *  part of these read-only families; the TS consumers remain intact. */

/** JSONB fact projection shared by the customer document and Support enrichment
 *  reads. Callers append their own ` FROM ...` source so the rendered SQL stays
 *  exactly the same as the pre-dedupe literals. */
export const jsonbAggFact = (alias: string) =>
  `(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id)`;

/** Bound existence probe over the local `bindings` CTE used by the customer
 *  document projection. The predicate is fixed at each call site. */
export const bindingsExists = (predicate: string) =>
  `EXISTS(SELECT 1 FROM bindings b WHERE ${predicate})`;
