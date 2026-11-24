import { finalRootOwnerProjectionSources as collaborationProjectionSources } from "@erpx/server-collaboration/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as projectsProjectionSources } from "@erpx/server-projects/public/final-root-owner-projection-sources";
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {participantSql} from '@erpx/server-workflow/public/approval-participant-projection';
import {socialPostScope} from '@erpx/server-collaboration/public/social-source-policy';
import {OPERATIONS_TASK_START_APPROVER_SQL,OPERATIONS_TASK_COMPLETION_APPROVER_SQL} from './task-approver-access-source';
import {operationsActiveDelegationSql} from './task-delegation-source';
import {operationsSocialTaskOriginExists} from './social-task-origin-exists';

/** Canonical overdue-alert notification probe shared by the cohort visibility
 * and executive evidence readers. Projection/alias/separator text is
 * parameterized so each caller renders byte-identical SQL; binds unchanged. */
export const operationsOverdueAlertSql = (
  projection: string,
  alias: string,
  whereSep = ' ',
  andSep = ' ',
) =>
  `SELECT ${projection} FROM (${collaborationProjectionSources.staff_notifications}) n${whereSep}WHERE n.company_id=${alias}.company_id AND n.user_id=? AND n.record_type='staff_task_alert'${andSep}AND n.record_id=${alias}.id AND n.record_version=${alias}.version${andSep}AND n.id LIKE 'staff-task-alert-overdue-%'`;

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
    OR iam_same_person((${operationsActiveDelegationSql('sd.delegate_user_id','sd','t.id','t.company_id','?','\n      ORDER BY sd.created_at DESC,sd.id DESC LIMIT 1','\n      ')},?,t.company_id)
    OR EXISTS(SELECT 1 FROM (${projectsProjectionSources.staff_projects}) sp WHERE sp.id=t.project_id AND sp.company_id=t.company_id
      AND iam_same_person(sp.owner_user_id,?,t.company_id))
    OR EXISTS(SELECT 1 FROM (${projectsProjectionSources.staff_project_members}) pm WHERE pm.project_id=t.project_id AND pm.user_id=?)
    OR ${OPERATIONS_TASK_START_APPROVER_SQL}
    OR ${OPERATIONS_TASK_COMPLETION_APPROVER_SQL}
    OR ${participantSql("task", "t")}
  )`;
  return {
    sql: `t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?)
      AND ${access}
      AND (NOT ${operationsSocialTaskOriginExists('ox','t.id')}
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
      sql: `${overdue} AND EXISTS(${operationsOverdueAlertSql('1', alias, '\n        ', '\n          ')})`,
      values: [referenceAt, actor.userId],
    };
  }
  return { sql: overdue, values: [referenceAt] };
}

export function operationsTaskHref(id: string) {
  return `/staff-tasks/${encodeURIComponent(id)}`;
}
