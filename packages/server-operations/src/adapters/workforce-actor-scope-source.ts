import 'server-only';
import {packageClockSql,purchasedCapabilityPredicate} from '@erpx/server-commercial/public/purchased-capability-sql';
import {scheduledFeaturePredicate} from '@erpx/server-pms/public/feature-availability';
import {OPERATIONS_SELECT_1_FROM} from './read-tail-r56-sources';
const operationsClockCte = (bound: boolean) => `(SELECT ${bound ? '?::text user_id,?::text assignment_id,' : ''}${packageClockSql} now_ms)`;
/** Canonical clock CTE projection shared by Operations owner reads. */
export const OPERATIONS_CLOCK_CTE_SQL = operationsClockCte(false);
/** Canonical bound viewer/assignment clock CTE shared by Operations owner reads. */
export const OPERATIONS_VIEWER_CLOCK_SQL = operationsClockCte(true);
/** Canonical viewer-clock EXISTS head shared by Operations owner reads. The
 *  viewer alias and FROM separator are the only varying bytes; the clock CTE,
 *  table and predicates stay local to each caller. Binds unchanged. The default
 *  newline/space separator preserves the r49-r53 call-site rendering. */
export const operationsViewerClockHead = (viewer: string, sep = '\n ') =>
  `EXISTS(WITH ${viewer} AS MATERIALIZED${OPERATIONS_VIEWER_CLOCK_SQL}${sep}SELECT 1 FROM `;
/** Canonical task scope projection shared by Operations owner reads. */
export const OPERATIONS_TASK_SCOPE_SQL = '(SELECT t.company_id,t.branch_id,t.department_id)';
/** Canonical bound actor-scope input projection shared by Operations owner reads. */
export const OPERATIONS_ACTOR_SCOPE_INPUT_SQL = '(SELECT ?::text company_id,?::text branch_id,?::text department_id)';
/** Canonical current-workforce scope existence clause shared by Operations owner reads. */
export const workforceActorScopeSql = (where: string) => operationsActorScopeInputExists(where, '', '');
/** Canonical materialized-clock existence probe shared by Operations owner predicate families. */
export const operationsClockProbe = (name: string) => `WITH ${name} AS MATERIALIZED${OPERATIONS_CLOCK_CTE_SQL} ${OPERATIONS_SELECT_1_FROM(name)}`;
/** Canonical actor-scope input projection with caller-supplied segments. Company,
 *  branch, department and clock segments are the only varying bytes; every
 *  rendering is byte-identical to the original inline literals. */
export const operationsActorScopeInput = (company: string, branch: string, department: string, clock = '') =>
  `(SELECT ${company},${branch},${department}${clock})`;
/** Canonical bound actor-scope existence clause preserving original spacing.
 *  The FROM and table separators and the predicate are the only varying bytes. */
export const operationsActorScopeInputExists = (where: string, fromSep = ' ', tableSep = ' ') =>
  `SELECT 1 FROM${fromSep}${OPERATIONS_ACTOR_SCOPE_INPUT_SQL}${tableSep}workforce_actor_scope WHERE ${where}`;
/** Canonical purchased Work actor-scope clock probe shared by the two record
 * workspace guards. The scope input segments and the current actor scope are
 * the only varying bytes; the purchased-history and feature clauses are fixed. */
export const operationsPurchasedActorScopeClockExists = (company: string, branch: string, department: string, where: string) =>
  `EXISTS(SELECT 1 FROM${operationsActorScopeInput(company, branch, department, `,${packageClockSql} now_ms`)}workforce_actor_scope WHERE ${where} AND ${purchasedCapabilityPredicate('workforce_actor_scope.company_id', 'work.actions', 'history', 'workforce_actor_scope.now_ms')} AND ${scheduledFeaturePredicate('workforce_actor_scope.company_id', 'operations')})`;
