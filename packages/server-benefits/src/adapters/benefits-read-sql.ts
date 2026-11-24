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
/** Byte-identical owner-local released-election handoff guard prefix shared by
 * the Benefits and People package-settlement fact projections; generated SQL
 * is unchanged. */
export const benefitReleasedElectionHandoffPrefix = " OR win.id IS NULL OR win.company_id IS DISTINCT FROM e.company_id OR win.version<e.window_version\n OR (e.status='released' AND (e.released_by IS NULL OR NOT EXISTS(SELECT 1 FROM benefit_payroll_inputs handoff";
/** Byte-identical owner-local scope CTE prefix shared by the Benefits and People
 * package-settlement fact queries; generated SQL is unchanged. */
export const benefitScopePrefix = "WITH scope AS(SELECT ?::text company_id,";
/** Byte-identical owner-local window fact projection prefix shared by the
 * Benefits and People package-settlement fact unions; generated SQL is unchanged. */
export const benefitWindowFactPrefix = "UNION ALL SELECT 'window',w.id,w.version,w.status,to_jsonb(w),";
/** Byte-identical owner-local dependent fact projection prefix shared by the
 * Benefits and People package-settlement fact unions; generated SQL is unchanged. */
export const benefitDependentFactPrefix = "UNION ALL SELECT 'dependent',d.id,d.version,d.status,to_jsonb(d),d.status='pending'";
/** Byte-identical owner-local plan fact projection prefix shared by the
 * Benefits and People package-settlement fact unions; generated SQL is unchanged. */
export const benefitPlanFactPrefix = "SELECT 'plan' kind,p.id,p.version,p.status state,to_jsonb(p) image,";
/** Parameterized owner-local aggregate metric projection shared by the two
 *  Benefits executive metric queries; generated SQL is unchanged. */
export const benefitMetricCountSql = (label: string, table: string, alias: string, status: string, group: string, where: string) =>
  `SELECT ${label} label,count(*)::text value FROM ${table}${alias ? ` ${alias}` : ''} WHERE ${alias ? `${alias}.` : ''}company_id=? AND ${where} AND ${alias ? `${alias}.` : ''}${status} AND ${alias ? `${alias}.` : ''}effective_from<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (${alias ? `${alias}.` : ''}effective_to IS NULL OR ${alias ? `${alias}.` : ''}effective_to>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))${group}`;
/** Parameterized owner-local final fact projection shared by the Benefits
 *  and People package-settlement fact unions; generated SQL is unchanged. */
export const benefitFactFinalSelect = (pending: string, tail: string) =>
  `) SELECT kind,id,version,state,image,${pending} pending FROM facts${tail}`;
/** Parameterized owner-local learning fact projection shared by the two
 *  Benefits learning-consumption settlement UNION arms; generated SQL is unchanged. */
export const benefitLearningFactArm = (kind: string, id: string, version: string, state: string, image: string, tail: string) =>
  `UNION ALL SELECT '${kind}',${id},${version},${state},${image},${tail}`;
/** Parameterized owner-local existence guard shared by the sensitive-attachment
 *  version guard and the payroll-source fact guard; generated SQL is unchanged. */
export const benefitOwnerExistsGuard = (table: string, conditions: string) =>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND ${conditions})`;
