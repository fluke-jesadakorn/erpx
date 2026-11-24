import 'server-only';
/** Canonical byte-identical SELECT-only owner reads shared across People Payroll
 * adapters (G4 read-tail r15). Each constant owns one literal; a consumer imports
 * the constant and keeps no local copy. All six are pure reads: no INSERT, UPDATE,
 * DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock. */
export const PP_RD15_COMMAND_RECEIPT_PRIOR_SQL = "SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?";
export const PP_RD15_ACTIVE_PAYEE_SQL = "SELECT id,version,legal_name,preferred_name FROM hr_people WHERE id=? AND company_id=? AND status='active' AND merged_into_id IS NULL";
export const PP_RD15_FINAL_PAY_VALID_SQL = 'SELECT hr_final_pay_cancellation_valid(?) valid';
export const PP_RD15_PEOPLE_SNAPSHOT_SQL = "SELECT id,company_id,user_id,status,version,merged_into_id FROM hr_people";
