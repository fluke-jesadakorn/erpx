/** G4 / retire.crm.read-tail-r44 byte-identical UNION-ALL head.
 *
 * The owner-local fact arms share one renderer. Callers keep their leading
 * indentation and pass only the projection/predicate body (including their own
 * cross-owner sources, kind interpolations and nested probes), so rendered SQL,
 * whitespace and bind order stay byte-identical to the retired inline literals.
 * Plain heads, MATERIALIZED heads, the r42 activity NOT EXISTS and the
 * r8-r43 families are untouched.
 */
export const crmUnionAllSql = (body: string) => `UNION ALL SELECT ${body}`;
