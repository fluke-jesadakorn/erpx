import { PP_RD20_HR_ENGAGEMENTS_COMPACT_SQL } from './pp-read-tail-r20-sources';
import "server-only";
import {PP_RD18_HR_PEOPLE_PROJ_SQL} from './pp-read-tail-r18-sources';
/** Closed native projections for commercial owner joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const commercialOwnerProjectionSources = {
  finance_expenses: "SELECT \"id\",\"company_id\",\"employee_id\",\"expense_date\",\"merchant\",\"currency\",\"amount_minor\",\"tax_minor\",\"category_account_id\",\"status\",\"journal_id\",\"evidence_json\",\"version\",\"created_by\",\"approved_by\",\"created_at\",\"updated_at\",\"branch_id\",\"department_id\",\"engagement_id\",\"business_purpose\",\"receipt_reference\",\"rejection_reason\",\"prepared_by\",\"submitted_at\",\"posted_at\",\"reimbursed_at\",\"cancelled_at\",\"invoice_id\",\"reversal_journal_id\",\"claimant_user_id\",\"claimant_name\",\"workflow_version\",\"workflow_stage\",\"accounting_json\",\"request_kind\",\"payee_party_id\" FROM finance_expenses",
  hr_engagements: PP_RD20_HR_ENGAGEMENTS_COMPACT_SQL,
  hr_people: PP_RD18_HR_PEOPLE_PROJ_SQL,
} as const;
