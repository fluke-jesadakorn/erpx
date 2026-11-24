import 'server-only';
/** Canonical bound actor-scope input projection shared by Operations owner reads. */
export const OPERATIONS_ACTOR_SCOPE_INPUT_SQL = '(SELECT ?::text company_id,?::text branch_id,?::text department_id)';
/** Canonical current-workforce scope existence clause shared by Operations owner reads. */
export const workforceActorScopeSql = (where: string) => `SELECT 1 FROM${OPERATIONS_ACTOR_SCOPE_INPUT_SQL}workforce_actor_scope WHERE ${where}`;
