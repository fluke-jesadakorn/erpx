import Link from "next/link";
import { LeaveDecisions, LeaveEditor } from "@/components/self-service/leave-editor";
import styles from "@/components/leave/leave-flow.module.css";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import type { ApprovalView } from "@/server/approvals";
import { ApprovalHistory } from "@/components/modules/approval-history";
import { OperationSummary } from "@/components/workplace/operation-summary";
import { getLeave } from "@/server/leave";
import { leaveSummary } from "@/server/leave-self-service";
import { pageError } from "@/server/page-error";
import { getLeaveEvidence } from "@/server/work";
import {
  attachmentsV2Readable,
  attachmentsV2ReadOnly,
} from "@/server/attachments/feature";
import { parseLeaveEvidencePolicy } from "@/server/leave-evidence";
import { LeaveEvidencePanel } from "@/components/self-service/leave-evidence-panel";
import { ApiError } from "@/server/http";

function dateLabel(value: string | null | undefined) {
  if (!value) return "—";
  return new Intl.DateTimeFormat(undefined, {
    weekday: "short",
    day: "numeric",
    month: "short",
    year: "numeric",
    timeZone: "UTC",
  }).format(new Date(value + "T12:00:00Z"));
}
function statusLabel(state: string, cancellationState?: string | null) {
  if (cancellationState === "pending_review") return "Cancellation in review";
  return state.replaceAll("_", " ");
}
function ApprovalHistoryDisclosure({ approval, title }: { approval: ApprovalView; title: string }) {
  return <ApprovalHistory approval={approval} title={title}/>;
}
function nextAction(request: Awaited<ReturnType<typeof getLeave>>) {
  if (request.permittedActions.includes("save"))
    return "Review your dates, calculation, and any required evidence before submitting.";
  if (request.permittedActions.some((action) => ["approve", "reject", "approve_cancellation", "reject_cancellation"].includes(action)))
    return "This request is assigned to you for review. Record the decision and add a reason when required.";
  if (request.permittedActions.includes("cancel"))
    return request.state === "approved"
      ? "Your approved leave remains in place. You can request a cancellation review if plans change."
      : "You can withdraw this request while it is still in review.";
  if (request.cancellation_state === "pending_review")
    return "The cancellation is waiting for the required independent reviews. The approved absence remains effective.";
  if (request.state === "pending_approval") return "Your request is waiting for its current approver.";
  if (request.state === "approved") return "Your leave is approved.";
  if (request.state === "rejected") return "This request was rejected. Review the decision reason or prepare a new request.";
  if (request.state === "cancelled" || request.state === "withdrawn") return "This request is closed. Its decision history is retained below.";
  return "Review the request details and approval history below.";
}

export default async function LeaveTaskPage({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const base = workspaceBase(publicScope(actor));
  const request = await getLeave(actor.holdingId, id, actor).catch((error) => pageError(error, holdingSlug));
  const evidence = await getLeaveEvidence(actor.holdingId, id, actor).catch((error) => {
    if (error instanceof ApiError && error.status === 403) return null;
    throw error;
  });
  const editable = request.permittedActions.includes("save");
  const summary = editable ? await leaveSummary(actor) : null;
  const approval = request.cancellationApproval ?? request.approval;
  const evidencePolicy = parseLeaveEvidencePolicy(request.evidence_policy_snapshot);
  const attachmentPlatform = attachmentsV2Readable();
  const attachmentReadOnly = attachmentsV2ReadOnly();
  return (
    <div className={"content " + styles.leavePage}>
      <div className={styles.detailFooter}><Link href={base + "/leave"}>← My leave</Link></div>
      <header className={styles.detailHeader}>
        <div>
          <p className={styles.eyebrow}>Personal leave request · {request.number}</p>
          <h1>{request.leave_type_name ?? "Leave request"}</h1>
          <p>{dateLabel(request.start_date)}{request.end_date && request.end_date !== request.start_date ? " – " + dateLabel(request.end_date) : ""} · {request.calculated_units ?? request.days} working days</p>
        </div>
        <span className={styles.detailStatus}>{statusLabel(request.state, request.cancellation_state)}</span>
      </header>

      <OperationSummary operation={{status:request.state,responsible:approval.currentApprover?.approver_name??"Requester",currentStep:approval.currentApprover?`${approval.currentApprover.stage?.replaceAll("_"," ")??"Approval"} · step ${approval.currentApprover.ordinal+1}`:null,nextAction:nextAction(request),blockedReason:approval.blockedReason,dueAt:null,recordHref:null,actionKind:request.permittedActions.some(action=>["approve","reject","approve_cancellation","reject_cancellation"].includes(action))?"approval":"work"}}/>

      {request.cancellation_state === "pending_review" && (
        <p className={styles.detailMessage}><strong>Approved leave remains in effect.</strong> The cancellation takes effect only after the required manager and independent HR reviews.</p>
      )}
      {request.approval.blockedReason && editable && (
        <p className={styles.detailMessage}>{request.approval.blockedReason} Ask HR to resolve the approval route before submitting.</p>
      )}
      {request.decision_note && (
        <p className={styles.detailMessage}><strong>Decision reason:</strong> {request.decision_note}</p>
      )}

      <section className={styles.detailPanel} aria-labelledby="leave-facts-title">
        <h2 id="leave-facts-title">Request details</h2>
        <dl className={styles.factsGrid}>
          <div><dt>Request number</dt><dd>{request.number}</dd></div>
          <div><dt>Status</dt><dd>{statusLabel(request.state, request.cancellation_state)}</dd></div>
          <div><dt>First day and portion</dt><dd>{dateLabel(request.start_date)} · {(request.start_segment ?? "full").replaceAll("_", " ")}</dd></div>
          <div><dt>Last day and portion</dt><dd>{dateLabel(request.end_date ?? request.start_date)} · {(request.end_segment ?? "full").replaceAll("_", " ")}</dd></div>
          <div><dt>Calculated working days</dt><dd>{request.calculated_units ?? request.days}</dd></div>
          <div><dt>Updated</dt><dd>{new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(new Date(request.updated_at))}</dd></div>
        </dl>
        <p className={styles.notes}>{request.handover_note || "No handover note added."}</p>
      </section>

      {editable && summary && (
        <LeaveEditor
          key={id + ":" + request.version}
          base={base}
          record={request}
          types={summary.balances}
          evidencePresent={Boolean(evidence?.some((file) => file.source_state === "available"))}
          externalEvidence={attachmentPlatform}
        />
      )}

      {evidence && <LeaveEvidencePanel
        base={base}
        requestId={request.id}
        requestVersion={request.version}
        evidencePolicy={evidencePolicy}
        initialFiles={evidence}
        canWrite={!attachmentReadOnly && request.created_by === actor.userId && ["draft", "rejected", "pending_approval"].includes(request.state)}
        enabled={attachmentPlatform}
      />}
      {!evidence && <p className={styles.detailMessage}>Private leave evidence is hidden because you are not the current reviewer, a recorded decision maker, the requester, or authorized HR.</p>}

      <LeaveDecisions key={"decisions:" + id + ":" + request.version} base={base} record={request} />

      <ApprovalHistoryDisclosure approval={approval} title={request.cancellationApproval ? "Cancellation approval history" : "Approval history"} />
      {request.cancellationApproval && (
        <ApprovalHistoryDisclosure approval={request.approval} title="Original approval history" />
      )}

    </div>
  );
}
