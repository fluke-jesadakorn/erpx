import "server-only";
import type { Filters } from "@erpx/feature-shared/lib/executive/canvas";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { projectOwnerOrMemberColumnSql, projectOwnerOrMemberSql } from "@erpx/server-projects/adapters/project-read-projections";
import { projectDependencyRowsHead } from './read-tail-r43';

/** Shared current membership and due-time cohort for metric, search, and detail reads. */
export function projectMilestoneOverdueCohort(actor: WorkContext, filters: Filters, referenceAt: number, milestone = "m", project = "p") {
  return {
    sql: `${project}.company_id=? AND (?::text IS NULL OR ${project}.branch_id=?) AND (?::text IS NULL OR ${project}.department_id=?)
      AND ${projectOwnerOrMemberColumnSql(project,'?','member')}
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
      AND ${projectOwnerOrMemberSql(project)}
      AND ${milestone}.company_id=${project}.company_id AND ${milestone}.state IN ('planned','in_progress')
      AND ${milestone}.due_at>=? AND ${milestone}.due_at<?
      AND EXISTS(${projectDependencyRowsHead}dependency
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
