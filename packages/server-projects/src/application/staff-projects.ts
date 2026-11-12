import { BusinessRuleError } from '@erpx/server-shared/index';
import type { StaffActor } from '@erpx/server-operations/public/staff-policy';
import type { ProjectRow } from '@erpx/server-projects/domain/staff-projects';
export interface StaffProjectsRepository<Effect> {
    visibleProject(id: string, companyId: string, userId: string, userId2: string): Promise<ProjectRow | null>;
    visibleProjects(companyId: string, userId: string, userId2: string): Promise<{
        results: ProjectRow[];
    }>;
}
export interface StaffProjectsPorts<Effect> {
    repository: StaffProjectsRepository<Effect>;
}
export function createStaffProjectsApplication<Effect>(ports: StaffProjectsPorts<Effect>) {
    const r = ports.repository;
    async function getProject(actor: StaffActor, id: string) {
        const row = await r.visibleProject(id, actor.companyId, actor.userId, actor.userId);
        if (!row)
            throw new BusinessRuleError(404, "PROJECT_NOT_FOUND", "Project not found.");
        return row;
    }
    async function listProjects(actor: StaffActor) {
        return (await r.visibleProjects(actor.companyId, actor.userId, actor.userId)).results;
    }
    return { getProject, listProjects };
}
