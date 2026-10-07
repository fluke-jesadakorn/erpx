import "server-only";
import type { Filters } from "@/lib/executive/canvas";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";

/** Shared current membership and due-time cohort for metric, search, and detail reads. */
export function projectMilestoneOverdueCohort(actor: WorkContext, filters: Filters, referenceAt: number, milestone = "m", project = "p") {
  return {
    sql: `${project}.company_id=? AND (?::text IS NULL OR ${project}.branch_id=?) AND (?::text IS NULL OR ${project}.department_id=?)
      AND (${project}.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members member WHERE member.project_id=${project}.id AND member.user_id=?))
      AND ${milestone}.company_id=${project}.company_id AND ${milestone}.state IN ('planned','in_progress')
      AND ${milestone}.due_at<? AND ${milestone}.due_at>=? AND ${milestone}.due_at<?`,
    values: [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId,
      actor.userId, actor.userId, referenceAt, Date.parse(filters.from), Date.parse(filters.to) + 86_400_000],
  };
}

/** Count unique, current incomplete milestones that still have an incomplete prerequisite. */
export function projectDependencyBlockerCohort(actor: WorkContext, filters: Filters, milestone = "m", project = "p") {
  return {
    sql: `${project}.company_id=? AND (?::text IS NULL OR ${project}.branch_id=?) AND (?::text IS NULL OR ${project}.department_id=?)
      AND (iam_same_person(${project}.owner_user_id,?,${project}.company_id) OR EXISTS(
        SELECT 1 FROM staff_project_members member WHERE member.project_id=${project}.id
          AND iam_same_person(member.user_id,?,${project}.company_id)))
      AND ${milestone}.company_id=${project}.company_id AND ${milestone}.state IN ('planned','in_progress')
      AND ${milestone}.due_at>=? AND ${milestone}.due_at<?
      AND EXISTS(SELECT 1 FROM staff_project_milestone_dependencies dependency
        JOIN staff_project_milestones prerequisite ON prerequisite.id=dependency.predecessor_milestone_id
          AND prerequisite.company_id=dependency.company_id AND prerequisite.project_id=dependency.project_id
        WHERE dependency.company_id=${project}.company_id AND dependency.project_id=${project}.id
          AND dependency.successor_milestone_id=${milestone}.id AND prerequisite.state<>'completed')`,
    values: [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId,
      actor.userId, actor.userId, Date.parse(filters.from), Date.parse(filters.to) + 86_400_000],
  };
}

export function projectMilestoneHref(projectId: string) {
  return `/projects/${encodeURIComponent(projectId)}`;
}
