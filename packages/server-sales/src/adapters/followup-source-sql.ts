import { commercialOwnerProjectionSources as people_payrollProjectionSources } from "@erpx/server-people_payroll/public/commercial-owner-projection-sources";
import {packageClockSql} from "@erpx/server-commercial/public/purchased-capability-sql";

export const ownerNameProjection=`(SELECT legal_name FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE company_id=c.company_id AND user_id=c.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name`;
export const actorScopeTuple="(SELECT ?::text company_id,?::text branch_id,?::text department_id)";
export const clockNowMs=`SELECT ${packageClockSql} now_ms`;
