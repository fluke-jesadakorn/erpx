import type { ApprovalView } from "@/server/approvals";
export function ApprovalHistory({
  approval,
  blockedReason,
}: {
  approval: ApprovalView;
  blockedReason?: string | null;
}) {
  return (
    <section className="card">
      <h2>Approval</h2>
      {(blockedReason || approval.blockedReason) && (
        <p role="status" className="notice">
          Submission or decision blocked:{" "}
          {blockedReason || approval.blockedReason}
        </p>
      )}
      {approval.currentApprover && (
        <p>
          <strong>
            Current approver: {approval.currentApprover.approver_name}
          </strong>{" "}
          ·{" "}
          {approval.currentApprover.authority === "hr_admin"
            ? "HR review"
            : "Named review"}
        </p>
      )}
      <details>
        <summary>Approval route and history</summary>
        {approval.instance && (
          <p>
            Cycle {approval.instance.cycle} · {approval.instance.phase} ·{" "}
            {approval.instance.state}
          </p>
        )}
        {approval.steps.length > 0 && (
          <ol>
            {approval.steps.map((step) => (
              <li key={step.id}>
                {step.approver_name} ·{" "}
                {step.authority === "hr_admin" ? "HR" : "Named approver"} ·{" "}
                {step.state}
                {approval.currentApprover?.id === step.id
                  ? " · current step"
                  : ""}
              </li>
            ))}
          </ol>
        )}
        {!approval.instance && (
          <p className="muted">The route is captured when submitted.</p>
        )}
        {approval.history.map((d) => (
          <article className="row" key={d.id}>
            <div className="row-main">
              <strong>
                {d.action} · {d.actor_name}
              </strong>
              <p>{d.reason || "No additional note"}</p>
              <small>
                Reviewed version {d.reviewed_version} ·{" "}
                {new Date(d.created_at)
                  .toISOString()
                  .replace("T", " ")
                  .slice(0, 19)}{" "}
                UTC
              </small>
              {d.action === "reroute" && (
                <p>
                  Previous: {d.previous_approver_user_id} → {d.approver_user_id}
                </p>
              )}
            </div>
          </article>
        ))}
      </details>
    </section>
  );
}
