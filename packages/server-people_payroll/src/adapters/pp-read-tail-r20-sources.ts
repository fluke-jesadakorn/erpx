import 'server-only';
/** Canonical byte-identical SELECT-only owner fragments shared across People Payroll
 * adapters (G4 read-tail r20): roster assignment point reads, payroll posting/run
 * selects and snapshot projection tails. Each constant owns one literal copied verbatim
 * from its former inline copies; consumers import it and keep no local copy. All are pure
 * reads: no INSERT, UPDATE, DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock. */
export const PP_RD20_ROSTER_ASSIGN_BY_ID_SQL = "SELECT * FROM people_roster_assignments WHERE id=?";
export const PP_RD20_ROSTER_ASSIGN_BY_SHIFT_SQL = "SELECT * FROM people_roster_assignments WHERE shift_id=? AND state='assigned'";
export const PP_RD20_PAYROLL_RUNS_SQL = "SELECT * FROM hr_payroll_runs";
export const PP_RD20_PAYROLL_POSTINGS_SQL = "SELECT * FROM hr_payroll_postings";
export const PP_RD20_STATUTORY_REMITTANCES_SQL = "SELECT * FROM hr_payroll_statutory_remittances";
export const PP_RD20_PAYROLL_ITEMS_SQL = "SELECT * FROM hr_payroll_items";
export const PP_RD20_PAYMENT_BATCHES_SQL = "SELECT * FROM hr_payment_batches";
export const PP_RD20_PAYMENT_BATCH_ITEMS_SQL = "SELECT * FROM hr_payment_batch_items";
export const PP_RD20_PAYROLL_PAYMENT_SETTLEMENTS_SQL = "SELECT * FROM hr_payroll_payment_settlements";
export const PP_RD20_PAYROLL_RECOVERIES_SQL = "SELECT * FROM hr_payroll_recoveries";
export const PP_RD20_PAYROLL_ADVANCES_SQL = "SELECT * FROM hr_payroll_advances";
export const PP_RD20_PAYROLL_ADVANCE_REPAYMENTS_SQL = "SELECT * FROM hr_payroll_advance_repayments";
export const PP_RD20_HR_ENGAGEMENTS_FROM_SQL = "FROM (SELECT \"id\", \"person_id\", \"company_id\", \"branch_id\", \"department_id\", \"worker_type\", \"employment_type\", \"status\", \"start_date\", \"end_date\", \"manager_person_id\", \"job_title\", \"currency\", \"version\", \"created_by\", \"created_at\", \"updated_at\", \"pay_group_id\", \"level_id\", \"position_id\", \"self_service_top_level\" FROM hr_engagements) hr_engagements";
export const PP_RD20_LIFECYCLE_CASES_C_FROM_SQL = "FROM (SELECT \"id\", \"company_id\", \"person_id\", \"engagement_id\", \"kind\", \"status\", \"version\", \"effective_at\", \"package_id\", \"package_revision\", \"attempts\", \"next_attempt_at\", \"last_error\", \"definition\", \"review_snapshot\", \"authorizing_assignment_id\", \"created_by\", \"updated_at\", \"departure_reason\", \"workflow\" FROM people_lifecycle_cases) c";
export const PP_RD20_LIFECYCLE_CASES_C_JOIN_SQL = "JOIN (SELECT \"id\", \"company_id\", \"person_id\", \"engagement_id\", \"kind\", \"status\", \"version\", \"effective_at\", \"package_id\", \"package_revision\", \"attempts\", \"next_attempt_at\", \"last_error\", \"definition\", \"review_snapshot\", \"authorizing_assignment_id\", \"created_by\", \"updated_at\", \"departure_reason\", \"workflow\" FROM people_lifecycle_cases) c";
export const PP_RD20_APPLIED_PACKAGES_AP_FROM_SQL = "FROM (SELECT \"id\", \"company_id\", \"person_id\", \"engagement_id\", \"position_id\", \"package_id\", \"package_revision\", \"definition\", \"source_kind\", \"scope\", \"case_id\", \"operation_id\", \"authorizing_assignment_id\", \"valid_from\", \"valid_until\", \"ended_at\", \"created_at\" FROM people_applied_packages) ap";
export const PP_RD20_HR_PEOPLE_H_JOIN_SQL = "JOIN (SELECT \"id\", \"user_id\", \"company_id\", \"legal_name\", \"preferred_name\", \"email\", \"phone\", \"country_code\", \"status\", \"version\", \"created_by\", \"created_at\", \"updated_at\", \"merged_into_id\", \"work_email\", \"work_phone\" FROM hr_people) h";
export const PP_RD20_HR_PEOPLE_P_COMPACT_JOIN_SQL = "JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p";
export const PP_RD20_HR_ENGAGEMENTS_COMPACT_SQL = "SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\",\"level_id\",\"position_id\",\"self_service_top_level\" FROM hr_engagements";
