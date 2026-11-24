/** Owner-local actor scope CTE shared by Recruitment read guards. The three
 * binds are company_id, branch_id and department_id in that order. */
export const talentActorScopeCte = "(SELECT ?::text company_id,?::text branch_id,?::text department_id)";
