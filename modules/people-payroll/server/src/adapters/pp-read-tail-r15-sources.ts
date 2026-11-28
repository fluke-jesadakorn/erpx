import 'server-only';
/** Canonical byte-identical SELECT-only owner reads shared across People Payroll
 * adapters (G4 read-tail r15). Each constant owns one literal; a consumer imports
 * the constant and keeps no local copy. Both remaining literals are pure reads:
 * no INSERT, UPDATE, DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock. The
 * former receipt-replay and active-payee reads are now owner-native kinds in
 * `pp-read-tail-r53-native`. */
export const PP_RD15_FINAL_PAY_VALID_SQL = 'SELECT hr_final_pay_cancellation_valid(?) valid';
export const PP_RD15_PEOPLE_SNAPSHOT_SQL = "SELECT id,company_id,user_id,status,version,merged_into_id FROM hr_people";
