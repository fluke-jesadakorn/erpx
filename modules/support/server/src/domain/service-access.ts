import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { RecordScope } from "@erpx/operations-server/public/staff-policy";
export function inServiceWorkspace(actor: WorkContext, scope: RecordScope) { return actor.companyId === scope.company_id && (!actor.branchId || actor.branchId === scope.branch_id) && (!actor.departmentId || actor.departmentId === scope.department_id); }
