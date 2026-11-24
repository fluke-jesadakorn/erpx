import 'server-only';
/** Canonical parameterized SELECT-only owner read fragments shared across People Payroll
 * adapters (G4 read-tail r28). Each builder owns one read-only literal copied verbatim from
 * its former inline copies; consumers import it and keep no local copy. Every family here
 * is a pure read (no INSERT, UPDATE, DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock)
 * and is not covered by the r19-r27 canonical modules. Lock-bearing reads stay untouched. */

/** Manager approval-chain recursive witness. `staffLevelsJoin` is the owner join fragment. */
export const ppRd28ApprovalChainSql = (staffLevelsJoin: string) => `EXISTS(WITH RECURSIVE chain AS (
      SELECT e.manager_person_id,l.level_number selected_level,ARRAY[e.person_id] seen,0 depth
      FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id ${staffLevelsJoin} ON l.id=e.level_id AND l.company_id=e.company_id
      WHERE e.company_id=? AND e.status='active' AND iam_same_person(p.user_id,?,e.company_id)
      UNION ALL SELECT e.manager_person_id,c.selected_level,c.seen||e.person_id,c.depth+1
      FROM chain c JOIN hr_engagements e ON e.person_id=c.manager_person_id AND e.company_id=? AND e.status='active'
      WHERE NOT e.person_id=ANY(c.seen) AND c.depth<100)
      SELECT 1 FROM chain c JOIN hr_people p ON p.id=c.manager_person_id AND p.company_id=? JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id AND e.status='active' ${staffLevelsJoin} ON l.id=e.level_id AND l.company_id=e.company_id
      WHERE iam_same_person(p.user_id,?,p.company_id) AND l.level_number<c.selected_level)`;

/** Lifecycle-case worker-source projection parameterized by the marked/source subqueries. */
export const ppRd28LifecycleCasesProjectionSql = (markedRows: string, sourceRows: string) =>
  `SELECT c.id,c.version,c.status,to_jsonb(c)::text image,source.proof FROM people_lifecycle_cases c JOIN (${markedRows}) marked ON marked.id=c.id AND marked.company_id=c.company_id LEFT JOIN (${sourceRows}) source ON source.id=c.id AND source.company_id=c.company_id WHERE c.company_id=? ORDER BY c.id`;

/** Open advance-repayment source-blocker projection shared by calculate and readiness reads. */
export const PP_RD28_ADVANCE_REPAYMENTS_SQL = `SELECT r.id,r.version,r.amount_minor,r.due_period_start,r.due_period_end,a.reason
      FROM hr_payroll_advance_repayments r JOIN hr_payroll_advances a ON a.id=r.advance_id
      WHERE r.company_id=? AND a.engagement_id=? AND a.status='approved' AND r.status IN ('open','reserved')
        AND r.due_period_start<=? AND r.due_period_end>=? AND (r.payroll_run_id IS NULL OR r.payroll_run_id=?)
      ORDER BY r.sequence`;

/** Contractor headcount projection shared by readiness and scheduled-preview reads. */
export const PP_RD28_CONTRACTOR_COUNT_SQL = `SELECT COUNT(*) AS count FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.pay_group_id=? AND e.worker_type='contractor' AND e.status IN ('active','ended') AND p.status='active' AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?)`;

/** Draft/submitted time-entry headcount projection shared by readiness and scheduled-preview reads. */
export const PP_RD28_PENDING_TIME_COUNT_SQL = `SELECT COUNT(*) AS count FROM hr_time_entries t JOIN hr_engagements e ON e.id=t.engagement_id WHERE e.company_id=? AND e.pay_group_id=? AND t.work_date BETWEEN ? AND ? AND t.status IN ('draft','submitted')`;

/** Non-merged person legal-name IN-list projection. `inList` is the caller's bind placeholders. */
export const ppRd28PersonLegalNamesSql = (inList: string) =>
  `SELECT user_id,legal_name FROM hr_people WHERE company_id=? AND merged_into_id IS NULL AND user_id IN (${inList})`;

/** Payroll source-owner existence witness; `idSql` is a raw id expression, `extra` a predicate. */
export const ppRd28SourceExistsByIdSql = (table: string, idSql: string, extra: string) =>
  `EXISTS(SELECT 1 FROM ${table} s WHERE s.id=${idSql}${extra})`;
