/** Byte-identical parameterized actor-scope CTE shared by the Support portal
 * feature probe and the workforce private-read scope probe. Only the optional
 * branch/department columns vary; the enclosing outer projection, binds and
 * WHERE stay call-site owned. Text only: nothing is cached or executed here.
 * Read-only; no locks, purchase calls or native-kind statements move here. */
export const actorScopeCteSql = (extraColumns: string) =>
    `SELECT ?::text company_id${extraColumns}`;
