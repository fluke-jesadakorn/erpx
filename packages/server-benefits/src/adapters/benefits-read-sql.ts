/** Owner-local byte-identical read SQL shared across Benefits adapters. A single
 * text source keeps equivalent payroll and workforce reads from drifting apart. */
export const benefitPayrollInputsSql = "SELECT b.*,p.name plan_name FROM benefit_payroll_inputs b JOIN benefit_elections e ON e.id=b.election_id JOIN benefit_plans p ON p.id=e.plan_id WHERE b.engagement_id=? AND b.currency=? AND b.status='released' AND b.effective_from<=? AND (b.effective_to IS NULL OR b.effective_to>=?) ORDER BY b.id";
/** Shared prefix of the current-actor workforce scope probe. Consumers append either
 * ")workforce_actor_scope ..." or ",${packageClockSql} now_ms)workforce_actor_scope ...". */
export const workforceActorScopeProbe = "SELECT 1 FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id";
