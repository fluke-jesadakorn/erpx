import { z } from 'zod';
import { samePerson } from "@/lib/identity_access/iam";
import type { WorkContext } from '../../identity_access/public/authorization';
import type { StaffActor, StaffPerson } from '../../operations/public/staff-policy';
import type { ProjectRow } from '../domain/staff-projects';
import { BusinessRuleError, type Clock } from '../../shared';
import { projectMilestoneTransitionSchema, type ProjectMilestoneRow, type MilestoneInput, type MilestoneEventAction } from '../domain/milestones';
export interface ProjectMilestoneRepository<Effect> {
    isProjectMember(id: string, company_id: string, userId: string, userId2: string): Promise<Record<string, unknown> | null>;
    milestone(milestoneId: string, projectId: string): Promise<ProjectMilestoneRow | null>;
    prepareMilestoneEvent(value: string, id: string, version: number, action: MilestoneEventAction, userId: string, reason: string, value2: string, now: number): Effect;
    milestoneRows(companyId: string, projectId: string): Promise<{
        results: (Omit<ProjectMilestoneRow, "depends_on_ids" | "depends_on_titles" | "blocked_by_titles"> & {
            depends_on_ids: string;
            depends_on_titles: string;
            blocked_by_titles: string;
        })[];
    }>;
    parentMilestone(parentId: string, companyId: string, id: string): Promise<{
        id: string;
        version: number;
        state: string;
    } | null>;
    previousDependencies(companyId: string, id: string, id2: string): Promise<{
        results: {
            predecessor_milestone_id: string;
        }[];
    }>;
    referencedDependencies(value: string, companyId: string, id: string): Promise<{
        results: {
            id: string;
            state: string;
        }[];
    }>;
    dependencyCycle(id: string, id2: string, companyId: string, id3: string, value: string): Promise<{
        cyclic: boolean;
    } | null>;
    prepareProjectLock(id: string, companyId: string): Effect;
    guardProjectOwner(key: string, value: "project-owner", id: string, companyId: string, version: number, userId: string): Effect;
    guardParent(key: string, value: "package-parent", id: string, companyId: string, id2: string, version: number): Effect;
    guardPrerequisites(key: string, value: "milestone-prerequisites", dependencyJson: string, dependencyJson2: string, companyId: string, id: string, id2: string): Effect;
    guardAcyclic(key: string, value: "milestone-prerequisites-acyclic", id: string, id2: string, companyId: string, id3: string, dependencyJson: string): Effect;
    guardMilestoneRevision(key: string, value: "milestone-version", id: string, id2: string, companyId: string, expectedVersion: number): Effect;
    prepareMilestoneRevision(title: string, description: string, value: number, ownerUserId: string, parentId: string | null, position: number, now: number, id: string): Effect;
    guardMember(key: string, value: "project-member", id: string, companyId: string, ownerUserId: string, ownerUserId2: string): Effect;
    prepareMilestone(id: string, companyId: string, id2: string, title: string, description: string, value: number, ownerUserId: string, parentId: string | null, position: number, userId: string, now: number, now2: number): Effect;
    prepareDependenciesReplacement(companyId: string, id: string, id2: string): Effect;
    prepareDependency(companyId: string, id: string, predecessorId: string, id2: string, userId: string, now: number): Effect;
    prepareProjectVersion(now: number, id: string, companyId: string): Effect;
    incompletePrerequisites(companyId: string, id: string, milestoneId: string): Promise<{
        results: {
            title: string;
        }[];
    }>;
    activeSuccessors(companyId: string, id: string, milestoneId: string): Promise<Record<string, unknown> | null>;
    completedSuccessor(companyId: string, id: string, milestoneId: string): Promise<Record<string, unknown> | null>;
    prepareTransitionLock(id: string, companyId: string): Effect;
    guardTransitionOwner(key: string, value: "project-owner", id: string, companyId: string, version: number, userId: string): Effect;
    guardTransitionVersion(key: string, value: "milestone-version", milestoneId: string, id: string, companyId: string, expectedVersion: number, state: "completed" | "cancelled" | "planned" | "in_progress"): Effect;
    guardPrerequisitesCompleted(key: string, value: "milestone-prerequisites-complete", action: "start" | "complete" | "reopen" | "cancel", companyId: string, id: string, milestoneId: string): Effect;
    guardStillUsed(key: string, value: "milestone-prerequisite-still-used", action: "start" | "complete" | "reopen" | "cancel", companyId: string, id: string, milestoneId: string): Effect;
    guardSuccessorReopening(key: string, value: "milestone-no-completed-successor", action: "start" | "complete" | "reopen" | "cancel", companyId: string, id: string, milestoneId: string): Effect;
    prepareTransition(nextState: "completed" | "cancelled" | "in_progress", value: number | null, now: number, milestoneId: string): Effect;
    prepareTransitionProjectVersion(now: number, id: string, companyId: string): Effect;
}
export interface OwnerCommands<Effect> {
    replay<Result = Record<string, unknown>>(actor: StaffActor, key: string, operation: string, id: string, input: unknown): Promise<Result | null>;
    commit<Result>(actor: StaffActor, key: string, operation: string, input: unknown, id: string, version: number, result: Result, effects: Effect[], scope?: Pick<StaffActor, 'companyId' | 'branchId' | 'departmentId'>): Promise<Result>;
}
export interface ProjectMilestonePorts<Effect> {
    repository: ProjectMilestoneRepository<Effect>;
    clock: Clock;
    commands: OwnerCommands<Effect>;
    getProject(actor: StaffActor, id: string): Promise<ProjectRow>;
    activeStaff(companyId: string, userId: string): Promise<StaffPerson>;
    staffGuard(key: string, companyId: string, userId: string, suffix?: string): Effect;
    requirePermission(actor: WorkContext, permission: string): WorkContext;
}
export function createProjectMilestoneApplication<Effect>(ports: ProjectMilestonePorts<Effect>) {
    const r = ports.repository;
    function projectConstraint(error: unknown): never {
        const message = error instanceof Error ? error.message : '';
        if ((error as {
            code?: string;
        }).code === '23514' && /^PROJECT_[A-Z_]+$/.test(message)) {
            const descriptions: Record<string, string> = { PROJECT_STRUCTURE_FROZEN: 'Reopen the project before changing its work or evidence.', PROJECT_PACKAGE_CHILDREN_INCOMPLETE: 'Complete or cancel the active child packages before closing their parent.', PROJECT_PACKAGE_TASKS_INCOMPLETE: 'Complete or cancel the assigned tasks before closing this work package.', PROJECT_PACKAGE_CYCLE_OR_DEPTH: 'Choose a parent that keeps the work structure acyclic and within thirty levels.', PROJECT_EXECUTION_GRAPH_CYCLE: 'The selected hierarchy and prerequisites would prevent the project from completing. Choose an acyclic work structure.' };
            const conflict = new BusinessRuleError(409, message, descriptions[message] ?? 'The project source changed. Reload its current work structure and review again.');
            conflict.cause = error;
            throw conflict;
        }
        throw error;
    }
    function projectOwner(project: ProjectRow, actor: StaffActor) {
        if (!samePerson(actor, project.owner_user_id))
            throw new BusinessRuleError(403, "PROJECT_OWNER_REQUIRED", "Only the project owner manages milestones.");
    }
    async function assertProjectMember(project: ProjectRow, userId: string) {
        const member = !!(await r.isProjectMember(project.id, project.company_id, userId, userId));
        if (!member)
            throw new BusinessRuleError(422, "PROJECT_MEMBER_REQUIRED", "Assign the milestone to the project owner or a current project member.");
        await ports.activeStaff(project.company_id, userId);
    }
    async function loadMilestone(projectId: string, milestoneId: string) {
        const row = await r.milestone(milestoneId, projectId);
        if (!row)
            throw new BusinessRuleError(404, "PROJECT_MILESTONE_NOT_FOUND", "Project milestone not found.");
        return row;
    }
    function eventStatement(id: string, version: number, action: MilestoneEventAction, actor: StaffActor, reason: string, snapshot: Record<string, unknown>, now: number) {
        return r.prepareMilestoneEvent(`${id}:${version}`, id, version, action, actor.userId, reason, JSON.stringify(snapshot), now);
    }
    async function listProjectMilestones(actor: StaffActor, projectId: string) {
        await ports.getProject(actor, projectId);
        const rows = (await r.milestoneRows(actor.companyId, projectId)).results;
        return rows.map((row) => {
            const parse = (value: string) => {
                try {
                    const parsed = JSON.parse(value) as unknown;
                    return Array.isArray(parsed) ? parsed.filter((item): item is string => typeof item === "string") : [];
                }
                catch {
                    return [];
                }
            };
            const { depends_on_ids, depends_on_titles, blocked_by_titles, ...milestone } = row;
            const blockedByTitles = parse(blocked_by_titles);
            return { ...milestone, depends_on_ids: parse(depends_on_ids), depends_on_titles: parse(depends_on_titles), blocked_by_titles: blockedByTitles, blocked_by_count: blockedByTitles.length };
        });
    }
    async function saveProjectMilestone(actor: StaffActor, projectId: string, milestoneId: string | null, input: MilestoneInput, key: string) {
        const id = milestoneId ?? `milestone-${key}`;
        const operation = milestoneId ? "project.milestone.revise" : "project.milestone.create";
        const prior = await ports.commands.replay<{
            id: string;
            version: number;
        }>(actor, key, operation, id, { projectId, ...input });
        if (prior)
            return prior;
        const project = await ports.getProject(actor, projectId);
        projectOwner(project, actor);
        await assertProjectMember(project, input.ownerUserId);
        const current = milestoneId ? await loadMilestone(projectId, milestoneId) : null;
        const parentId = input.parentMilestoneId === undefined ? current?.parent_milestone_id ?? null : input.parentMilestoneId, position = input.ordinalPosition ?? current?.ordinal_position ?? 0;
        const parent = parentId ? await r.parentMilestone(parentId, actor.companyId, project.id) : null;
        if (parentId && (!parent || parentId === id))
            throw new BusinessRuleError(422, 'PROJECT_PACKAGE_PARENT_INVALID', 'Choose a different parent package in this project.');
        if (parent && parent.state !== 'planned' && parent.state !== 'in_progress')
            throw new BusinessRuleError(409, 'PROJECT_PACKAGE_PARENT_TERMINAL', 'Reopen the parent package before adding or revising its work.');
        if (current && (current.state === "completed" || current.state === "cancelled"))
            throw new BusinessRuleError(409, "PROJECT_MILESTONE_TERMINAL", "Reopen the milestone before editing its details.");
        const priorDependencies = current ? (await r.previousDependencies(actor.companyId, project.id, id)).results.map((row) => row.predecessor_milestone_id) : [];
        const dependencyIds = input.dependsOnMilestoneIds ?? priorDependencies;
        if (dependencyIds.includes(id))
            throw new BusinessRuleError(422, "PROJECT_DEPENDENCY_CYCLE", "A milestone cannot depend on itself.");
        const referenced = dependencyIds.length ? (await r.referencedDependencies(JSON.stringify(dependencyIds), actor.companyId, project.id)).results : [];
        if (referenced.length !== dependencyIds.length)
            throw new BusinessRuleError(422, "PROJECT_DEPENDENCY_SCOPE_INVALID", "Prerequisites must be milestones in this project.");
        if (referenced.some((row) => row.state === "cancelled"))
            throw new BusinessRuleError(422, "PROJECT_DEPENDENCY_CANCELLED", "A cancelled milestone cannot be a prerequisite.");
        if (dependencyIds.length) {
            const cycle = await r.dependencyCycle(id, id, actor.companyId, project.id, JSON.stringify(dependencyIds));
            if (cycle?.cyclic)
                throw new BusinessRuleError(422, "PROJECT_DEPENDENCY_CYCLE", "This prerequisite would create a project dependency cycle.");
        }
        const now = ports.clock.now(), version = current ? input.expectedVersion + 1 : 1;
        const snapshot = { projectId, title: input.title, description: input.description, dueAt: Date.parse(input.dueAt), ownerUserId: input.ownerUserId, dependsOnMilestoneIds: dependencyIds, parentMilestoneId: parentId, ordinalPosition: position };
        const dependencyJson = JSON.stringify(dependencyIds);
        return ports.commands.commit(actor, key, operation, { projectId, ...input }, id, version, { id, version }, [
            r.prepareProjectLock(project.id, actor.companyId),
            ports.staffGuard(key, actor.companyId, actor.userId),
            ports.staffGuard(key, actor.companyId, input.ownerUserId, "milestone-owner"),
            r.guardProjectOwner(key, "project-owner", project.id, actor.companyId, project.version, actor.userId),
            ...(parent ? [r.guardParent(key, 'package-parent', parent.id, actor.companyId, project.id, parent.version)] : []),
            r.guardPrerequisites(key, "milestone-prerequisites", dependencyJson, dependencyJson, actor.companyId, project.id, id),
            r.guardAcyclic(key, "milestone-prerequisites-acyclic", id, id, actor.companyId, project.id, dependencyJson),
            ...(current ? [
                r.guardMilestoneRevision(key, "milestone-version", id, project.id, actor.companyId, input.expectedVersion),
                r.prepareMilestoneRevision(input.title, input.description, Date.parse(input.dueAt), input.ownerUserId, parentId, position, now, id),
            ] : [
                r.guardMember(key, "project-member", project.id, actor.companyId, input.ownerUserId, input.ownerUserId),
                r.prepareMilestone(id, actor.companyId, project.id, input.title, input.description, Date.parse(input.dueAt), input.ownerUserId, parentId, position, actor.userId, now, now),
            ]),
            r.prepareDependenciesReplacement(actor.companyId, project.id, id),
            ...dependencyIds.map((predecessorId) => r.prepareDependency(actor.companyId, project.id, predecessorId, id, actor.userId, now)),
            r.prepareProjectVersion(now, project.id, actor.companyId),
            eventStatement(id, version, current ? "revised" : "created", actor, input.reason, snapshot, now),
        ], { companyId: project.company_id, branchId: project.branch_id, departmentId: project.department_id }).catch(projectConstraint);
    }
    async function transitionProjectMilestone(actor: StaffActor, projectId: string, milestoneId: string, input: z.infer<typeof projectMilestoneTransitionSchema>, key: string) {
        const operation = `project.milestone.${input.action}`;
        const prior = await ports.commands.replay<{
            id: string;
            version: number;
            state: ProjectMilestoneRow["state"];
        }>(actor, key, operation, milestoneId, { projectId, ...input });
        if (prior)
            return prior;
        const project = await ports.getProject(actor, projectId);
        projectOwner(project, actor);
        const current = await loadMilestone(projectId, milestoneId);
        const nextState = input.action === "start" ? "in_progress"
            : input.action === "complete" ? "completed"
                : input.action === "reopen" ? "in_progress" : "cancelled";
        const allowed = input.action === "start" ? current.state === "planned"
            : input.action === "complete" ? current.state === "planned" || current.state === "in_progress"
                : input.action === "reopen" ? current.state === "completed"
                    : current.state === "planned" || current.state === "in_progress";
        if (!allowed)
            throw new BusinessRuleError(409, "PROJECT_MILESTONE_TRANSITION_INVALID", "This milestone cannot move to the requested state.");
        const incompletePrerequisites = (await r.incompletePrerequisites(actor.companyId, project.id, milestoneId)).results;
        if ((input.action === "start" || input.action === "complete") && incompletePrerequisites.length)
            throw new BusinessRuleError(409, "PROJECT_DEPENDENCIES_INCOMPLETE", `Complete these prerequisites first: ${incompletePrerequisites.map((item) => item.title).join(", ")}.`);
        if (input.action === "cancel") {
            const activeSuccessors = await r.activeSuccessors(actor.companyId, project.id, milestoneId);
            if (activeSuccessors)
                throw new BusinessRuleError(409, "PROJECT_MILESTONE_REQUIRED_PREREQUISITE", "Remove or complete dependent milestones before cancelling this prerequisite.");
        }
        if (input.action === "reopen") {
            const completedSuccessor = await r.completedSuccessor(actor.companyId, project.id, milestoneId);
            if (completedSuccessor)
                throw new BusinessRuleError(409, "PROJECT_MILESTONE_DEPENDENT_COMPLETED", "Reopen completed dependent milestones before reopening this prerequisite.");
        }
        const now = ports.clock.now(), version = input.expectedVersion + 1;
        const snapshot = { projectId, state: nextState, dueAt: current.due_at, ownerUserId: current.owner_user_id };
        const action: MilestoneEventAction = input.action === "start" ? "started"
            : input.action === "complete" ? "completed"
                : input.action === "reopen" ? "reopened" : "cancelled";
        return ports.commands.commit(actor, key, operation, { projectId, ...input }, milestoneId, version, { id: milestoneId, version, state: nextState }, [
            r.prepareTransitionLock(project.id, actor.companyId),
            ports.staffGuard(key, actor.companyId, actor.userId),
            r.guardTransitionOwner(key, "project-owner", project.id, actor.companyId, project.version, actor.userId),
            r.guardTransitionVersion(key, "milestone-version", milestoneId, project.id, actor.companyId, input.expectedVersion, current.state),
            r.guardPrerequisitesCompleted(key, "milestone-prerequisites-complete", input.action, actor.companyId, project.id, milestoneId),
            r.guardStillUsed(key, "milestone-prerequisite-still-used", input.action, actor.companyId, project.id, milestoneId),
            r.guardSuccessorReopening(key, "milestone-no-completed-successor", input.action, actor.companyId, project.id, milestoneId),
            r.prepareTransition(nextState, nextState === "completed" ? now : null, now, milestoneId),
            r.prepareTransitionProjectVersion(now, project.id, actor.companyId),
            eventStatement(milestoneId, version, action, actor, input.reason, snapshot, now),
        ], { companyId: project.company_id, branchId: project.branch_id, departmentId: project.department_id }).catch(projectConstraint);
    }
    return { projectConstraint, listProjectMilestones, saveProjectMilestone, transitionProjectMilestone };
}
