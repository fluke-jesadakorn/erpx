import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { RecordScope } from "@erpx/server-operations/public/staff-policy";
export function inServiceWorkspace(actor: WorkContext, scope: RecordScope) { return actor.companyId === scope.company_id && (!actor.branchId || actor.branchId === scope.branch_id) && (!actor.departmentId || actor.departmentId === scope.department_id); }
