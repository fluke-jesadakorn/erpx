import type { StaffActor } from '@erpx/server-operations/public/staff-policy';
import type { ProjectRow } from '@erpx/server-projects/domain/staff-projects';
import { BusinessRuleError } from '@erpx/server-shared/index';
import type { ProjectMilestoneRow } from '@erpx/server-projects/domain/milestones';
export interface ProjectMilestoneRepository<Effect> {
    milestoneRows(companyId: string, projectId: string): Promise<{
        results: (Omit<ProjectMilestoneRow, "depends_on_ids" | "depends_on_titles" | "blocked_by_titles"> & {
            depends_on_ids: string;
            depends_on_titles: string;
            blocked_by_titles: string;
        })[];
    }>;
}
export interface ProjectMilestonePorts<Effect> {
    repository: ProjectMilestoneRepository<Effect>;
    getProject(actor: StaffActor, id: string): Promise<ProjectRow>;
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
    return { projectConstraint, listProjectMilestones };
}
