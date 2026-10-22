import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import { z } from 'zod';
import { BusinessRuleError, type Clock } from '@erpx/server-shared/index';
import type { StaffActor, StaffPerson, RecordScope } from '@erpx/server-operations/public/staff-policy';
import { projectSchema, type ProjectRow } from '@erpx/server-projects/domain/staff-projects';
export interface StaffProjectsRepository<Effect> {
    visibleProject(id: string, companyId: string, userId: string, userId2: string): Promise<ProjectRow | null>;
    visibleProjects(companyId: string, userId: string, userId2: string): Promise<{
        results: ProjectRow[];
    }>;
    guardCurrentOwner(key: string, value: "project-version", recordId: string, expectedVersion: number, userId: string): Effect;
    prepareProjectRevision(title: string, description: string, now: number, recordId: string): Effect;
    prepareProject(recordId: string, companyId: string, branch_id: string | null, department_id: string | null, title: string, description: string, userId: string, now: number, now2: number): Effect;
    prepareMemberReplacement(recordId: string): Effect;
    prepareMember(recordId: string, user: string, userId: string, now: number): Effect;
}
export interface OwnerCommands<Effect> {
    replay<Result = Record<string, unknown>>(actor: StaffActor, key: string, operation: string, id: string, input: unknown): Promise<Result | null>;
    commit<Result>(actor: StaffActor, key: string, operation: string, input: unknown, id: string, version: number, result: Result, effects: Effect[], scope?: Pick<StaffActor, 'companyId' | 'branchId' | 'departmentId'>): Promise<Result>;
}
export interface StaffProjectsPorts<Effect> {
    repository: StaffProjectsRepository<Effect>;
    clock: Clock;
    commands: OwnerCommands<Effect>;
    activeStaff(companyId: string, userId: string): Promise<StaffPerson>;
    staffGuard(key: string, companyId: string, userId: string, suffix?: string): Effect;
}
export function createStaffProjectsApplication<Effect>(ports: StaffProjectsPorts<Effect>) {
    const r = ports.repository;
    const scope = (r: RecordScope) => ({ companyId: r.company_id, branchId: r.branch_id, departmentId: r.department_id });
    async function getProject(actor: StaffActor, id: string) {
        const row = await r.visibleProject(id, actor.companyId, actor.userId, actor.userId);
        if (!row)
            throw new BusinessRuleError(404, "PROJECT_NOT_FOUND", "Project not found.");
        return row;
    }
    async function listProjects(actor: StaffActor) {
        return (await r.visibleProjects(actor.companyId, actor.userId, actor.userId)).results;
    }
    async function saveProject(actor: StaffActor, id: string | null, input: z.infer<typeof projectSchema>, key: string) {
        const recordId = id ?? `project-${key}`, operation = id ? "project.update" : "project.create";
        const prior = await ports.commands.replay<{id:string;version:number}>(actor, key, operation, recordId, input);
        if (prior)
            return prior;
        const staff = await ports.activeStaff(actor.companyId, actor.userId), current = id ? await getProject(actor, id) : null;
        if (current && !samePerson(actor, current.owner_user_id))
            throw new BusinessRuleError(403, "PROJECT_OWNER_REQUIRED", "Only the project owner manages this project and its membership.");
        if (current && current.lifecycle_status !== 'active')
            throw new BusinessRuleError(409, 'PROJECT_STRUCTURE_FROZEN', 'Reopen the project before changing its brief or membership.');
        const members = [...new Set(input.memberUserIds)];
        for (const user of members)
            await ports.activeStaff(actor.companyId, user);
        const now = ports.clock.now(), version = current ? input.expectedVersion + 1 : 1;
        return ports.commands.commit(actor, key, operation, input, recordId, version, { id: recordId, version }, [
            ports.staffGuard(key, actor.companyId, actor.userId),
            ...(current
                ? [
                    r.guardCurrentOwner(key, "project-version", recordId, input.expectedVersion, actor.userId),
                ]
                : []),
            current
                ? r.prepareProjectRevision(input.title, input.description, now, recordId)
                : r.prepareProject(recordId, actor.companyId, staff.branch_id, staff.department_id, input.title, input.description, actor.userId, now, now),
            r.prepareMemberReplacement(recordId),
            ...members.flatMap((user, i) => [
                ports.staffGuard(key, actor.companyId, user, `member-${i}`),
                r.prepareMember(recordId, user, actor.userId, now),
            ]),
        ], scope(current ?? staff));
    }
    return { getProject, listProjects, saveProject };
}
