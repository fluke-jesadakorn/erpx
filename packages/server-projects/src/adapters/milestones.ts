import { projectReadProjection as projectpeople_payrollProjection } from '@erpx/server-people_payroll/public/project-read-projections';
import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { StaffActor } from '@erpx/server-operations/public/staff-policy';
import { getProject } from '@erpx/server-projects/public/staff-projects';
import { BusinessRuleError } from '@erpx/server-shared/index';
import type { ProjectMilestoneRow } from '@erpx/server-projects/domain/milestones';
import { createProjectMilestoneApplication, type ProjectMilestoneRepository } from '@erpx/server-projects/application/milestones';
import { requireProjectPurchasedRead } from '@erpx/server-projects/adapters/purchases';
async function inward<T>(action: () => Promise<T>) { try {
    return await action();
}
catch (error) {
    if (error instanceof ApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
async function exposed<T>(action: () => Promise<T>) { try {
    return await action();
}
catch (error) {
    if (error instanceof BusinessRuleError) {
        const out = new ApiError(error.status, error.code, error.message);
        out.cause = error.cause;
        throw out;
    }
    throw error;
} }
function application(db: SqlDatabase) {
    const repository: ProjectMilestoneRepository<SqlStatement> = {
        async milestoneRows(companyId: string, projectId: string) {
            return db.prepare(`SELECT m.*,p.legal_name owner_name,
      dependency.depends_on_ids,dependency.depends_on_titles,dependency.blocked_by_titles,
      (m.state IN ('planned','in_progress') AND m.due_at<floor(extract(epoch FROM clock_timestamp())*1000)::bigint) is_overdue
    FROM staff_project_milestones m
    ${projectpeople_payrollProjection.milestoneOwner}
    LEFT JOIN LATERAL (
      SELECT COALESCE(jsonb_agg(pr.id ORDER BY pr.due_at,pr.title),'[]'::jsonb)::text depends_on_ids,
        COALESCE(jsonb_agg(pr.title ORDER BY pr.due_at,pr.title),'[]'::jsonb)::text depends_on_titles,
        COALESCE(jsonb_agg(pr.title ORDER BY pr.due_at,pr.title) FILTER (WHERE pr.state<>'completed'),'[]'::jsonb)::text blocked_by_titles
      FROM staff_project_milestone_dependencies d
      JOIN staff_project_milestones pr ON pr.id=d.predecessor_milestone_id AND pr.company_id=d.company_id AND pr.project_id=d.project_id
      WHERE d.company_id=m.company_id AND d.project_id=m.project_id AND d.successor_milestone_id=m.id
    ) dependency ON true
    WHERE m.company_id=? AND m.project_id=? ORDER BY m.due_at,m.id`)
                .bind(companyId, projectId).all<Omit<ProjectMilestoneRow, "depends_on_ids" | "depends_on_titles" | "blocked_by_titles"> & {
                depends_on_ids: string;
                depends_on_titles: string;
                blocked_by_titles: string;
            }>();
        } };
    return createProjectMilestoneApplication({ repository, getProject: (...args) => inward(() => getProject(db, ...args)) });
}
export function projectConstraint(error: unknown): never { try {
    return application(null as unknown as SqlDatabase).projectConstraint(error);
}
catch (failure) {
    if (failure instanceof BusinessRuleError) {
        const out = new ApiError(failure.status, failure.code, failure.message);
        out.cause = failure.cause;
        throw out;
    }
    throw failure;
} }
export async function listProjectMilestones(db: SqlDatabase, actor: StaffActor, projectId: string) { await requireProjectPurchasedRead(db,actor,projectId);return exposed(() => application(db).listProjectMilestones(actor, projectId)); }
