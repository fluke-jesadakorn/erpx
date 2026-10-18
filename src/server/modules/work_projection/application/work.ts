import type {WorkContext} from "../../identity_access/public/authorization";
import type {WorkRow} from "@/lib/work_projection/work-queue";
import type {approvalView} from "../../workflow/public/approvals";
import type {getLeave} from "../../people_payroll/public/leave";
type WorkReadResult<Row>={results:Row[]};
export type WorkReadUnit={pendingLeave(facts:{companyId:string;}):Promise<WorkReadResult<{ id: string; }>>;visibleLeave(facts:{companyId:string;userId:string;assignmentId:string;}):Promise<WorkReadResult<WorkRow>>;evidenceSets(facts:{recordId:string;companyId:string;}):Promise<WorkReadResult<{ id: string; set_id: string; purpose_key: string; original_name: string; bytes: number | null; media_type: string | null; source_state: string | null; link_state: string; preview_available: boolean; created_at: number; }>>;legacyEvidence(facts:{recordId:string;companyId:string;}):Promise<WorkReadResult<{ id: string; original_name: string; bytes: number; media_type: string; set_id: string | null; purpose_key: string; source_state: string; link_state: string; preview_available: boolean; created_at: number; }>>;approval(actor:WorkContext,id:string):ReturnType<typeof approvalView>;leave(holdingId:string,id:string,actor:WorkContext):ReturnType<typeof getLeave>;evidenceAccess(actor:WorkContext,input:{leaveRequestId:string;purposeKey:"medical_certificate";operation:"read"}):Promise<unknown>;};
export function createWorkRead(ports:{resource(holdingId:string):Promise<WorkReadUnit>;readable():boolean}){
async function getWork(holdingId: string, context: WorkContext) {
  const write = await ports.resource(holdingId);
  const pending = (
    await write.pendingLeave({companyId:context.companyId})
  ).results;
  for (const row of pending)
    await write.approval(context,row.id);
  const result = await write.visibleLeave({companyId:context.companyId,userId:context.userId,assignmentId:context.assignmentId});
  const rows = await Promise.all(
    result.results.map(async (row) => {
      const request = await write.leave(holdingId,row.id,context);
      const actions=request.permittedActions.filter(action=>!request.purchase||(["save","submit"].includes(action)?request.purchase.newWork:request.purchase.settleExisting));
      const actionable = actions.some((a) =>
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
        isApproval: actions.includes("approve") || actions.includes("approve_cancellation"),
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
async function getLeaveEvidence(
  holdingId: string,
  recordId: string,
  context: WorkContext,
) {
  const write = await ports.resource(holdingId);
  await write.evidenceAccess(context, {
    leaveRequestId: recordId,
    purposeKey: "medical_certificate",
    operation: "read",
  });
  if (ports.readable()) {
    return (
      await write.evidenceSets({recordId:recordId,companyId:context.companyId})
    ).results;
  }
  return (
    await write.legacyEvidence({recordId:recordId,companyId:context.companyId})
  ).results;
}

return {getWork,getLeaveEvidence};}
