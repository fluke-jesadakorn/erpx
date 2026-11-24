import { commercialOwnerProjectionSources as people_payrollProjectionSources } from "@erpx/server-people_payroll/public/commercial-owner-projection-sources";
import "server-only";

/** One CRM-owned owner display-name projection shared by the lead records and
 * calendar fact reads so both callers execute identical query text instead of
 * two independently maintained literals. */
export const leadOwnerNameSql = `(SELECT legal_name FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE company_id=l.company_id AND user_id=l.owner_id AND merged_into_id IS NULL LIMIT 1)`;
