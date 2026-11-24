/** Shared workforce actor-scope CTE head for the r31 identity_access retire wave.
 * The admin workspace role actor and the private attachment reader used the same
 * fixed head; each keeps its own tail, binds and transaction at the call site.
 * `where` is the caller's already-authorized scope predicate, never caller SQL.
 * No lock, write, session authority, native kind or earlier-round helper changes. */
export const workforceActorScopeSql = (where: string): string =>
  `SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${where}`;
