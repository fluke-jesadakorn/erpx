import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { hasModuleAccess } from '@erpx/catalogue/module-access';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { StaffPerson } from '@erpx/server-operations/public/staff-policy';
import { BusinessRuleError, type Clock } from '@erpx/server-shared/index';
import { sourceCurrent, addCost, type Project, type GraphTask, type Actual, type ProjectGraph } from '@erpx/server-projects/domain/graph';
import { createProjectGraphApplication } from '@erpx/server-projects/application/graph';
import type { ProjectHandoffView, HandoffRow } from '@erpx/server-projects/domain/structure';
import type { ProjectMilestoneRow } from '@erpx/server-projects/domain/milestones';
import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";
export interface ProjectStructureRepository {
    currentProjectAuthority(assignmentId: string, companyId: string, userId: string, role: string, now: number, now2: number): Promise<Record<string, unknown> | null>;
    graphTasks(projectId: string, companyId: string): Promise<{
        results: GraphTask[];
    }>;
    graphActuals(projectId: string, companyId: string, projectId2: string, companyId2: string): Promise<{
        results: Actual[];
    }>;
    handoffs(companyId: string, id: string): Promise<{
        results: HandoffRow[];
    }>;
    handoffProject(id: string, companyId: string): Promise<{
        project_id: string;
    } | null>;
    currentWorkAuthority(assignmentId: string, companyId: string, userId: string, role: string, now: number, now2: number): Promise<Record<string, unknown> | null>;
    handoffWork(companyId: string, userId: string, userId2: string, userId3: string, userId4: string): Promise<{
        results: {
            id: string;
            version: number;
            project_id: string;
            recipient_id: string;
            prepared_by: string;
            status: string;
            prepared_at: number;
            decided_at: number | null;
            title: string;
            lifecycle_status: string;
        }[];
    }>;
}
export interface ProjectStructurePorts {
    purchasedCostHistory?: boolean;
    repository: ProjectStructureRepository;
    clock: Clock;
    requirePermission(actor: WorkContext, permission: string): WorkContext;
    activeStaff(companyId: string, userId: string): Promise<StaffPerson>;
    getProject(actor: WorkContext, id: string): Promise<Project>;
    listMilestones(actor: WorkContext, id: string): Promise<ProjectMilestoneRow[]>;
}
export function createProjectStructureApplication(ports: ProjectStructurePorts) {
    const r = ports.repository;
    function access(a: WorkContext, write = false) { ports.requirePermission(a, write ? 'records.write' : 'records.read'); if (!hasModuleAccess(a.modules, 'projects'))
        throw new BusinessRuleError(403, 'MODULE_FORBIDDEN', 'Your current role cannot access Projects.'); }
    function costAccess(a: WorkContext, p: Project) { return ports.purchasedCostHistory !== false && hasPermission(a, 'finance.read') && (!a.branchId || a.branchId === p.branch_id) && (!a.departmentId || a.departmentId === p.department_id); }
    async function project(a: WorkContext, projectId: string) {
        access(a);
        const current = await r.currentProjectAuthority(a.assignmentId, a.companyId, a.userId, a.role, ports.clock.now(), ports.clock.now());
        if (!current)
            throw new BusinessRuleError(404, 'PROJECT_NOT_FOUND', 'Project not found in the current role.');
        await ports.activeStaff(a.companyId, a.userId);
        return await ports.getProject(a, projectId) as Project;
    }
    async function graphRows(a: WorkContext, projectId: string) {
        const p = await project(a, projectId), milestones = await ports.listMilestones(a, projectId), tasks = (await r.graphTasks(projectId, a.companyId)).results;
        if (milestones.length > 1000 || tasks.length > 1000)
            throw new BusinessRuleError(422, 'PROJECT_GRAPH_TOO_LARGE', 'Divide this project into bounded work packages before creating a complete handoff snapshot.');
        const actuals = (await r.graphActuals(projectId, a.companyId, projectId, a.companyId)).results;
        return { project: p, milestones, tasks, actuals };
    }
    async function projectGraph(a: WorkContext, projectId: string) { try {
        return await createProjectGraphApplication({ read: (id) => graphRows(a, id), viewer: (p) => ({ isOwner: samePerson(a, p.owner_user_id), canReadCosts: costAccess(a, p), isSamePerson: (id) => samePerson(a, id) }) })(projectId);
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    } }
    async function projectHandoffs(a: WorkContext, projectId: string): Promise<ProjectHandoffView[]> {
        const p = await project(a, projectId), isOwner = samePerson(a, p.owner_user_id), canCosts = costAccess(a, p), rows = (await r.handoffs(a.companyId, p.id)).results;
        return rows.map(row => {
            const graph = JSON.parse(String(row.snapshot_json)) as ProjectGraph;
            const visible = graph.actuals.filter(source => source.source_kind === 'cost_allocation' ? canCosts : isOwner || samePerson(a, source.user_id));
            const packages = graph.packages.map(packageRow => { const costs: Record<string, string> = {}; let minutes = 0; for (const source of visible.filter(s => s.milestone_id === packageRow.id && sourceCurrent(s))) {
                if (source.source_kind === 'work_log')
                    minutes += Number(source.minutes);
                else
                    addCost(costs, source.currency!, Number(source.amount_minor));
            } return { ...packageRow, approvedMinutes: minutes, costByCurrency: costs }; });
            const costs: Record<string, string> = {};
            for (const source of visible.filter(source => source.source_kind === 'cost_allocation' && sourceCurrent(source)))
                addCost(costs, source.currency!, Number(source.amount_minor));
            return { ...row, snapshot_json: undefined, snapshot: { ...graph, actuals: visible, packages, canReadCosts: canCosts, isProjectOwner: isOwner, totals: { approvedMinutes: packages.reduce((n, pack) => n + pack.approvedMinutes, 0), costByCurrency: costs } } };
        });
    }
    async function projectHandoffTarget(a: WorkContext, id: string) {
        const source = await r.handoffProject(id, a.companyId);
        if (!source)
            throw new BusinessRuleError(404, 'PROJECT_HANDOFF_NOT_FOUND', 'Project handoff not found.');
        await project(a, source.project_id);
        return `/projects/${encodeURIComponent(source.project_id)}#project-structure`;
    }
    async function projectHandoffWork(a: WorkContext): Promise<WorkItem[]> {
        if (!hasPermission(a, 'records.read') || !hasModuleAccess(a.modules, 'projects'))
            return [];
        const current = await r.currentWorkAuthority(a.assignmentId, a.companyId, a.userId, a.role, ports.clock.now(), ports.clock.now());
        if (!current)
            return [];
        const rows = (await r.handoffWork(a.companyId, a.userId, a.userId, a.userId, a.userId)).results;
        return rows.map(row => {
            const canAct = row.status === 'pending' && row.lifecycle_status === 'handoff_pending' && samePerson(a, row.recipient_id) && !samePerson(a, row.prepared_by) && hasPermission(a, 'records.write'), href = `/projects/${encodeURIComponent(row.project_id)}#project-structure`;
            return { id: row.id, type: 'project_handoff', source: { kind: 'project_handoff', id: row.id, version: row.version, label: 'Project handoff', href }, title: row.title, detail: `Project handoff · ${row.status}`, href, status: row.status, updatedAt: row.decided_at ?? row.prepared_at, queue: row.status !== 'pending' ? 'completed' : canAct ? 'todo' : 'waiting', canAct, actionKind: 'approval', nextAction: canAct ? 'Review completed project and accept or request correction' : 'Await independent receiving review', responsibleQueue: 'Receiving project member', relationships: [samePerson(a, row.recipient_id) ? 'approval' : 'requested'] };
        });
    }
    return { projectGraph, projectHandoffs, projectHandoffTarget, projectHandoffWork, access };
}
