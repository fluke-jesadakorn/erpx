/** G4 / retire.finance.read-tail-r35 parameterized read family.
 *
 * The constant is the byte-identical static prefix of the initially-current
 * workforce actor scope probe shared by three private attachment readers.
 * The caller owns the WHERE clause and all binds; no row-lock text, tax
 * assertion guard or r15-r34 helper is changed here. Text only: nothing is
 * cached or executed by this module.
 */
export const WORKFORCE_ACTOR_SCOPE_SQL = 'SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ';
