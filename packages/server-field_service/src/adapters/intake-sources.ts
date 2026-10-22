import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { assignmentReadSql } from "@erpx/server-identity_access/public/record-source-authority";
export type ServiceIntakeAgreement = { id:string;version:number;company_id:string;branch_id:string;department_id:string|null;customer_id:string;asset_id:string|null };
export async function serviceIntakeAgreement(db:SqlDatabase,a:WorkContext,id:string) {
 return db.prepare(`SELECT sg.id,sg.version,sg.company_id,sg.branch_id,sg.department_id,sg.customer_id,sg.asset_id FROM field_service_agreements sg
 WHERE sg.id=? AND sg.company_id=? AND (?::text IS NULL OR sg.branch_id=?) AND (?::text IS NULL OR sg.department_id=?)
 AND ${assignmentReadSql("sg","records.write")}`).bind(id,a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId,a.assignmentId,a.userId,a.role).first<ServiceIntakeAgreement>();
}
export function serviceIntakeAgreementGuards(db:SqlDatabase,a:WorkContext,key:string,source:ServiceIntakeAgreement) {
 return [db.prepare("SELECT id FROM field_service_agreements WHERE id=? AND company_id=? FOR SHARE").bind(source.id,a.companyId),
 assertion(db,key,"service-intake-agreement",`EXISTS(SELECT 1 FROM field_service_agreements sg WHERE sg.id=? AND sg.version=? AND sg.company_id=?
 AND sg.customer_id=? AND sg.branch_id=? AND sg.department_id IS NOT DISTINCT FROM ?::text AND sg.asset_id IS NOT DISTINCT FROM ?::text
 AND sg.status='active' AND sg.start_date<=to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD') AND sg.end_date>=to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD')
 AND (?::text IS NULL OR sg.branch_id=?) AND (?::text IS NULL OR sg.department_id=?) AND ${assignmentReadSql("sg","records.write")})`,
 [source.id,source.version,a.companyId,source.customer_id,source.branch_id,source.department_id,source.asset_id,a.branchId,a.branchId,a.departmentId,a.departmentId,a.assignmentId,a.userId,a.role])];
}
