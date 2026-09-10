import "server-only";
import { holdingResources } from "./runtime";
import type { WorkContext } from "./authorization";
import { participantSql, approvalView } from "./approvals";
import { getLeave } from "./leave";
import type { WorkRow } from "@/lib/work-queue";
export async function getWork(holdingId: string, context: WorkContext) {
  const { write } = await holdingResources(holdingId);
  const pending = (
    await write
      .prepare(
        "SELECT id FROM leave_requests WHERE company_id=? AND state='pending_approval' LIMIT 250",
      )
      .bind(context.companyId)
      .all<{ id: string }>()
  ).results;
  for (const row of pending)
    await approvalView(write, "leave", row.id, context);
  const result = await write
    .prepare(
      `SELECT l.id,l.number,l.state,l.start_date,l.days,l.created_by,l.updated_at,l.handover_note FROM leave_requests l WHERE l.company_id=? AND (l.created_by=? OR l.decided_by=? OR ${participantSql("leave", "l")}) ORDER BY l.updated_at DESC,l.id LIMIT 250`,
    )
    .bind(context.companyId, context.userId, context.userId, context.userId)
    .all<WorkRow>();
  const rows = await Promise.all(
    result.results.map(async (row) => {
      const request = await getLeave(holdingId, row.id, context);
      const actionable = request.permittedActions.some((a) =>
        ["save", "submit", "approve", "reject"].includes(a),
      );
      const queue: "todo" | "waiting" | "completed" = actionable
        ? "todo"
        : ["approved", "cancelled"].includes(row.state)
          ? "completed"
          : "waiting";
      return {
        ...row,
        queue,
        isApproval: request.permittedActions.includes("approve"),
        approvalStage: request.approval.currentApprover?.authority ?? null,
        currentApprover:
          request.approval.currentApprover?.approver_name ?? null,
        blockedReason: request.approval.blockedReason,
      };
    }),
  );
  return {
    rows,
    counts: {
      todo: rows.filter((r) => r.queue === "todo").length,
      waiting: rows.filter((r) => r.queue === "waiting").length,
      completed: rows.filter((r) => r.queue === "completed").length,
      all: rows.length,
    },
  };
}
export async function getLeaveEvidence(
  holdingId: string,
  recordId: string,
  context: WorkContext,
) {
  await getLeave(holdingId, recordId, context);
  const { write } = await holdingResources(holdingId);
  return (
    await write
      .prepare(
        "SELECT id,original_name,bytes,media_type FROM attachments WHERE owner_type='leave_request' AND owner_id=? AND revoked_at IS NULL ORDER BY created_at DESC",
      )
      .bind(recordId)
      .all<{
        id: string;
        original_name: string;
        bytes: number;
        media_type: string;
      }>()
  ).results;
}
