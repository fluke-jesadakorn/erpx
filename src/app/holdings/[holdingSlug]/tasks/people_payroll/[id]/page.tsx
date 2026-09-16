import { pageError } from "@/server/page-error";
import { FlowPanel, documentFlow } from "@/components/workplace/flow-panel";
import { ApprovalHistory } from "@/components/modules/approval-history";
import { ApprovalAdminLink } from "@/components/modules/approval-admin-link";
import { staffSettings } from "@/server/staff-settings";
import { holdingResources } from "@/server/runtime";
import Link from "next/link";
import { LeaveControls } from "@/components/modules/leave-controls";
import { ArrowLeft } from "lucide-react";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { getLeave } from "@/server/leave";
import { getLeaveEvidence } from "@/server/work";
import { StatusBadge } from "@/components/ui/primitives";
import { LeaveTaskForm } from "./task-form";
import styles from "@/components/workplace/pages.module.css";
export default async function LeaveTaskPage({
  params,
  searchParams,
}: PageProps<"/holdings/[holdingSlug]/tasks/people_payroll/[id]">) {
  const { holdingSlug, id } = await params;
  const search = await searchParams;
  const context = await requireWorkContext(holdingSlug);
  const base = workspaceBase(publicScope(context));
  const request = await getLeave(context.holdingId, id, context).catch(
    (error) => {
      return pageError(error, holdingSlug);
    },
  );
  const { write } = await holdingResources(context.holdingId);
  const options = await staffSettings(write, context);
  const evidence = await getLeaveEvidence(context.holdingId, id, context);
  const back =
    search.from === "home"
      ? `${base}/home`
      : `${base}/work${typeof search.return === "string" ? `?${new URLSearchParams(search.return).toString()}` : ""}`;
  const flow = await documentFlow(context,"leave",id);
  return (
    <main className="content" data-source="live">
      <Link href={back} className="back-link">
        <ArrowLeft size={14} />
        Back to {search.from === "home" ? "home" : "work"}
      </Link>
      <header className={styles.recordHeader}>
        <div>
          <div className="eyebrow">People & payroll / Annual leave</div>
          <h1>{request.number}</h1>
          <p className="lede">A clear handover makes time away easier.</p>
        </div>
        <StatusBadge status={request.state} />
      </header>
      <div className={styles.recordGrid}>
        <div>
          <section className="card">
            <h2 style={{ marginBottom: 24 }}>Request details</h2>
            <dl className={styles.facts}>
              <div>
                <dt>First day</dt>
                <dd>{request.start_date}</dd>
              </div>
              <div>
                <dt>Duration</dt>
                <dd>{request.days} days</dd>
              </div>
              <div>
                <dt>Status</dt>
                <dd>{request.state.replaceAll("_", " ")}</dd>
              </div>
            </dl>
            <div className={styles.detailNote}>
              <h3>Handover plan</h3>
              <p>
                {request.handover_note || "No handover note has been added."}
              </p>
            </div>
            <div className={styles.detailNote}>
              <h3>Supporting evidence</h3>
              {evidence.length
                ? evidence.map((file) => (
                    <a
                      className="row"
                      key={file.id}
                      href={`/api/v1${base}/attachments/${file.id}/content`}
                    >
                      <span className="row-main">{file.original_name}</span>
                      <span className="small">
                        {Math.max(1, Math.round(file.bytes / 1024))} KB ·
                        Download
                      </span>
                    </a>
                  ))
                : "No supporting files attached."}
            </div>
            {request.decision_note && (
              <div className={styles.detailNote}>
                <h3>Decision note</h3>
                <p>{request.decision_note}</p>
              </div>
            )}
          </section>
          <ApprovalHistory approval={request.approval} />
          {options.canManage &&
            (request.state === "pending_approval" ||
              !!request.approval.blockedReason) && (
              <ApprovalAdminLink base={base} kind="leave" id={request.id} />
            )}
          <LeaveTaskForm
            key={request.id}
            workspaceBase={base}
            request={request}
            canApprove={flow.canAct&&request.permittedActions.includes("approve")}
            canSubmit={request.created_by === context.userId}
            blockedReason={request.approval.blockedReason}
          />
          {(request.permittedActions.includes("cancel") ||
            request.permittedActions.includes("reject")) && (
            <LeaveControls
              base={base}
              recordId={id}
              version={request.version}
              canCancel={request.permittedActions.includes("cancel")}
              canReject={flow.canAct&&request.permittedActions.includes("reject")}
            />
          )}
        </div>
        <aside className={styles.recordAside}>
          <div className="eyebrow">How it moves forward</div>
          <h2 style={{ marginTop: 8 }}>A simple handoff</h2>
          <ol>
            <li>
              <strong>Prepare</strong>Dates, handover, and evidence.
            </li>
            <li>
              <strong>Department review</strong>Your named department approver
              reviews your request.
            </li>
            <li>
              <strong>HR review</strong>An independent HR administrator makes
              the final decision.
            </li>
          </ol>
          <p className="small" style={{ margin: "24px 0 0" }}>
            Record version {request.version}
          </p>
        </aside>
      </div>
    <FlowPanel flow={flow} actor={context} kind="leave" id={id} base={base}/></main>
  );
}
