import 'server-only';
import { ppRdRound5ExistsByIdAndStatus } from './pp-read-tail-round5-sources';
/** Canonical byte-identical SELECT-only owner fragments shared across People Payroll
 * adapters (G4 read-tail r19). Each constant owns one literal copied verbatim from its
 * former inline copies; consumers import it and keep no local copy. All are pure read
 * guards/projections: no INSERT, UPDATE, DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock. */
/** engagement id/version guard literal */
export const PP_RD19_HR_ENGAGEMENT_VERSION_EXISTS_SQL = "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=?)";
/** 16-column hr_people projection aliased p */
export const PP_RD19_HR_PEOPLE_P_FROM_SQL = "FROM (SELECT \"id\", \"user_id\", \"company_id\", \"legal_name\", \"preferred_name\", \"email\", \"phone\", \"country_code\", \"status\", \"version\", \"created_by\", \"created_at\", \"updated_at\", \"merged_into_id\", \"work_email\", \"work_phone\" FROM hr_people) p";
/** open evaluation cycle guard literal */
export const PP_RD19_EVAL_DEF_OPEN_EXISTS_SQL = "EXISTS(SELECT 1 FROM hr_evaluation_definitions WHERE id=? AND version=? AND payload_json::jsonb->>'status'='open')";
/** active engagement id/version guard literal */
export const PP_RD19_ACTIVE_ENGAGEMENT_EXISTS_SQL = ppRdRound5ExistsByIdAndStatus('hr_engagements', 'version', 'active');
/** jsonb array membership predicate builder */
export const ppRd19InList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
