import "server-only";
import type { SqlDatabase } from "./runtime/types";
import { scheduledFeaturePredicate } from "./hospitality-rollout";

export type StaffTaskAlertResult = {
  dueSoon: number;
  overdue: number;
};

type TaskAlertRow = {
  id: string;
  company_id: string;
  title: string;
  due_at: number;
  version: number;
  assignee_user_id: string;
  creator_user_id: string;
  project_owner_id: string | null;
  delegate_user_id: string | null;
};

/**
 * Emits one durable due-soon alert and one durable overdue escalation for each
 * task version. The notification unique key makes retries safe; notification
 * visibility still re-checks the task's current authorization before display.
 */
export async function escalateStaffTasks(
  db: SqlDatabase,
  now = Date.now(),
  warningWindowMs = 86_400_000,
): Promise<StaffTaskAlertResult> {
  const rows = (
    await db
      .prepare(
        `SELECT t.id,t.company_id,t.title,t.due_at,t.version,t.assignee_user_id,
          t.creator_user_id,p.owner_user_id project_owner_id,d.delegate_user_id
         FROM staff_tasks t
         LEFT JOIN staff_projects p ON p.id=t.project_id AND p.company_id=t.company_id
         LEFT JOIN LATERAL (SELECT sd.delegate_user_id FROM staff_task_delegations sd
           WHERE sd.task_id=t.id AND sd.company_id=t.company_id AND sd.status='active'
             AND sd.starts_at<=? AND sd.expires_at>?
           ORDER BY sd.created_at DESC,sd.id DESC LIMIT 1) d ON TRUE
         WHERE t.state NOT IN ('completed','cancelled') AND t.due_at<=?
           AND ${scheduledFeaturePredicate("t.company_id","operations")}
         ORDER BY t.due_at,t.id
         LIMIT 500`,
      )
      .bind(now, now, now + warningWindowMs)
      .all<TaskAlertRow>()
  ).results;

  let dueSoon = 0;
  let overdue = 0;
  for (const task of rows) {
    const isOverdue = Number(task.due_at) < now;
    const phase = isOverdue ? "overdue" : "due-soon";
    const message = isOverdue
      ? `Task overdue: ${task.title}. Follow up or escalate the owner.`
      : `Task due within 24 hours: ${task.title}.`;
    const recipients = new Set<string>([
      task.assignee_user_id,
      ...(task.delegate_user_id ? [task.delegate_user_id] : []),
      ...(isOverdue ? [task.creator_user_id, task.project_owner_id ?? ""] : []),
    ]);
    const approvers = await db
      .prepare(
        `SELECT s.approver_user_id
           FROM approval_instances i
           JOIN approval_steps s ON s.instance_id=i.id AND s.state='pending'
          WHERE i.company_id=? AND i.record_type='task' AND i.record_id=? AND i.state='pending'`,
      )
      .bind(task.company_id, task.id)
      .all<{ approver_user_id: string }>();
    for (const approver of approvers.results) recipients.add(approver.approver_user_id);
    recipients.delete("");
    for (const userId of recipients) {
      const result = await db
        .prepare(
          `INSERT INTO staff_notifications
             (id,company_id,user_id,record_type,record_id,record_version,message,created_at)
           SELECT ?,?,?,?,?,?,?,? WHERE ${scheduledFeaturePredicate("?","operations")}
           ON CONFLICT DO NOTHING`,
        )
        .bind(
          `staff-task-alert-${phase}-${task.id}-${task.version}-${userId}`,
          task.company_id,
          userId,
          "staff_task_alert",
          task.id,
          task.version,
          message,
          now,
          ...(process.env.NODE_ENV === "production" ? [task.company_id] : []),
        )
        .run();
      if (Number(result.meta.changes) > 0) {
        if (isOverdue) overdue += 1;
        else dueSoon += 1;
      }
    }
  }
  return { dueSoon, overdue };
}
