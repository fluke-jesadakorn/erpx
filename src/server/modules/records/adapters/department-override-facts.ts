import "server-only";
import type { SqlDatabase } from '../../runtime/public/runtime/types';
import type { EmployeeRequest } from '../public/employee-requests';
export const departmentOverrideRequest = (db: SqlDatabase, companyId: string, id: string) => db.prepare('SELECT * FROM employee_requests WHERE id=? AND company_id=?').bind(id, companyId).first<EmployeeRequest>();
export async function departmentOverridePendingRequests(db: SqlDatabase, companyId: string) { return (await db.prepare("SELECT * FROM employee_requests WHERE company_id=? AND state='awaiting_approval' AND approval_json::jsonb->'route'->'steps'->COALESCE((approval_json::jsonb->>'currentStep')::integer,0)->>'stage'='department_approval' ORDER BY updated_at DESC LIMIT 150").bind(companyId).all<EmployeeRequest>()).results; }
