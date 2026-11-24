import 'server-only';
/** Canonical parameterized SELECT-only owner read builders shared across People Payroll
 * adapters (G4 read-tail r43). Each builder owns one read-only literal copied verbatim from
 * its former inline copies; consumers import it and keep no local copy. Every family here
 * is a pure read (no INSERT/UPDATE/DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock)
 * and is not covered by the r1-r42 canonical modules. Lock-bearing reads, native kinds,
 * cross-owner projections and r1-r42 modules stay untouched. */

/** Policy-versioned AI reservation source head; no binds. */
export const ppRd43AiReservationProjection = (cols: string) =>
  `SELECT ${cols} FROM expense_ai_proposals WHERE finance_policy_version IS NOT NULL`;

/** Active employee engagement head with the cost-evidence/scope tail; bind order is
 * (company_id, pay_group_id, currency, period_end, period_start, scope_id, scope_id2). */
export const ppRd43EmployeeEngagementProjection = (cols: string) =>
  `SELECT ${cols} FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.pay_group_id=? AND e.currency=? AND e.worker_type='employee' AND e.status IN ('active','ended') AND p.status='active' AND p.merged_into_id IS NULL AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?) AND (?::text IS NULL OR e.id=?) ORDER BY e.id`;

/** Unreleased draft payment point; bind order is (id, expense_id, company_id). */
export const ppRd43DraftPaymentProjection = (cols: string) =>
  `SELECT ${cols} FROM expense_payment_drafts WHERE id=? AND expense_id=? AND company_id=? AND status='draft'`;

/** Effective access-cutoff point; bind order is ($1 company_id, $2 person_id, $3 now). */
export const ppRd43EffectiveCutoffProjection = (cols: string) =>
  `SELECT ${cols} FROM people_access_cutoffs WHERE company_id=$1 AND person_id=$2 AND cancelled_at IS NULL AND restored_at IS NULL AND effective_at<=$3`;

/** Active leave-policy list; bind is (company_id). */
export const ppRd43ActiveLeavePolicyProjection = (cols: string) =>
  `SELECT ${cols} FROM leave_policy_versions WHERE company_id=? AND status='active' ORDER BY name,version DESC`;
