import "server-only";
import {PP_RD18_HR_PEOPLE_PROJ_SQL} from './pp-read-tail-r18-sources';
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const finalRootOwnerProjectionSources = {
  hr_engagements: "SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\",\"level_id\",\"position_id\",\"self_service_top_level\" FROM hr_engagements",
  hr_people: PP_RD18_HR_PEOPLE_PROJ_SQL,
} as const;
