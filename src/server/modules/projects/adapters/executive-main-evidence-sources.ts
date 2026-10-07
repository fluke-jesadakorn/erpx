import { inventoryExecutiveOwnerProjections as financeProjectionSources } from "../../finance/public/inventory-executive-owner-projections";
import { inventoryExecutiveOwnerProjections as people_payrollProjectionSources } from "../../people_payroll/public/inventory-executive-owner-projections";
import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { projectDependencyBlockerCohort, projectMilestoneOverdueCohort } from "@/server/modules/runtime/public/executive/project-milestone-access";
import { ProjectMilestoneEvidenceRow, ProjectDependencyEvidenceRow, ProjectActualEvidenceRow } from '@/server/modules/runtime/public/executive/evidence-contract';
function projectActualScope() {
    return `p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?)
    AND (iam_same_person(p.owner_user_id,?,p.company_id) OR EXISTS(
      SELECT 1 FROM staff_project_members member WHERE member.project_id=p.id AND iam_same_person(member.user_id,?,p.company_id)))`;
}
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
            const rows = (await db.prepare(`SELECT m.id,m.project_id,p.title project_title,m.title,m.due_at,m.owner_user_id,
      COALESCE(h.legal_name,m.owner_user_id) owner_name,m.state,m.version
    FROM staff_project_milestones m JOIN staff_projects p ON p.id=m.project_id AND p.company_id=m.company_id
    LEFT JOIN (${people_payrollProjectionSources.hr_people}) h ON h.company_id=m.company_id AND h.user_id=m.owner_user_id AND h.merged_into_id IS NULL
    WHERE ${conditions.join(" AND ")} ORDER BY m.due_at,m.id LIMIT ?`)
                .bind(...values, input.limit + 1).all<ProjectMilestoneEvidenceRow>()).results;
            return { rows };
        }, async factsReadProjectMilestoneEvidence(actor: WorkContext, filters: {
            from: string;
            to: string;
            currency: string;
        }, referenceAt: number, id: string): Promise<{
            row: ProjectMilestoneEvidenceRow | null;
        }> {
            const cohort = projectMilestoneOverdueCohort(actor, filters, referenceAt);
            const row = await db.prepare(`SELECT m.id,m.project_id,p.title project_title,m.title,m.due_at,m.owner_user_id,
      COALESCE(h.legal_name,m.owner_user_id) owner_name,m.state,m.version
    FROM staff_project_milestones m JOIN staff_projects p ON p.id=m.project_id AND p.company_id=m.company_id
    LEFT JOIN (${people_payrollProjectionSources.hr_people}) h ON h.company_id=m.company_id AND h.user_id=m.owner_user_id AND h.merged_into_id IS NULL
    WHERE ${cohort.sql} AND m.id=? LIMIT 1`).bind(...cohort.values, id).first<ProjectMilestoneEvidenceRow>();
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
            const cohort = projectDependencyBlockerCohort(actor, input.filters);
            const conditions = [cohort.sql];
            const values: unknown[] = [...cohort.values];
            if (input.query) {
                conditions.push(`(position(lower(?) in lower(m.title))>0 OR position(lower(?) in lower(p.title))>0 OR EXISTS(
      SELECT 1 FROM staff_project_milestone_dependencies search_dependency
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
            const rows = (await db.prepare(`SELECT m.id,m.project_id,p.title project_title,m.title,m.due_at,m.state,m.version,
      COALESCE(blockers.items,'[]'::jsonb)::text blockers_json
    FROM staff_project_milestones m JOIN staff_projects p ON p.id=m.project_id AND p.company_id=m.company_id
    LEFT JOIN LATERAL (
      SELECT jsonb_agg(jsonb_build_object('id',pr.id,'title',pr.title,'state',pr.state,'version',pr.version) ORDER BY pr.due_at,pr.title) items
      FROM staff_project_milestone_dependencies dependency
      JOIN staff_project_milestones pr ON pr.id=dependency.predecessor_milestone_id AND pr.company_id=dependency.company_id AND pr.project_id=dependency.project_id
      WHERE dependency.company_id=p.company_id AND dependency.project_id=p.id AND dependency.successor_milestone_id=m.id AND pr.state<>'completed'
    ) blockers ON true
    WHERE ${conditions.join(" AND ")} ORDER BY m.due_at,m.id LIMIT ?`)
                .bind(...values, input.limit + 1).all<ProjectDependencyEvidenceRow>()).results;
            return { rows };
        }, async factsReadProjectDependencyEvidence(actor: WorkContext, filters: {
            from: string;
            to: string;
            currency: string;
        }, id: string): Promise<{
            row: ProjectDependencyEvidenceRow | null;
        }> {
            const cohort = projectDependencyBlockerCohort(actor, filters);
            const row = await db.prepare(`SELECT m.id,m.project_id,p.title project_title,m.title,m.due_at,m.state,m.version,
      COALESCE(blockers.items,'[]'::jsonb)::text blockers_json
    FROM staff_project_milestones m JOIN staff_projects p ON p.id=m.project_id AND p.company_id=m.company_id
    LEFT JOIN LATERAL (
      SELECT jsonb_agg(jsonb_build_object('id',pr.id,'title',pr.title,'state',pr.state,'version',pr.version) ORDER BY pr.due_at,pr.title) items
      FROM staff_project_milestone_dependencies dependency
      JOIN staff_project_milestones pr ON pr.id=dependency.predecessor_milestone_id AND pr.company_id=dependency.company_id AND pr.project_id=dependency.project_id
      WHERE dependency.company_id=p.company_id AND dependency.project_id=p.id AND dependency.successor_milestone_id=m.id AND pr.state<>'completed'
    ) blockers ON true
    WHERE ${cohort.sql} AND m.id=? LIMIT 1`)
                .bind(...cohort.values, id).first<ProjectDependencyEvidenceRow>();
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
                sql = `SELECT l.id,l.project_id,p.title project_title,l.work_date::text work_date,l.work_date::text sort_date,
        l.minutes,l.user_id,l.state,l.version
      FROM staff_project_work_logs l JOIN staff_projects p ON p.id=l.project_id AND p.company_id=l.company_id`;
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
                sql = `SELECT a.id,a.project_id,p.title project_title,j.entry_date::text entry_date,j.entry_date::text sort_date,
        j.id journal_id,j.journal_number,j.currency,ac.code account_code,ac.name account_name,
        a.amount_minor::text amount_minor,a.state,a.version
      FROM staff_project_cost_allocations a JOIN staff_projects p ON p.id=a.project_id AND p.company_id=a.company_id
      JOIN (${financeProjectionSources.finance_journal_lines}) l ON l.id=a.journal_line_id JOIN (${financeProjectionSources.finance_journals}) j ON j.id=l.journal_id
      JOIN (${financeProjectionSources.finance_accounts}) ac ON ac.id=l.account_id`;
            }
            if (cursor) {
                conditions.push(`(${dateColumn},${idColumn})>(?,?)`);
                values.push(cursor.date, cursor.id);
            }
            const rows = (await db.prepare(`${sql} WHERE ${conditions.join(" AND ")} ORDER BY ${dateColumn},${idColumn} LIMIT ?`)
                .bind(...values, input.limit + 1).all<ProjectActualEvidenceRow>()).results;
            return { rows };
        }, async factsReadProjectActualEvidence(metric: "project-actual-hours" | "project-posted-cost", actor: WorkContext, id: string, filters: {
            from: string;
            to: string;
            currency: string;
        }): Promise<{
            row: ProjectActualEvidenceRow | null;
        }> {
            const scope = projectActualScope();
            let row: ProjectActualEvidenceRow | null;
            if (metric === "project-actual-hours") {
                row = await db.prepare(`SELECT l.id,l.project_id,p.title project_title,l.work_date::text work_date,l.work_date::text sort_date,
        l.minutes,l.user_id,l.state,l.version
      FROM staff_project_work_logs l JOIN staff_projects p ON p.id=l.project_id AND p.company_id=l.company_id
      WHERE ${scope} AND l.id=? AND l.state='approved' AND l.work_date BETWEEN ? AND ?
        AND (iam_same_person(p.owner_user_id,?,p.company_id) OR iam_same_person(l.user_id,?,p.company_id)) LIMIT 1`)
                    .bind(actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, actor.userId, actor.userId, id, filters.from, filters.to, actor.userId, actor.userId).first<ProjectActualEvidenceRow>();
            }
            else {
                row = await db.prepare(`SELECT a.id,a.project_id,p.title project_title,j.entry_date::text entry_date,j.entry_date::text sort_date,
        j.id journal_id,j.journal_number,j.currency,l.id journal_line_id,ac.code account_code,ac.name account_name,
        a.amount_minor::text amount_minor,a.state,a.version
      FROM staff_project_cost_allocations a JOIN staff_projects p ON p.id=a.project_id AND p.company_id=a.company_id
      JOIN (${financeProjectionSources.finance_journal_lines}) l ON l.id=a.journal_line_id JOIN (${financeProjectionSources.finance_journals}) j ON j.id=l.journal_id
      JOIN (${financeProjectionSources.finance_accounts}) ac ON ac.id=l.account_id
      WHERE ${scope} AND a.id=? AND a.state='approved' AND j.status='posted' AND ac.category IN ('expense','other_expense')
        AND j.currency=? AND j.entry_date BETWEEN ? AND ? LIMIT 1`)
                    .bind(actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, actor.userId, actor.userId, id, filters.currency, filters.from, filters.to).first<ProjectActualEvidenceRow>();
            }
            return { row };
        } };
}
