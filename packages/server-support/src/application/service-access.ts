import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { RecordScope } from "@erpx/server-operations/public/staff-policy";
import { inServiceWorkspace } from "@erpx/server-support/domain/service-access";
export type SupportViolation = {
    status: number;
    code: string;
    message: string;
};
export interface ServiceScopePorts {
    error(violation: SupportViolation): Error;
    member(scope: RecordScope, userId: string): Promise<boolean>;
    company(companyId: string): Promise<boolean>;
    branch(companyId: string, branchId: string): Promise<boolean>;
    department(companyId: string, branchId: string | null, departmentId: string): Promise<boolean>;
}
export function createValidateServiceScope(ports: ServiceScopePorts) {
    return async (actor: WorkContext, scope: RecordScope) => {
        if (!inServiceWorkspace(actor, scope) || !await ports.member(scope, actor.userId))
            throw ports.error({ status: 403, code: "REQUEST_SCOPE_FORBIDDEN", message: "Choose a location within your authorized workspace." });
        if (!await ports.company(scope.company_id))
            throw ports.error({ status: 409, code: "COMPANY_UNAVAILABLE", message: "The company is unavailable." });
        if (scope.branch_id && !await ports.branch(scope.company_id, scope.branch_id))
            throw ports.error({ status: 422, code: "LOCATION_INVALID", message: "Select an active branch." });
        if (scope.department_id && !await ports.department(scope.company_id, scope.branch_id, scope.department_id))
            throw ports.error({ status: 422, code: "LOCATION_INVALID", message: "Select a department in this branch." });
    };
}
export function createRequireServiceOwner(ports: {
    owner(actor: WorkContext): Promise<boolean>;
    error(violation: SupportViolation): Error;
}) { return async (actor: WorkContext) => { if (!await ports.owner(actor))
    throw ports.error({ status: 403, code: "OWNER_REQUIRED", message: "A company owner must configure service teams and handbook editors." }); }; }
