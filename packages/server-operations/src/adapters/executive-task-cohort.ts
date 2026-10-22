import { finalRootOwnerProjectionSources as collaborationProjectionSources } from "@erpx/server-collaboration/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as projectsProjectionSources } from "@erpx/server-projects/public/final-root-owner-projection-sources";
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { participantSql } from "@erpx/server-workflow/public/approvals";
import { socialPostScope } from "@erpx/server-collaboration/public/social-source";

export const operationsMetrics = [
  "open-operations-exceptions",
  "exception-resolution-time",
  "operations-escalations",
] as const;

export type OperationsMetricId = (typeof operationsMetrics)[number];

/**
 * Match the row boundary used by listTasks/getTask: the current reader must be
 * related to the task, and a conversation-derived task also requires access to
 * its source post. Executive never receives descriptions or task notes.
 */
export function operationsTaskVisibility(actor: WorkContext, referenceAt = Date.now()) {
  const posts = socialPostScope(actor, "p");
  const access = `(
    iam_same_person(t.creator_user_id,?,t.company_id)
    OR iam_same_person(t.assignee_user_id,?,t.company_id)
    OR iam_same_person((SELECT sd.delegate_user_id FROM staff_task_delegations sd WHERE sd.task_id=t.id AND sd.company_id=t.company_id
      AND sd.status='active' AND sd.starts_at<=? AND sd.expires_at>?
      ORDER BY sd.created_at DESC,sd.id DESC LIMIT 1),?,t.company_id)
    OR EXISTS(SELECT 1 FROM (${projectsProjectionSources.staff_projects}) sp WHERE sp.id=t.project_id AND sp.company_id=t.company_id
      AND iam_same_person(sp.owner_user_id,?,t.company_id))
    OR EXISTS(SELECT 1 FROM (${projectsProjectionSources.staff_project_members}) pm WHERE pm.project_id=t.project_id AND pm.user_id=?)
    OR EXISTS(SELECT 1 FROM erpx_json_each(t.start_approvers_json) WHERE value=?)
    OR EXISTS(SELECT 1 FROM erpx_json_each(t.completion_approvers_json) WHERE value=?)
    OR ${participantSql("task", "t")}
  )`;
  return {
    sql: `t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?)
      AND ${access}
      AND (NOT EXISTS(SELECT 1 FROM (${collaborationProjectionSources.social_task_origins}) ox WHERE ox.task_id=t.id)
        OR EXISTS(SELECT 1 FROM (${collaborationProjectionSources.social_task_origins}) o JOIN (${collaborationProjectionSources.social_posts}) p ON p.id=o.post_id
          WHERE o.task_id=t.id AND ${posts.sql}))`,
    values: [
      actor.companyId,
      actor.branchId,
      actor.branchId,
      actor.departmentId,
      actor.departmentId,
      actor.userId,
      actor.userId,
      referenceAt,
      referenceAt,
      actor.userId,
      actor.userId,
      actor.userId,
      actor.userId,
      actor.userId,
      actor.userId,
      ...posts.bindings,
    ],
  };
}

export function operationsTaskCohort(
  metric: OperationsMetricId,
  alias: string,
  actor: WorkContext,
  filters: { from: string; to: string },
  referenceAt: number,
) {
  if (metric === "exception-resolution-time") {
    return {
      sql: `${alias}.state='completed' AND ${alias}.due_at<${alias}.updated_at
        AND ${alias}.updated_at>=? AND ${alias}.updated_at<?`,
      values: [Date.parse(filters.from), Date.parse(filters.to) + 86_400_000],
    };
  }

  const overdue = `${alias}.state NOT IN ('completed','cancelled') AND ${alias}.due_at<?`;
  if (metric === "operations-escalations") {
    return {
      sql: `${overdue} AND EXISTS(SELECT 1 FROM (${collaborationProjectionSources.staff_notifications}) n
        WHERE n.company_id=${alias}.company_id AND n.user_id=? AND n.record_type='staff_task_alert'
          AND n.record_id=${alias}.id AND n.record_version=${alias}.version
          AND n.id LIKE 'staff-task-alert-overdue-%')`,
      values: [referenceAt, actor.userId],
    };
  }
  return { sql: overdue, values: [referenceAt] };
}

export function operationsTaskHref(id: string) {
  return `/staff-tasks/${encodeURIComponent(id)}`;
}
