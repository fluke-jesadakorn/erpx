import 'server-only';
import {packageClockSql} from '@erpx/server-commercial/public/purchased-capability-sql';
/** Canonical clock CTE projection shared by Operations owner reads. */
export const OPERATIONS_CLOCK_CTE_SQL = `(SELECT ${packageClockSql} now_ms)`;
/** Canonical bound viewer/assignment clock CTE shared by Operations owner reads. */
export const OPERATIONS_VIEWER_CLOCK_SQL = `(SELECT ?::text user_id,?::text assignment_id,${packageClockSql} now_ms)`;
/** Canonical task scope projection shared by Operations owner reads. */
export const OPERATIONS_TASK_SCOPE_SQL = '(SELECT t.company_id,t.branch_id,t.department_id)';
/** Canonical bound actor-scope input projection shared by Operations owner reads. */
export const OPERATIONS_ACTOR_SCOPE_INPUT_SQL = '(SELECT ?::text company_id,?::text branch_id,?::text department_id)';
/** Canonical current-workforce scope existence clause shared by Operations owner reads. */
export const workforceActorScopeSql = (where: string) => `SELECT 1 FROM${OPERATIONS_ACTOR_SCOPE_INPUT_SQL}workforce_actor_scope WHERE ${where}`;
