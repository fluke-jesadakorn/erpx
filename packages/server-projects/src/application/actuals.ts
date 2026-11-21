import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { StaffActor } from '@erpx/server-operations/public/staff-policy';
import type { ProjectRow } from '@erpx/server-projects/domain/staff-projects';
import { BusinessRuleError } from '@erpx/server-shared/index';
import type { ProjectWorkLogRow, ProjectCostAllocationRow, ProjectCostSourceRow, ProjectFinanceScope } from '@erpx/server-projects/domain/actuals';
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
export interface ProjectActualsRepository<Effect> {
    workLogs(userId: string, userId2: string, userId3: string, companyId: string, id: string, userId4: string, userId5: string): Promise<{
        results: ProjectWorkLogRow[];
    }>;
    costAllocations(userId: string, userId2: string, companyId: string, id: string): Promise<{
        results: Record<string, string | number | boolean | null>[];
    }>;
    costSources(companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null, branch_id: string | null, branch_id2: string | null, department_id: string | null, department_id2: string | null): Promise<{
        results: Record<string, string | number>[];
    }>;
}
export interface ProjectActualsPorts<Effect> {
    repository: ProjectActualsRepository<Effect>;
    getProject(actor: StaffActor, id: string): Promise<ProjectRow>;
    requirePermission(actor: WorkContext, permission: string): WorkContext;
}
export function createProjectActualsApplication<Effect>(ports: ProjectActualsPorts<Effect>) {
    const r = ports.repository;
    async function assertFinanceProjectAccess(actor: WorkContext, project: ProjectFinanceScope) {
        if ((actor.branchId && actor.branchId !== project.branch_id)
            || (actor.departmentId && actor.departmentId !== project.department_id))
            throw new BusinessRuleError(404, "PROJECT_NOT_FOUND", "Project not found in the current finance scope.");
    }
    async function listProjectWorkLogs(actor: StaffActor, projectId: string) {
        const project = await ports.getProject(actor, projectId);
        return (await r.workLogs(actor.userId, actor.userId, actor.userId, actor.companyId, project.id, actor.userId, actor.userId)).results;
    }
    function financeProject(actor: WorkContext, permission: "finance.read" | "finance.write" | "finance.approve") {
        if (!hasPermission(actor, permission))
            throw new BusinessRuleError(403, "FINANCE_PERMISSION_REQUIRED", `The ${permission} permission is required for project cost actuals.`);
        return ports.requirePermission(actor, permission);
    }
    async function visibleFinanceProject(actor: WorkContext, projectId: string) {
        const project = await ports.getProject(actor, projectId) as ProjectFinanceScope;
        await assertFinanceProjectAccess(actor, project);
        return project;
    }
    async function listProjectCostAllocations(actor: WorkContext, projectId: string) {
        actor = financeProject(actor, "finance.read");
        const project = await visibleFinanceProject(actor, projectId);
        return (await r.costAllocations(actor.userId, actor.userId, actor.companyId, project.id)).results.map((row) => ({
            ...row, amount: minorToDecimal(Number(row.amount_minor)),
        })) as ProjectCostAllocationRow[];
    }
    async function listProjectCostSources(actor: WorkContext, projectId: string) {
        actor = financeProject(actor, "finance.read");
        const project = await visibleFinanceProject(actor, projectId);
        const rows = (await r.costSources(actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, project.branch_id, project.branch_id, project.department_id, project.department_id)).results;
        return rows.map((row) => ({ ...row, debit: minorToDecimal(Number(row.debit_minor)), remaining: minorToDecimal(Number(row.remaining_minor)) })) as ProjectCostSourceRow[];
    }
    return { listProjectWorkLogs, listProjectCostAllocations, listProjectCostSources };
}
