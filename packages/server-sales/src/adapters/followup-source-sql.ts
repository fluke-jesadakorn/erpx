import { commercialOwnerProjectionSources as people_payrollProjectionSources } from "@erpx/server-people_payroll/public/commercial-owner-projection-sources";
import {packageClockSql} from "@erpx/server-commercial/public/purchased-capability-sql";

export const ownerNameProjection=`(SELECT legal_name FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE company_id=c.company_id AND user_id=c.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name`;
export const actorScopeTuple="(SELECT ?::text company_id,?::text branch_id,?::text department_id)";
/** One parameterized assignment-authority scope-read family. The authority
 * predicate, scope tuple, relation and optional extra guard are owner-local
 * fixed tokens composed by callers; no caller input enters the rendered
 * statement, so each call stays byte-identical to its original literal. */
export const authorityScopeRead=(authority:string,tuple:string,relation:string,extra='')=>`SELECT ${authority}${extra} allowed FROM ${tuple} ${relation}`;
export const clockNowMs=`SELECT ${packageClockSql} now_ms`;
export const followupEvidencePrefix=`SELECT c.id,c.order_id,c.next_action,c.due_date,c.status,c.version,o.number order_number FROM `;
export const followupMetricPrefix=`SELECT followup.*,actual.branch_id,actual.department_id,actual.kind order_kind`;
