import { InlineNotice } from "@erpx/ui/components/foundation";
import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import type { ApprovalView } from "@erpx/server-workflow/public/approvals";
import { Card } from "@erpx/ui/components/foundation";
function source(snapshotText: string | null | undefined) {
  if (!snapshotText) return null;
  try {
    const snapshot = JSON.parse(snapshotText) as {departmentPolicyVersion?:number;departmentPolicyId?:string|null;selector?:{kind?:string};rerouted?:boolean;appointmentKind?:string;appointedPersonId?:string|null};
    if (snapshot.departmentPolicyVersion !== undefined) return `${snapshot.departmentPolicyId ? `Department flow v${snapshot.departmentPolicyVersion}` : "Appointed department head"} · ${snapshot.selector?.kind === "position" ? "position" : "person"}${snapshot.rerouted ? " · approver changed" : ""}`;
    if (snapshot.appointmentKind === "hr") return snapshot.appointedPersonId ? "Appointed HR reviewer" : "HR role";
  } catch { return null; }
  return null;
}
export function ApprovalHistory({
  approval,
  blockedReason,
  title = "Approval",
}: {
  approval: ApprovalView;
  blockedReason?: string | null;
  title?: string;
}) {
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>{title}</h2>
      {(blockedReason || approval.blockedReason) && (
        <InlineNotice tone="info" role="status" className="notice">
          Blocked:{" "}
          {blockedReason || approval.blockedReason}
        </InlineNotice>
      )}
      {approval.currentApprover && (
        <p>
          <strong>
            Current approver: {approval.currentApprover.approver_name}
          </strong>{" "}
          ·{" "}
          {approval.currentApprover.authority === "hr_admin"
            ? "HR review"
            : approval.currentApprover.stage === "department_approval"
              ? "Department review"
              : "Named review"}
        </p>
      )}
      <details>
        <summary>Approval route and history</summary>
        {approval.instance && (
          <p>
            Saved route at submission · cycle {approval.instance.cycle} · {approval.instance.phase} ·{" "}
            {approval.instance.state}
          </p>
        )}
        {approval.steps.length > 0 && (
          <ol>
            {approval.steps.map((step) => (
              <li key={step.id}>
                {step.approver_name} ·{" "}
                {step.authority === "hr_admin" ? "HR" : step.stage === "department_approval" ? "Department approver" : "Named approver"} ·{" "}
                {step.state}
                {approval.currentApprover?.id === step.id
                  ? " · current step"
                  : ""}
                {source(step.route_snapshot_json) && <small className="muted"> · {source(step.route_snapshot_json)}</small>}
              </li>
            ))}
          </ol>
        )}
        {!approval.instance && (
          <p className="muted">The route is captured when submitted.</p>
        )}
        {approval.history.map((d) => (
          <article className="ui-record-item" key={d.id}>
            <div className="row-main">
              <strong>
                {d.action === "override" ? "Decision on behalf of assigned approver" : d.action === "reroute" ? "Approver changed" : d.action} · {d.actor_name}
              </strong>
              <p>{d.reason || "No additional note"}</p>
              <small>
                Reviewed version {d.reviewed_version} ·{" "}
                <TimeText appearance="standard" value={d.created_at} />
              </small>
              {(d.action === "reroute" || d.action === "override") && (
                <p>
                  {d.previous_approver_name ?? d.previous_approver_user_id ?? "Previous approver"} → {d.action === "override" ? d.actor_name : d.approver_name ?? d.approver_user_id ?? "New approver"}
                </p>
              )}
            </div>
          </article>
        ))}
      </details>
    </Card>
  );
}
