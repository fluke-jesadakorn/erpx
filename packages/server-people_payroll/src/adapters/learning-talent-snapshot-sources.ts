import 'server-only';
import {PP_RD18_HR_PEOPLE_FROM_SQL, PP_RD18_HR_ENGAGEMENTS_E_JOIN_SQL} from './pp-read-tail-r18-sources';
import {PP_RD16_HR_PEOPLE_P_JOIN_SQL} from './pp-read-tail-r16-sources';
/** Closed explicit-column owner projections for the supplied IAM statement snapshot.
 * No caller-supplied table, SQL, predicate or alias is accepted. */
export function learningTalentSnapshotSources(){return {
  "learning_adapters_learning_hr_people_p_join": PP_RD16_HR_PEOPLE_P_JOIN_SQL,
  "learning_adapters_learning_hr_engagements_e_join": PP_RD18_HR_ENGAGEMENTS_E_JOIN_SQL,
  "learning_adapters_learning_hr_people_from": PP_RD18_HR_PEOPLE_FROM_SQL,
  "learning_adapters_learning_hr_people_p_from": "FROM (SELECT \"id\", \"user_id\", \"company_id\", \"legal_name\", \"preferred_name\", \"email\", \"phone\", \"country_code\", \"status\", \"version\", \"created_by\", \"created_at\", \"updated_at\", \"merged_into_id\", \"work_email\", \"work_phone\" FROM hr_people) p",
  "learning_adapters_learning_eligibility_hr_people_p_join": PP_RD16_HR_PEOPLE_P_JOIN_SQL,
  "talent_adapters_talent_hr_people_p_join": PP_RD16_HR_PEOPLE_P_JOIN_SQL
} as const;}
