import 'server-only';
import { ppRdRound9ProjectionFragment } from './pp-read-tail-round9-sources';
/** Canonical byte-identical SELECT-only owner projections shared across People Payroll
 * adapters (G4 read-tail r18). Each constant owns one literal copied verbatim from its
 * former inline copies; consumers import it and keep no local copy. All are pure read
 * projections: no INSERT, UPDATE, DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock. */
/** FROM (hr_people projection, alias hr_people) */
export const PP_RD18_HR_PEOPLE_FROM_SQL = "FROM (SELECT \"id\", \"user_id\", \"company_id\", \"legal_name\", \"preferred_name\", \"email\", \"phone\", \"country_code\", \"status\", \"version\", \"created_by\", \"created_at\", \"updated_at\", \"merged_into_id\", \"work_email\", \"work_phone\" FROM hr_people) hr_people";
/** The 21-column hr_engagements projection, byte-identical in JOIN and FROM forms. */
const HR_ENGAGEMENT_COLUMNS = '"id", "person_id", "company_id", "branch_id", "department_id", "worker_type", "employment_type", "status", "start_date", "end_date", "manager_person_id", "job_title", "currency", "version", "created_by", "created_at", "updated_at", "pay_group_id", "level_id", "position_id", "self_service_top_level"';
/** JOIN (21-col hr_engagements projection, alias e) */
export const PP_RD18_HR_ENGAGEMENTS_E_JOIN_SQL = ppRdRound9ProjectionFragment('JOIN', HR_ENGAGEMENT_COLUMNS, 'hr_engagements', 'e');
/** FROM (21-col hr_engagements projection, alias e) */
export const PP_RD18_HR_ENGAGEMENTS_E_FROM_SQL = ppRdRound9ProjectionFragment('FROM', HR_ENGAGEMENT_COLUMNS, 'hr_engagements', 'e');
/** compact quoted hr_people projection */
export const PP_RD18_HR_PEOPLE_PROJ_SQL = "SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people";
