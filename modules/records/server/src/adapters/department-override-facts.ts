import "server-only";
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import type {EmployeeRequest} from '@erpx/records-server/public/employee-requests';
import {nativeEmployeeRequestReadRepository} from './employee-requests-read';
import {employeeRequestsProjectionSql} from './platform-owner-projection-sources';
/** Switch to the owner's canonical native read repository; no local TS SQL. */
export const departmentOverrideRequest = (db: SqlDatabase, companyId: string, id: string): Promise<EmployeeRequest | null> => nativeEmployeeRequestReadRepository(db).readReadRequestForOverrideRow(id, companyId);
/** Reuse the owner's canonical employee-request projection (the same fixed
 * selection the native owner states); only the admitted predicates are local. */
const pendingDepartmentRowsSql = `${employeeRequestsProjectionSql} WHERE company_id=? AND state='awaiting_approval' AND approval_json::jsonb->'route'->'steps'->COALESCE((approval_json::jsonb->>'currentStep')::integer,0)->>'stage'='department_approval' ORDER BY updated_at DESC LIMIT 150`;
export async function departmentOverridePendingRequests(db: SqlDatabase, companyId: string) { return (await db.prepare(pendingDepartmentRowsSql).bind(companyId).all<EmployeeRequest>()).results; }
