import "server-only";
import { holdingResources } from "./runtime";
import type { WorkContext } from "./authorization";
import { participantSql, approvalView } from "./approvals";
import { getLeave } from "./leave";
import { assertLeaveAttachmentAccess } from "./leave-evidence";
import {
  attachmentsV2Readable,
  attachmentsV2ReadOnly,
} from "./attachments/feature";
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
      `SELECT l.id,l.number,l.state,l.start_date,l.days,l.created_by,l.updated_at,l.handover_note,l.cancellation_state FROM leave_requests l WHERE l.company_id=? AND (l.created_by=? OR l.decided_by=? OR ${participantSql("leave", "l", "?", "?")} OR ${participantSql("leave_cancellation", "l", "?", "?")}) ORDER BY l.updated_at DESC,l.id LIMIT 250`,
    )
    .bind(context.companyId, context.userId, context.userId, context.userId, context.assignmentId, context.userId, context.assignmentId)
    .all<WorkRow>();
  const rows = await Promise.all(
    result.results.map(async (row) => {
      const request = await getLeave(holdingId, row.id, context);
      const actionable = request.permittedActions.some((a) =>
        ["save", "submit", "approve", "reject", "approve_cancellation", "reject_cancellation"].includes(a),
      );
      const queue: "todo" | "waiting" | "completed" = actionable
        ? "todo"
        : request.cancellation_state === "pending_review" ? "waiting" : ["approved", "cancelled"].includes(row.state)
          ? "completed"
          : "waiting";
      return {
        ...row,
        queue,
        isApproval: request.permittedActions.includes("approve") || request.permittedActions.includes("approve_cancellation"),
        approvalStage: (request.cancellationApproval??request.approval).currentApprover?.authority ?? null,
        currentApprover:
          (request.cancellationApproval??request.approval).currentApprover?.approver_name ?? null,
        blockedReason: (request.cancellationApproval??request.approval).blockedReason,
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
  const { write } = await holdingResources(holdingId);
  await assertLeaveAttachmentAccess(write, context, {
    leaveRequestId: recordId,
    purposeKey: "medical_certificate",
    operation: "read",
  });
  if (attachmentsV2Readable()) {
    return (
      await write
        .prepare(
          `SELECT link.id,link.set_id,set_row.purpose_key,
             COALESCE(source.original_name,intent.original_name,'Pending file') original_name,
             COALESCE(source.bytes,intent.actual_bytes,intent.declared_bytes) bytes,
             COALESCE(source.detected_media_type,source.media_type,intent.detected_media_type,intent.media_type) media_type,
             source.source_state,link.state link_state,
             (source.preview_object_key IS NOT NULL) preview_available,
             link.created_at
           FROM attachment_sets set_row
           JOIN attachment_links link ON link.set_id=set_row.id
           LEFT JOIN attachments source
             ON source.id=link.attachment_id AND source.company_id=set_row.company_id
           LEFT JOIN upload_intents intent
             ON intent.id=link.upload_intent_id AND intent.company_id=set_row.company_id
           WHERE set_row.target_type='leave_request' AND set_row.target_id=?
             AND set_row.company_id=?
             AND (link.attachment_id IS NULL OR source.id IS NOT NULL)
             AND (link.upload_intent_id IS NULL OR intent.id IS NOT NULL)
             AND set_row.state${attachmentsV2ReadOnly() ? "='bound'" : " IN ('draft','bound')"}
             AND link.state${attachmentsV2ReadOnly() ? "='bound'" : " IN ('draft','bound')"}
             ${attachmentsV2ReadOnly() ? "AND source.source_state='available'" : ""}
           ORDER BY link.created_at DESC,link.id DESC`,
        )
        .bind(recordId, context.companyId)
        .all<{
          id: string;
          set_id: string;
          purpose_key: string;
          original_name: string;
          bytes: number | null;
          media_type: string | null;
          source_state: string | null;
          link_state: string;
          preview_available: boolean;
          created_at: number;
        }>()
    ).results;
  }
  return (
    await write
      .prepare(
        "SELECT id,NULL::text set_id,purpose purpose_key,original_name,bytes,COALESCE(detected_media_type,media_type) media_type,source_state,'bound' link_state,(preview_object_key IS NOT NULL) preview_available,created_at FROM attachments WHERE owner_type='leave_request' AND owner_id=? AND company_id=? AND revoked_at IS NULL ORDER BY created_at DESC",
      )
      .bind(recordId, context.companyId)
      .all<{
        id: string;
        original_name: string;
        bytes: number;
        media_type: string;
        set_id: string | null;
        purpose_key: string;
        source_state: string;
        link_state: string;
        preview_available: boolean;
        created_at: number;
      }>()
  ).results;
}
