import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";

import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";
import { SetupChecklist } from "@erpx/ui/components/setup-checklist";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { notFound } from "next/navigation";
import { approvalAdministrationData } from "@erpx/server-workflow/public/approval-administration";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { approvalAdminHref } from "@erpx/feature-shared/lib/administration";
import { RerouteControl } from "@erpx/feature-work_projection/components/staff-work-controls";
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
    </>
  );
}
