import {marketingSelectHeadSql} from './select-head-sql';

/** Owner-local current-actor scope subquery shared by Marketing assignment guards.
 * One renderer keeps the emitted SQL byte-identical at every bind site. */
export const marketingActorScopeSql = `(${marketingSelectHeadSql('?::text company_id,?::text branch_id,?::text department_id')})`;
