import {projectMetricSource} from "@erpx/projects-server/adapters/metric-purchases";
import {projectActualHoursFrom,projectMilestoneOwnerJoin,projectMilestoneProjectJoin,projectPostedCostFrom} from "./executive-evidence-chains";
import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { projectDependencyBlockerCohort, projectMilestoneOverdueCohort } from "@erpx/platform-runtime-server/public/executive/project-milestone-access";
import { projectOwnerOrMemberSql } from "@erpx/projects-server/adapters/project-read-projections";
import { evidenceProjectionHead } from './read-tail-r38';
import { projectDependencyRowsHead } from './read-tail-r43';
import { ProjectMilestoneEvidenceRow, ProjectDependencyEvidenceRow, ProjectActualEvidenceRow } from '@erpx/platform-runtime-server/public/executive/evidence-contract';
function projectActualScope() {
    return `p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?)
    AND ${projectOwnerOrMemberSql('p')}`;
}
/** Owner-local fixed projections for Projects executive evidence. The search
 * and by-id readers of each metric execute the identical projection column
 * list; one definition removes the redundant query text and stops the two
 * call sites from drifting apart. */
const milestoneEvidenceSelect = evidenceProjectionHead('milestone');
const dependencyEvidenceSelect = evidenceProjectionHead('dependency');
/** Byte-identical work-log evidence source shared by the paged search and by-id readers. */
const projectActualHoursEvidenceFrom = (sourceSql: string) => `${evidenceProjectionHead('actual-hours')}
      ${projectActualHoursFrom(sourceSql)}`;
/** Byte-identical posted-cost projection head shared by the paged search and by-id readers. */
const projectPostedCostEvidenceHead = evidenceProjectionHead('posted-cost');
/** Byte-identical blocker lateral shared by the milestone search and by-id readers. */
const projectDependencyBlockerLateral = (sourceSql: string) => `FROM staff_project_milestones m JOIN (${sourceSql}) p ON p.id=m.project_id AND p.company_id=m.company_id
    LEFT JOIN LATERAL (
      SELECT jsonb_agg(jsonb_build_object('id',pr.id,'title',pr.title,'state',pr.state,'version',pr.version) ORDER BY pr.due_at,pr.title) items
      FROM staff_project_milestone_dependencies dependency
      JOIN staff_project_milestones pr ON pr.id=dependency.predecessor_milestone_id AND pr.company_id=dependency.company_id AND pr.project_id=dependency.project_id
      WHERE dependency.company_id=p.company_id AND dependency.project_id=p.id AND dependency.successor_milestone_id=m.id AND pr.state<>'completed'
    ) blockers ON true
    `;
export function executiveMainEvidenceSources(db: SqlDatabase) {
    return { async factsSearchProjectMilestoneEvidence(actor: WorkContext, input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "overdue-project-milestones";
            cursor?: string | undefined;
        }, referenceAt: number, cursor: {
            dueAt: number;
            id: string;
        } | null): Promise<{
            rows: ProjectMilestoneEvidenceRow[];
        }> {
            const projectSource=await projectMetricSource(db,actor,"overdue-project-milestones");
            const cohort = projectMilestoneOverdueCohort(actor, input.filters, referenceAt);
            const conditions = [cohort.sql];
            const values: unknown[] = [...cohort.values];
            if (input.query) {
                conditions.push("(position(lower(?) in lower(m.title))>0 OR position(lower(?) in lower(p.title))>0 OR position(lower(?) in lower(h.legal_name))>0 OR position(lower(?) in lower(m.state))>0)");
                values.push(input.query, input.query, input.query, input.query);
            }
            if (cursor) {
                conditions.push("(m.due_at,m.id)>(?,?)");
                values.push(cursor.dueAt, cursor.id);
            }
            const rows = (await db.prepare(`${milestoneEvidenceSelect}
    FROM staff_project_milestones m ${projectMilestoneProjectJoin(projectSource.sql)}
    ${projectMilestoneOwnerJoin()}
    WHERE ${conditions.join(" AND ")} ORDER BY m.due_at,m.id LIMIT ?`)
                .bind(...projectSource.args,...values, input.limit + 1).all<ProjectMilestoneEvidenceRow>()).results;
            return { rows };
        }, async factsReadProjectMilestoneEvidence(actor: WorkContext, filters: {
            from: string;
            to: string;
            currency: string;
        }, referenceAt: number, id: string): Promise<{
            row: ProjectMilestoneEvidenceRow | null;
        }> {
            const projectSource=await projectMetricSource(db,actor,"overdue-project-milestones");
            const cohort = projectMilestoneOverdueCohort(actor, filters, referenceAt);
            const row = await db.prepare(`${milestoneEvidenceSelect}
    FROM staff_project_milestones m ${projectMilestoneProjectJoin(projectSource.sql)}
    ${projectMilestoneOwnerJoin()}
    WHERE ${cohort.sql} AND m.id=? LIMIT 1`).bind(...projectSource.args,...cohort.values, id).first<ProjectMilestoneEvidenceRow>();
            return { row };
        }, async factsSearchProjectDependencyEvidence(actor: WorkContext, input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "project-dependency-blockers";
            cursor?: string | undefined;
        }, cursor: {
            dueAt: number;
            id: string;
        } | null): Promise<{
            rows: ProjectDependencyEvidenceRow[];
        }> {
            const projectSource=await projectMetricSource(db,actor,"project-dependency-blockers");
            const cohort = projectDependencyBlockerCohort(actor, input.filters);
            const conditions = [cohort.sql];
            const values: unknown[] = [...cohort.values];
            if (input.query) {
                conditions.push(`(position(lower(?) in lower(m.title))>0 OR position(lower(?) in lower(p.title))>0 OR EXISTS(
      ${projectDependencyRowsHead}search_dependency
      JOIN staff_project_milestones prerequisite ON prerequisite.id=search_dependency.predecessor_milestone_id
      WHERE search_dependency.company_id=p.company_id AND search_dependency.project_id=p.id
        AND search_dependency.successor_milestone_id=m.id AND prerequisite.state<>'completed'
        AND position(lower(?) in lower(prerequisite.title))>0))`);
                values.push(input.query, input.query, input.query);
            }
            if (cursor) {
                conditions.push("(m.due_at,m.id)>(?,?)");
                values.push(cursor.dueAt, cursor.id);
            }
            const rows = (await db.prepare(`${dependencyEvidenceSelect}
    ${projectDependencyBlockerLateral(projectSource.sql)}WHERE ${conditions.join(" AND ")} ORDER BY m.due_at,m.id LIMIT ?`)
                .bind(...projectSource.args,...values, input.limit + 1).all<ProjectDependencyEvidenceRow>()).results;
            return { rows };
        }, async factsReadProjectDependencyEvidence(actor: WorkContext, filters: {
            from: string;
            to: string;
            currency: string;
        }, id: string): Promise<{
            row: ProjectDependencyEvidenceRow | null;
        }> {
            const projectSource=await projectMetricSource(db,actor,"project-dependency-blockers");
            const cohort = projectDependencyBlockerCohort(actor, filters);
            const row = await db.prepare(`${dependencyEvidenceSelect}
    ${projectDependencyBlockerLateral(projectSource.sql)}WHERE ${cohort.sql} AND m.id=? LIMIT 1`)
                .bind(...projectSource.args,...cohort.values, id).first<ProjectDependencyEvidenceRow>();
            return { row };
        }, async factsSearchProjectActualEvidence(metric: "project-actual-hours" | "project-posted-cost", input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "project-actual-hours" | "project-posted-cost";
            cursor?: string | undefined;
        }, actor: WorkContext, cursor: {
            date: string;
            id: string;
        } | null): Promise<{
            rows: ProjectActualEvidenceRow[];
        }> {
            const projectSource=await projectMetricSource(db,actor,metric);
            const scope = projectActualScope();
            const conditions = [scope];
            const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId,
                actor.departmentId, actor.departmentId, actor.userId, actor.userId];
            let sql: string;
            let idColumn: string;
            let dateColumn: string;
            if (metric === "project-actual-hours") {
                conditions.push("l.state='approved'", "l.work_date BETWEEN ? AND ?", "(iam_same_person(p.owner_user_id,?,p.company_id) OR iam_same_person(l.user_id,?,p.company_id))");
                values.push(input.filters.from, input.filters.to, actor.userId, actor.userId);
                if (input.query) {
                    conditions.push("(position(lower(?) in lower(p.title))>0 OR position(lower(?) in lower(l.work_date::text))>0 OR position(lower(?) in lower(l.user_id))>0)");
                    values.push(input.query, input.query, input.query);
                }
                dateColumn = "l.work_date::text";
                idColumn = "l.id";
                sql = projectActualHoursEvidenceFrom(projectSource.sql);
            }
            else {
                conditions.push("a.state='approved'", "j.status='posted'", "ac.category IN ('expense','other_expense')", "j.currency=?", "j.entry_date BETWEEN ? AND ?");
                values.push(input.filters.currency, input.filters.from, input.filters.to);
                if (input.query) {
                    conditions.push("(position(lower(?) in lower(p.title))>0 OR position(lower(?) in lower(j.journal_number))>0 OR position(lower(?) in lower(ac.code||' '||ac.name))>0)");
                    values.push(input.query, input.query, input.query);
                }
                dateColumn = "j.entry_date::text";
                idColumn = "a.id";
                sql = `${projectPostedCostEvidenceHead}ac.code account_code,ac.name account_name,
        a.amount_minor::text amount_minor,a.state,a.version
      ${projectPostedCostFrom(projectSource.sql)}`;
            }
            if (cursor) {
                conditions.push(`(${dateColumn},${idColumn})>(?,?)`);
                values.push(cursor.date, cursor.id);
            }
            const rows = (await db.prepare(`${sql} WHERE ${conditions.join(" AND ")} ORDER BY ${dateColumn},${idColumn} LIMIT ?`)
                .bind(...projectSource.args,...values, input.limit + 1).all<ProjectActualEvidenceRow>()).results;
            return { rows };
        }, async factsReadProjectActualEvidence(metric: "project-actual-hours" | "project-posted-cost", actor: WorkContext, id: string, filters: {
            from: string;
            to: string;
            currency: string;
        }): Promise<{
            row: ProjectActualEvidenceRow | null;
        }> {
            const projectSource=await projectMetricSource(db,actor,metric);
            const scope = projectActualScope();
            let row: ProjectActualEvidenceRow | null;
            if (metric === "project-actual-hours") {
                row = await db.prepare(`${projectActualHoursEvidenceFrom(projectSource.sql)}
      WHERE ${scope} AND l.id=? AND l.state='approved' AND l.work_date BETWEEN ? AND ?
        AND (iam_same_person(p.owner_user_id,?,p.company_id) OR iam_same_person(l.user_id,?,p.company_id)) LIMIT 1`)
                    .bind(...projectSource.args,actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, actor.userId, actor.userId, id, filters.from, filters.to, actor.userId, actor.userId).first<ProjectActualEvidenceRow>();
            }
            else {
                row = await db.prepare(`${projectPostedCostEvidenceHead}l.id journal_line_id,ac.code account_code,ac.name account_name,
        a.amount_minor::text amount_minor,a.state,a.version
      ${projectPostedCostFrom(projectSource.sql)}
      WHERE ${scope} AND a.id=? AND a.state='approved' AND j.status='posted' AND ac.category IN ('expense','other_expense')
        AND j.currency=? AND j.entry_date BETWEEN ? AND ? LIMIT 1`)
                    .bind(...projectSource.args,actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, actor.userId, actor.userId, id, filters.currency, filters.from, filters.to).first<ProjectActualEvidenceRow>();
            }
            return { row };
        } };
}
