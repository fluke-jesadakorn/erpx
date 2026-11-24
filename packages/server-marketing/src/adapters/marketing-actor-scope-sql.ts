/** Owner-local current-actor scope subquery shared by Marketing assignment guards.
 * One literal keeps the emitted SQL byte-identical at every bind site. */
export const marketingActorScopeSql = '(SELECT ?::text company_id,?::text branch_id,?::text department_id)';
