/** The released payroll-input read (formerly `benefitPayrollInputsSql`) is now
 * owned by the native lane-32 Rust reader; see
 * `adapters/native_lane_32/postgres.rs::SOURCE_SQL`. No TS SQL remains for it. */
/** Byte-identical owner-local company-id projection head shared by the current-actor
 *  workforce scope probe and the package-settlement scope CTE; generated SQL unchanged. */
export const benefitCompanyScopeHead = "SELECT ?::text company_id,";
/** Shared prefix of the current-actor workforce scope probe. Consumers append either
 * ")workforce_actor_scope ..." or ",${packageClockSql} now_ms)workforce_actor_scope ...". */
export const workforceActorScopeProbe = `SELECT 1 FROM (${benefitCompanyScopeHead}?::text branch_id,?::text department_id`;
/** Byte-identical reviewed payroll-input ownership predicate shared by the
 * Benefits and People package-settlement fact projections. */
export const benefitPayrollInputOwnerPredicate = "SELECT 'payroll-input',h.id,h.source_election_version,h.status,to_jsonb(h),e.id IS NULL OR e.status<>'released' OR e.company_id IS DISTINCT FROM h.company_id OR e.person_id IS DISTINCT FROM h.person_id OR e.engagement_id IS DISTINCT FROM h.engagement_id OR e.version<h.source_election_version OR e.currency IS DISTINCT FROM h.currency OR e.employee_deduction_minor IS DISTINCT FROM h.employee_deduction_minor OR e.employer_cost_minor IS DISTINCT FROM h.employer_cost_minor";
/** Parameterized owner-local UNION fact-arm projection shared by the Benefits
 *  package-settlement fact unions and the learning-consumption settlement
 *  UNION arms; generated SQL is unchanged. */
export const benefitLearningFactArm = (kind: string, id: string, version: string, state: string, image: string, tail: string) =>
  `UNION ALL SELECT '${kind}',${id},${version},${state},${image},${tail}`;
/** Byte-identical owner-local election fact projection prefix shared by the
 * Benefits and People package-settlement fact unions; generated SQL unchanged. */
export const benefitElectionFactPrefix = benefitLearningFactArm('election','e.id','e.version','e.status','to_jsonb(e)',"e.status NOT IN ('released','cancelled') OR");
/** Byte-identical owner-local released-election handoff guard prefix shared by
 * the Benefits and People package-settlement fact projections; generated SQL
 * is unchanged. */
export const benefitReleasedElectionHandoffPrefix = " OR win.id IS NULL OR win.company_id IS DISTINCT FROM e.company_id OR win.version<e.window_version\n OR (e.status='released' AND (e.released_by IS NULL OR NOT EXISTS(SELECT 1 FROM benefit_payroll_inputs handoff";
/** Byte-identical owner-local scope CTE prefix shared by the Benefits and People
 * package-settlement fact queries; generated SQL is unchanged. */
export const benefitScopePrefix = `WITH scope AS(${benefitCompanyScopeHead}`;
/** Byte-identical owner-local window fact projection prefix shared by the
 * Benefits and People package-settlement fact unions; generated SQL is unchanged. */
export const benefitWindowFactPrefix = benefitLearningFactArm('window','w.id','w.version','w.status','to_jsonb(w)','');
/** Byte-identical owner-local dependent fact projection prefix shared by the
 * Benefits and People package-settlement fact unions; generated SQL is unchanged. */
export const benefitDependentFactPrefix = benefitLearningFactArm('dependent','d.id','d.version','d.status','to_jsonb(d)',"d.status='pending'");
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
/** Byte-identical owner-local EXISTS existence head shared by the owner
 *  existence guard and the Benefits settlement terminal predicate; generated
 *  SQL, binds and column references are unchanged. */
export const benefitExistsHead = (table: string, tail: string) =>
  `EXISTS(SELECT 1 FROM ${table} ${tail})`;
/** Parameterized owner-local existence guard shared by the sensitive-attachment
 *  version guard and the payroll-source fact guard; generated SQL is unchanged. */
export const benefitOwnerExistsGuard = (table: string, conditions: string) =>
  benefitExistsHead(table, `WHERE id=? AND ${conditions}`);
/** Parameterized owner-local attachment source projection shared by the plan
 *  and person-backed attachment source selects; generated SQL is unchanged. */
export const benefitAttachmentSourceSelect = (branch: string, department: string, person: string, image: string, from: string) =>
  `SELECT r.company_id,${branch},${department},r.status,${person},${image} image FROM ${from} WHERE r.id=? AND r.company_id=?`;
/** Parameterized owner-local subquery absence probe shared by the Benefits and
 *  People package-settlement fact projections; generated SQL is unchanged. */
export const benefitSubqueryNotExists = (source: string, alias: string, predicate: string) =>
  `NOT EXISTS(${benefitSourceProbe(source, alias)} WHERE ${predicate})`;
/** Parameterized owner-local source existence probe shared by the Benefits
 *  attachment file-source and native-source read scopes; generated SQL is unchanged. */
export const benefitSourceProbe = (source: string, alias: string) =>
  `SELECT 1 FROM (${source}) ${alias}`;
