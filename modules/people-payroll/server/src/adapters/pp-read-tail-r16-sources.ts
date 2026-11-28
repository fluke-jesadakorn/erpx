import 'server-only';
/** Canonical byte-identical hr_people JOIN projection shared across People Payroll
 * snapshot-source adapters (G4 read-tail r16). The 267-character literal is copied
 * verbatim from the former inline copies; consumers import it and keep no local
 * copy. Pure read projection: no INSERT, UPDATE, DELETE, LOCK TABLE,
 * FOR UPDATE/SHARE or advisory lock. */
export const PP_RD16_HR_PEOPLE_P_JOIN_SQL = "JOIN (SELECT \"id\", \"user_id\", \"company_id\", \"legal_name\", \"preferred_name\", \"email\", \"phone\", \"country_code\", \"status\", \"version\", \"created_by\", \"created_at\", \"updated_at\", \"merged_into_id\", \"work_email\", \"work_phone\" FROM hr_people) p";
