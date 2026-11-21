import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";

import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";
import { SetupChecklist } from "@erpx/ui/components/setup-checklist";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { notFound } from "next/navigation";
import { approvalAdministrationData } from "@erpx/server-workflow/public/approval-administration";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { approvalAdminHref } from "@erpx/feature-shared/lib/administration";
import {RerouteControl} from '@erpx/feature-work_projection/components/staff-work-controls';
import { Card } from "@erpx/ui/components/foundation";

export async function ApprovalAdministration({
  actor,
  base,
  kind,
  record,
}: {
  actor: WorkContext;
  base: string;
  kind?: string;
  record?: string;
}) {
  const data = await approvalAdministrationData(actor, kind, record);
  if (!data) notFound();
  const { options, rows } = data;
  const nativeEvidence = data.nativeEvidence?.approval ?? null;
  if (record && !rows.length)
    return (
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Approval issue unavailable</h2>
        <p>This item may have moved on or may no longer be accessible.</p>
        <Link href={canonicalUiHref(`${base}/settings?section=approvals`)}>
          Back to approval issues
        </Link>
      </Card>
    );
  return (
    <>
      <div className="ui-form-actions">
        {!actor.beta && (
          <UiActionLink appearance="standard" variant="secondary" href={canonicalUiHref(`${base}/routine-templates`)}>
            Routine templates
          </UiActionLink>
        )}
        <UiActionLink appearance="standard" variant="secondary"

          href={canonicalUiHref(`${base}/settings?section=organization`)}
        >
          Organization settings
        </UiActionLink>
      </div>
      <SetupChecklist title="Approval readiness" items={rows.some(row=>row.approval.blockedReason) ? rows.filter(row=>row.approval.blockedReason).map(row=>({id:`${row.record_type}:${row.record_id}`,label:`${row.record_type} approval routing`,ready:false,owner:"Organization administrator",href:approvalAdminHref(base,row.record_type,row.record_id),detail:row.approval.blockedReason ?? "Assign an eligible independent approver"})) : [{id:"routing",label:"Approval routing",ready:true,owner:"Organization administrator",href:canonicalUiHref(`${base}/people/organization`),detail:"No blocked approval routes in your visible queue"}]} />
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Approval issues</h2>
        <p className="muted">
          Resolve missing configuration or replace an unavailable approver.
          Decisions stay with the original work.
        </p>
        {!rows.length && <p>No open approval issues in your scope.</p>}
        {rows.map((row) => (
          <article
            className="approval-admin-item"
            key={`${row.record_type}:${row.record_id}`}
          >
            <Link href={canonicalUiHref(`${base}/${row.destination}`)}>
              {row.record_type === "task"
                ? "Task"
                : row.record_type === "leave"
                  ? "Leave request"
                  : "Expense claim"}{" "}
              · {row.record_id}
            </Link>
            <p>
              {row.approval.blockedReason ??
                (row.approval.currentApprover
                  ? `Waiting for ${row.approval.currentApprover.approver_name}`
                  : "No open approval step. Review the item and organization configuration.")}
            </p>
            {record ? (
              <RerouteControl appearance="standard"
                base={base}
                approval={row.approval}
                version={row.version}
                options={options}
              />
            ) : (
              <Link
                href={approvalAdminHref(base, row.record_type, row.record_id)}
              >
                Resolve approval issue →
              </Link>
            )}
          </article>
        ))}
      </Card>
      {nativeEvidence && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Native approval evidence</h2>
          <p className="muted">
            Owner-issued approval route, decisions and linked command evidence for{" "}
            {nativeEvidence.instance.record_id}.
          </p>
          <p>
            {nativeEvidence.instance.phase} · cycle {nativeEvidence.instance.cycle} ·{" "}
            {nativeEvidence.instance.state} · version {nativeEvidence.instance.version}
          </p>
          {nativeEvidence.steps.length > 0 && (
            <ol>
              {nativeEvidence.steps.map((step) => (
                <li key={step.id}>
                  {step.approver_user_id} · {step.authority} · {step.state}
                </li>
              ))}
            </ol>
          )}
          {nativeEvidence.history.map((decision) => (
            <article className="ui-record-item" key={decision.id}>
              <div className="row-main">
                <strong>
                  {decision.action} · {decision.actor_user_id}
                </strong>
                <p>{decision.reason || "No additional note"}</p>
                <small>Reviewed version {decision.reviewed_version}</small>
              </div>
            </article>
          ))}
          <p className="muted">
            {nativeEvidence.receipts.length} receipt(s),{" "}
            {nativeEvidence.audit.length} audit event(s),{" "}
            {nativeEvidence.outbox.length} outbox event(s).
          </p>
        </Card>
      )}
    </>
  );
}
