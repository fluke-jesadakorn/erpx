import 'server-only';
import { ppRdRound3ExistsByIdTwoPred } from './pp-read-tail-round3-sources';
import { ppRdRound4ByProjectionIdCompany } from './pp-read-tail-round4-sources';
import { ppRdRound5ExistsByIdAndStatus } from './pp-read-tail-round5-sources';
/** Canonical byte-identical SELECT-only owner read fragments shared across People Payroll
 * adapters (G4 read-tail r23). Each constant owns one parameterized literal copied verbatim
 * from its former inline copies; consumers import it and keep no local copy. All are pure
 * reads: no INSERT, UPDATE, DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock. Families
 * here are not covered by the r19-r22 canonical modules. */
export const PP_RD23_HR_PEOPLE_APPROVER_LEGAL_NAME_SQL = 'SELECT legal_name FROM hr_people WHERE company_id=? AND iam_same_person(user_id,?,company_id) AND merged_into_id IS NULL LIMIT 1';
export const PP_RD23_FINANCE_EXPENSE_DRAFT_REJECTED_GUARD = "EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND cancelled_at IS NULL AND status IN ('draft','rejected'))";
export const PP_RD23_EXPENSE_AI_USAGE_MONTHS_SQL = 'SELECT company_id,month FROM expense_ai_usage';
export const PP_RD23_HR_COMP_REVISIONS_SQL = "SELECT * FROM hr_compensation_revisions WHERE engagement_id=? AND status IN ('active','approved') AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from DESC LIMIT 1";
export const PP_RD23_HR_TIME_ENTRIES_OVERTIME_SQL = "SELECT id,version,work_date,regular_hours,overtime_hours,cost_assignment_id,cost_branch_id,cost_department_id FROM hr_time_entries WHERE engagement_id=? AND work_date BETWEEN ? AND ? AND status='approved' ORDER BY id";
export const PP_RD23_OVERTIME_PAYROLL_HANDOFFS_SQL = 'SELECT id,request_id,request_version,minutes,work_date FROM overtime_payroll_handoffs WHERE engagement_id=? AND work_date BETWEEN ? AND ? ORDER BY id';
export const PP_RD23_HR_PAYROLL_COST_ASSIGNMENTS_SQL = "SELECT id,effective_from,effective_to,branch_id,department_id,evidence_status,version FROM hr_payroll_cost_assignments WHERE engagement_id=? AND company_id=? AND status='active' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from,id";
export const PP_RD23_HR_PAY_GROUPS_ACTIVE_CURRENCY_GUARD = "EXISTS(SELECT 1 FROM hr_pay_groups WHERE id=? AND company_id=? AND currency=? AND status='active')";
export const PP_RD23_HR_ENGAGEMENTS_ACTIVE_FOR_PERSON_NOT_EXISTS = "NOT EXISTS(SELECT 1 FROM hr_engagements WHERE company_id=? AND person_id=? AND status!='ended')";
export const PP_RD23_EVAL_ASSESSMENT_VERSION_GUARD = ppRdRound3ExistsByIdTwoPred('hr_evaluation_assessments', 'version', 'status');
export const PP_RD23_LEAVE_TYPES_ID_COMPANY_EXISTS = ppRdRound4ByProjectionIdCompany('1', 'leave_types');
export const PP_RD23_HR_ENGAGEMENTS_ID_COMPANY_EXISTS = ppRdRound4ByProjectionIdCompany('1', 'hr_engagements');
export const PP_RD23_HR_PAY_GROUPS_IMAGE_SQL = 'SELECT to_jsonb(g)::text image FROM hr_pay_groups g WHERE g.id=? AND g.company_id=?';
export const PP_RD23_RECOVERY_FIRST_PERIOD_GUARD = "NOT EXISTS(SELECT 1 FROM hr_payroll_runs WHERE company_id=? AND pay_group_id=? AND adjustment_of_run_id IS NULL AND status='posted' AND period_start<=? AND period_end>=?)";
export const PP_RD23_POSTED_PAYROLL_RUN_GUARD = ppRdRound5ExistsByIdAndStatus('hr_payroll_runs', 'company_id', 'posted');
export const PP_RD23_IAM_REVIEWS_BY_ID_SQL = 'SELECT id,kind,payload,created_by,created_person FROM iam_reviews WHERE id=$1 AND company_id=$2';
export const PP_RD23_PROJECT_WORKER_ENGAGEMENT_SQL = 'SELECT * FROM hr_engagements WHERE id=? AND person_id=? AND company_id=?';
