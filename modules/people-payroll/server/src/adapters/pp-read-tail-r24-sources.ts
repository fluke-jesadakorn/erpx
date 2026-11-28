import 'server-only';
/** Canonical byte-identical SELECT-only owner read fragments shared across People Payroll
 * adapters (G4 read-tail r24). Each constant owns one parameterized or byte-identical literal
 * copied verbatim from its former inline copies; consumers import it and keep no local copy.
 * All are pure reads: no INSERT, UPDATE, DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock.
 * Families here are not covered by the r19-r23 canonical modules; prior constants untouched. */
export const PP_RD24_UNCLAIMED_EXPENSE_GUARD = "NOT EXISTS(SELECT 1 FROM hr_payment_batches WHERE source_type='expense' AND source_id=? AND status!='cancelled')";
export const PP_RD24_EXPENSE_AI_JOB_LEASE_GUARD = "EXISTS(SELECT 1 FROM expense_ai_proposals WHERE id=? AND state='running' AND lease_token=?)";
export const PP_RD24_PEOPLE_PERSON_USER_SAME_SQL = 'SELECT people_person_user($1,$2,$3) same';
export const PP_RD24_PEOPLE_PERSON_USER_OK_SQL = 'SELECT people_person_user($1,$2,$3) ok';
export const PP_RD24_TIME_ENTRY_REVIEWABLE_GUARD = "EXISTS(SELECT 1 FROM hr_time_entries t JOIN hr_engagements e ON e.id=t.engagement_id JOIN hr_people p ON p.id=e.person_id WHERE t.id=? AND t.company_id=? AND t.version=? AND t.status='submitted' AND t.submitted_by!=? AND (p.user_id IS NULL OR p.user_id!=?) AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?))";
export const PP_RD24_STATUTORY_PACK_GUARD = "EXISTS(SELECT 1 FROM hr_country_packs WHERE id=? AND status='active' AND record_version=? AND source_digest=?)";
export const PP_RD24_APPROVED_RUN_EVIDENCE_GUARD = "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='approved' AND version=? AND revision=? AND input_digest=?)";
export const PP_RD24_TARGET_ID_SQL = 'SELECT s.id target_id';
