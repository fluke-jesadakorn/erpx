import type { SqlDatabase } from "./runtime/types";
import { getEmployeeRequest } from "./employee-requests";
import { readablePost } from "./social-source";
import type { WorkContext } from "./authorization";
import { getLeave } from "./leave";
import { getExpense } from "./expenses";
import { getTask } from "./staff-tasks";
import { ApiError } from "./http";
export type StaffNotification = {
  id: string;
  record_type: string;
  record_id: string;
  record_version: number;
  message: string;
  created_at: number;
  read_at: number | null;
  href: string;
};
export async function listStaffNotifications(
  db: SqlDatabase,
  actor: WorkContext,
) {
  const rows = (
    await db
      .prepare(
        "SELECT * FROM staff_notifications WHERE company_id=? AND user_id=? ORDER BY created_at DESC,id LIMIT 100",
      )
      .bind(actor.companyId, actor.userId)
      .all<Omit<StaffNotification, "href">>()
  ).results;
  const visible: StaffNotification[] = [];
  for (const row of rows) {
    try {
      if (row.record_type === "leave")
        await getLeave(actor.holdingId, row.record_id, actor);
      else if (row.record_type === "expense")
        await getExpense(actor, row.record_id);
      else if (row.record_type === "request") await getEmployeeRequest(db,actor,row.record_id);
      else if (row.record_type === "task")
        await getTask(db, actor, row.record_id);
      else if (row.record_type === "post") await readablePost(db,actor,row.record_id);
      else continue;
      visible.push({
        ...row,
        href: row.record_type === "request" ? `/requests/${row.record_id}` : row.record_type === "post" ? `/posts/${row.record_id}` :
          row.record_type === "leave"
            ? `/tasks/people_payroll/${row.record_id}`
            : row.record_type === "expense"
              ? `/expenses/${row.record_id}`
              : `/staff-tasks/${row.record_id}`,
      });
    } catch (error) {
      if (!(error instanceof ApiError) || ![403, 404].includes(error.status))
        throw error;
    }
  }
  return visible;
}
