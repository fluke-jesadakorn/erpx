import { platformOwnerProjectionSources } from "../../runtime/public/platform-owner-projection-sources";
export const recordsExecutiveUsageSql = `SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') AS month FROM (${platformOwnerProjectionSources.executive_generation_jobs}) executive_generation_jobs`;
