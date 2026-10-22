import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { hasModuleAccess } from '@erpx/catalogue/module-access';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { StaffPerson } from '@erpx/server-operations/public/staff-policy';
import { BusinessRuleError, type Clock } from '@erpx/server-shared/index';
import { buildProjectGraph, sourceCurrent, addCost, type Project, type GraphTask, type Actual, type ProjectGraph } from '@erpx/server-projects/domain/graph';
import { createProjectGraphApplication } from '@erpx/server-projects/application/graph';
import { projectStructureCommandSchema, type ProjectHandoffView, type HandoffRow } from '@erpx/server-projects/domain/structure';
import type { ProjectMilestoneRow } from '@erpx/server-projects/domain/milestones';
import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";
export interface ProjectStructureRepository<Effect> {
    currentProjectAuthority(assignmentId: string, companyId: string, userId: string, role: string, now: number, now2: number): Promise<Record<string, unknown> | null>;
    graphTasks(projectId: string, companyId: string): Promise<{
        results: GraphTask[];
    }>;
    graphActuals(projectId: string, companyId: string, projectId2: string, companyId2: string): Promise<{
        results: Actual[];
    }>;
    preparePackageEvent(value: string, company_id: string, id: string, packageId: string | null, action: string, userId: string, note: string, value2: string, now: number): Effect;
    lockProject(projectId: string, companyId: string): Promise<unknown>;
    guardStructureMember(key: string, value: "project-member", id: string, companyId: string, expectedVersion: number, userId: string, userId2: string): Effect;
    currentTask(taskId: string, companyId: string, id: string): Promise<{
        id: string;
        version: number;
        project_id: string;
    } | null>;
    taskBinding(taskId: string): Promise<{
        milestone_id: string;
        version: number;
    } | null>;
    guardAssignablePackage(key: string, value: "package-version", milestoneId: string, companyId: string, id: string, milestoneVersion: number): Effect;
    prepareTaskBinding(taskId: string, companyId: string, id: string, milestoneId: string, taskVersion: number, value: 1, userId: string, now: number): Effect;
    guardUnassignedPackage(key: string, value: "package-editable", packageId: string | null): Effect;
    prepareTaskUnbinding(taskId: string, bindingVersion: number): Effect;
    approvedTime(sourceId: string, companyId: string, id: string, sourceVersion: number): Promise<{
        minutes: number;
    } | null>;
    approvedCost(sourceId: string, companyId: string, id: string, sourceVersion: number): Promise<{
        amount_minor: number;
        currency: string;
    } | null>;
    guardActualPackage(key: string, value: "actual-package-version", packageId: string, companyId: string, id: string, milestoneVersion: number): Effect;
    prepareActualBinding(value: string, companyId: string, id: string, packageId: string, sourceKind: "work_log" | "cost_allocation", sourceId: string, sourceVersion: number, minutes: number | null, amount: number | null, currency: string | null, userId: string, now: number): Effect;
    handoffReadiness(id: string, id2: string, id3: string, id4: string, id5: string, id6: string, id7: string): Promise<{
        ready: boolean;
    } | null>;
    guardRecipientMember(key: string, value: "recipient-member", id: string, recipientId: string): Effect;
    prepareHandoff(handoffId: string, companyId: string, id: string, version: number, recipientId: string, snapshot: string, digest: string, evidence: string, userId: string, now: number): Effect;
    prepareHandoffNotice(value: string, companyId: string, recipientId: string, handoffId: string, now: number): Effect;
    currentHandoff(handoffId: string, companyId: string, id: string): Promise<{
        id: string;
        status: string;
        version: number;
        recipient_id: string;
        prepared_by: string;
        snapshot_sha256: string;
    } | null>;
    prepareHandoffDecision(status: "cancelled" | "accepted" | "rejected", userId: string, now: number, reason: string, id: string): Effect;
    prepareHandoffDecisionNotice(value: string, companyId: string, prepared_by: string, id: string, value2: number, value3: string, now: number): Effect;
    prepareLifecycle(lifecycle: "active" | "handoff_pending" | "completed" | "cancelled", now: number, id: string): Effect;
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
export interface OwnerCommands<Effect> {
    replay<Result = Record<string, unknown>>(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Result | null>;
    commit<Result>(actor: WorkContext, key: string, operation: string, input: unknown, id: string, version: number, result: Result, effects: Effect[], scope?: Pick<WorkContext, 'companyId' | 'branchId' | 'departmentId'>): Promise<Result>;
}
export interface ProjectStructurePorts<Effect> {
    purchasedCostHistory?: boolean;
    repository: ProjectStructureRepository<Effect>;
    clock: Clock;
    commands: OwnerCommands<Effect>;
    requirePermission(actor: WorkContext, permission: string): WorkContext;
    activeStaff(companyId: string, userId: string): Promise<StaffPerson>;
    staffGuard(key: string, companyId: string, userId: string, suffix?: string): Effect;
    getProject(actor: WorkContext, id: string): Promise<Project>;
    listMilestones(actor: WorkContext, id: string): Promise<ProjectMilestoneRow[]>;
    sha256(value: string): Promise<string>;
}
export function createProjectStructureApplication<Effect>(ports: ProjectStructurePorts<Effect>) {
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
    function owner(a: WorkContext, p: Project) { if (!samePerson(a, p.owner_user_id))
        throw new BusinessRuleError(403, 'PROJECT_OWNER_REQUIRED', 'The project owner manages work structure and handoff.'); }
    async function graphRows(a: WorkContext, projectId: string) {
        const p = await project(a, projectId), milestones = await ports.listMilestones(a, projectId), tasks = (await r.graphTasks(projectId, a.companyId)).results;
        if (milestones.length > 1000 || tasks.length > 1000)
            throw new BusinessRuleError(422, 'PROJECT_GRAPH_TOO_LARGE', 'Divide this project into bounded work packages before creating a complete handoff snapshot.');
        const actuals = (await r.graphActuals(projectId, a.companyId, projectId, a.companyId)).results;
        return { project: p, milestones, tasks, actuals };
    }
    function buildGraph(rows: Awaited<ReturnType<typeof graphRows>>, a: WorkContext, includeAll = false): ProjectGraph { try {
        return buildProjectGraph(rows, { isOwner: samePerson(a, rows.project.owner_user_id), canReadCosts: costAccess(a, rows.project), isSamePerson: (id) => samePerson(a, id) }, includeAll);
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    } }
    async function projectGraph(a: WorkContext, projectId: string) { try {
        return await createProjectGraphApplication({ read: (id) => graphRows(a, id), viewer: (p) => ({ isOwner: samePerson(a, p.owner_user_id), canReadCosts: costAccess(a, p), isSamePerson: (id) => samePerson(a, id) }) })(projectId);
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    } }
    function event(key: string, p: Project, packageId: string | null, action: string, a: WorkContext, note: string, snapshot: unknown, now: number) { return r.preparePackageEvent(`project-package-event-${key}`, p.company_id, p.id, packageId, action, a.userId, note, JSON.stringify(snapshot), now); }
    async function projectStructureCommand(a: WorkContext, projectId: string, raw: unknown, key: string): Promise<{id:string;version:number;status:string;taskId?:string;bindingVersion?:number;actualId?:string;handoffId?:string;snapshotSha256?:string}> {
        access(a, true);
        const input = projectStructureCommandSchema.parse(raw);
        await project(a, projectId);
        const op = `project.structure.${input.action}`, prior = await ports.commands.replay<{id:string;version:number;status:string;taskId?:string;bindingVersion?:number;actualId?:string;handoffId?:string;snapshotSha256?:string}>(a, key, op, projectId, input);
        if (prior)
            return prior;
        await r.lockProject(projectId, a.companyId);
        const p = await project(a, projectId), now = ports.clock.now(), nextVersion = input.expectedVersion + 1;
        if (p.version !== input.expectedVersion)
            throw new BusinessRuleError(409, 'CONFLICT', 'The project changed. Reload its current structure before continuing.');
        const steps: Effect[] = [ports.staffGuard(key, a.companyId, a.userId), r.guardStructureMember(key, 'project-member', p.id, a.companyId, input.expectedVersion, a.userId, a.userId)];
        let lifecycle = p.lifecycle_status, packageId: string | null = null, result: {id:string;version:number;taskId?:string;bindingVersion?:number;actualId?:string;handoffId?:string;snapshotSha256?:string} = { id: projectId, version: nextVersion };
        if (input.action === 'assign-task' || input.action === 'unassign-task') {
            owner(a, p);
            if (p.lifecycle_status !== 'active')
                throw new BusinessRuleError(409, 'PROJECT_STRUCTURE_FROZEN', 'Reopen the project before changing work structure.');
            const task = await r.currentTask(input.taskId, a.companyId, p.id), binding = await r.taskBinding(input.taskId);
            if (!task || task.version !== input.taskVersion || (binding?.version ?? 0) !== input.bindingVersion)
                throw new BusinessRuleError(409, 'PROJECT_TASK_BINDING_CHANGED', 'Reload the task and its current work package.');
            packageId = input.action === 'assign-task' ? input.milestoneId : binding?.milestone_id ?? null;
            if (input.action === 'assign-task')
                steps.push(r.guardAssignablePackage(key, 'package-version', input.milestoneId, a.companyId, p.id, input.milestoneVersion), r.prepareTaskBinding(input.taskId, a.companyId, p.id, input.milestoneId, input.taskVersion, 1, a.userId, now));
            else
                steps.push(r.guardUnassignedPackage(key, 'package-editable', packageId), r.prepareTaskUnbinding(input.taskId, input.bindingVersion));
            result = { ...result, taskId: input.taskId, bindingVersion: input.action === 'assign-task' ? input.bindingVersion + 1 : 0 };
        }
        else if (input.action === 'link-actual') {
            if (p.lifecycle_status !== 'active')
                throw new BusinessRuleError(409, 'PROJECT_STRUCTURE_FROZEN', 'Reopen the project before assigning approved actuals.');
            packageId = input.milestoneId;
            let minutes: number | null = null, amount: number | null = null, currency: string | null = null;
            if (input.sourceKind === 'work_log') {
                owner(a, p);
                const source = await r.approvedTime(input.sourceId, a.companyId, p.id, input.sourceVersion);
                if (!source)
                    throw new BusinessRuleError(409, 'PROJECT_TIME_SOURCE_CHANGED', 'Choose the current independently approved project time.');
                minutes = source.minutes;
            }
            else {
                ports.requirePermission(a, 'finance.write');
                if (!costAccess(a, p))
                    throw new BusinessRuleError(404, 'PROJECT_NOT_FOUND', 'Project not found in the current Finance scope.');
                const source = await r.approvedCost(input.sourceId, a.companyId, p.id, input.sourceVersion);
                if (!source)
                    throw new BusinessRuleError(409, 'PROJECT_COST_SOURCE_CHANGED', 'Choose the current approved allocation from a posted Finance journal.');
                amount = source.amount_minor;
                currency = source.currency;
            }
            steps.push(r.guardActualPackage(key, 'actual-package-version', packageId, a.companyId, p.id, input.milestoneVersion), r.prepareActualBinding(`project-package-actual-${key}`, a.companyId, p.id, packageId, input.sourceKind, input.sourceId, input.sourceVersion, minutes, amount, currency, a.userId, now));
            result = { ...result, actualId: `project-package-actual-${key}` };
        }
        else if (input.action === 'prepare-handoff') {
            owner(a, p);
            if (p.lifecycle_status !== 'active')
                throw new BusinessRuleError(409, 'PROJECT_HANDOFF_NOT_READY', 'The project must be active before a new handoff.');
            if (samePerson(a, input.recipientId))
                throw new BusinessRuleError(409, 'INDEPENDENT_HANDOFF_REQUIRED', 'A different project member must receive and review this handoff.');
            await ports.activeStaff(a.companyId, input.recipientId);
            const ready = await r.handoffReadiness(p.id, p.id, p.id, p.id, p.id, p.id, p.id);
            if (!ready?.ready)
                throw new BusinessRuleError(409, 'PROJECT_HANDOFF_INCOMPLETE', 'Close all work and reviews, and assign completed tasks and approved time/cost sources to work packages before handoff.');
            const graph = buildGraph(await graphRows(a, p.id), a, true), snapshot = JSON.stringify(graph), digest = await ports.sha256(snapshot), handoffId = `project-handoff-${key}`;
            steps.push(ports.staffGuard(key, a.companyId, input.recipientId, 'handoff-recipient'), r.guardRecipientMember(key, 'recipient-member', p.id, input.recipientId), r.prepareHandoff(handoffId, a.companyId, p.id, p.version, input.recipientId, snapshot, digest, input.evidence, a.userId, now), r.prepareHandoffNotice(`project-handoff-notice-${key}`, a.companyId, input.recipientId, handoffId, now));
            lifecycle = 'handoff_pending';
            result = { ...result, handoffId, snapshotSha256: digest };
        }
        else {
            const h = await r.currentHandoff(input.handoffId, a.companyId, p.id);
            if (!h || h.version !== input.handoffVersion)
                throw new BusinessRuleError(409, 'PROJECT_HANDOFF_CHANGED', 'Reload the current handoff before reviewing it.');
            if (input.action === 'reopen') {
                if (p.lifecycle_status !== 'completed' || h.status !== 'accepted' || !samePerson(a, h.recipient_id) || samePerson(a, p.owner_user_id))
                    throw new BusinessRuleError(403, 'PROJECT_RECIPIENT_REOPEN_REQUIRED', 'The independent receiving member must review reopening an accepted project.');
                lifecycle = 'active';
            }
            else {
                if (h.status !== 'pending' || p.lifecycle_status !== 'handoff_pending')
                    throw new BusinessRuleError(409, 'PROJECT_HANDOFF_DECIDED', 'This handoff already has a recorded decision.');
                if (input.action === 'cancel-handoff') {
                    if (!samePerson(a, h.prepared_by))
                        throw new BusinessRuleError(403, 'PROJECT_HANDOFF_PREPARER_REQUIRED', 'The preparing owner cancels the pending handoff.');
                }
                else if (!samePerson(a, h.recipient_id) || samePerson(a, h.prepared_by))
                    throw new BusinessRuleError(403, 'PROJECT_RECIPIENT_REQUIRED', 'The named independent receiving member must decide this handoff.');
                const status = input.action === 'accept-handoff' ? 'accepted' : input.action === 'reject-handoff' ? 'rejected' : 'cancelled';
                steps.push(r.prepareHandoffDecision(status, a.userId, now, input.reason, h.id));
                if (input.action !== 'cancel-handoff')
                    steps.push(r.prepareHandoffDecisionNotice(`project-handoff-decision-${key}`, a.companyId, h.prepared_by, h.id, h.version + 1, `Project handoff ${status}; retained review evidence is available`, now));
                lifecycle = status === 'accepted' ? 'completed' : 'active';
            }
            result = { ...result, handoffId: h.id, snapshotSha256: h.snapshot_sha256 };
        }
        steps.push(r.prepareLifecycle(lifecycle, now, p.id), event(key, p, packageId, input.action, a, input.reason, { ...input, ...result, status: lifecycle }, now));
        return ports.commands.commit(a, key, op, input, p.id, nextVersion, { ...result, status: lifecycle }, steps, { companyId: p.company_id, branchId: p.branch_id, departmentId: p.department_id });
    }
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
    return { projectGraph, projectStructureCommand, projectHandoffs, projectHandoffTarget, projectHandoffWork, access };
}
