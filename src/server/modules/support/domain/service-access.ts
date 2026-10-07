import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { RecordScope } from "@/server/modules/operations/public/staff-policy";
export function inServiceWorkspace(actor: WorkContext, scope: RecordScope) { return actor.companyId === scope.company_id && (!actor.branchId || actor.branchId === scope.branch_id) && (!actor.departmentId || actor.departmentId === scope.department_id); }
