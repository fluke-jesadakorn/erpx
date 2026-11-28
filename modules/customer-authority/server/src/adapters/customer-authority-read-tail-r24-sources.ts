/** One canonical binding-joined fact projection shared by the Customer Authority
 *  package-obligation facts UNION (G4 read-tail r24). The kind/id literals, source
 *  table/alias, pending predicate and optional join tail are closed, owner-local
 *  strings; every rendered fragment stays byte-identical to its pre-dedupe
 *  literal. Callers keep their own prepared statement and binds. This family is
 *  a pure read: no writes, no row/table locks and no advisory locks. The r21
 *  `customer-authority-read-sql`, r22 `package-obligation-sql` and r23
 *  `jsonbImageProjection` builders, all source locks and native-owned kinds stay
 *  untouched. */
export const bindingFactProjection = (
  kind: string,
  id: string,
  table: string,
  alias: string,
  pending: string,
  tail = `JOIN bindings binding ON binding.id=${alias}.binding_id`,
) => `SELECT '${kind}',${id},to_jsonb(${alias}),${pending} FROM ${table} ${alias} ${tail}`;
