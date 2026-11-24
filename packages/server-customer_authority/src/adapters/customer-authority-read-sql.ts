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

/** The single owner-local probe literal behind every Customer Authority
 *  existence read (G4 read-tail r39). Callers supply only the closed `FROM`/
 *  `WHERE` tail, wrapping it in their own `EXISTS`/`NOT EXISTS` or using it
 *  bare; each rendered statement keeps its exact pre-dedupe SQL bytes, prepared
 *  statement and binds. This family is a pure read: no writes, no row/table
 *  locks and no advisory locks. The r21-r38 builders, control-plane lock text,
 *  native-owned statement kinds and manager-eligibility helpers stay untouched. */
export const selectOneProbe = (tail: string) => `SELECT 1 ${tail}`;

/** Bound existence probe over the local `bindings` CTE used by the customer
 *  document projection. The predicate is fixed at each call site. */
export const bindingsExists = (predicate: string) =>
  `EXISTS(${selectOneProbe(`FROM bindings b WHERE ${predicate}`)})`;
