/** Owner-local byte-identical read SQL shared across Benefits adapters. A single
 * text source keeps equivalent payroll and workforce reads from drifting apart. */
export const benefitPayrollInputsSql = "SELECT b.*,p.name plan_name FROM benefit_payroll_inputs b JOIN benefit_elections e ON e.id=b.election_id JOIN benefit_plans p ON p.id=e.plan_id WHERE b.engagement_id=? AND b.currency=? AND b.status='released' AND b.effective_from<=? AND (b.effective_to IS NULL OR b.effective_to>=?) ORDER BY b.id";
/** Shared prefix of the current-actor workforce scope probe. Consumers append either
 * ")workforce_actor_scope ..." or ",${packageClockSql} now_ms)workforce_actor_scope ...". */
export const workforceActorScopeProbe = "SELECT 1 FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id";
/** Byte-identical reviewed payroll-input ownership predicate shared by the
 * Benefits and People package-settlement fact projections. */
export const benefitPayrollInputOwnerPredicate = "SELECT 'payroll-input',h.id,h.source_election_version,h.status,to_jsonb(h),e.id IS NULL OR e.status<>'released' OR e.company_id IS DISTINCT FROM h.company_id OR e.person_id IS DISTINCT FROM h.person_id OR e.engagement_id IS DISTINCT FROM h.engagement_id OR e.version<h.source_election_version OR e.currency IS DISTINCT FROM h.currency OR e.employee_deduction_minor IS DISTINCT FROM h.employee_deduction_minor OR e.employer_cost_minor IS DISTINCT FROM h.employer_cost_minor";
/** Byte-identical owner-local election fact projection prefix shared by the
 * Benefits and People package-settlement fact unions; generated SQL unchanged. */
export const benefitElectionFactPrefix = "UNION ALL SELECT 'election',e.id,e.version,e.status,to_jsonb(e),e.status NOT IN ('released','cancelled') OR";
