
import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { SetupChecklist } from "@/components/ui/setup-checklist";
import { RecordLink as Link } from "@/components/records/record-link";
import { notFound } from "next/navigation";
import { approvalView, type ApprovalKind } from "@/server/approvals";
import { holdingResources } from "@/server/runtime";
import type { WorkContext } from "@/server/authorization";
import { staffSettings } from "@/server/staff-settings";
import { getLeave } from "@/server/leave";
import { getExpense } from "@/server/expenses";
import { getTask } from "@/server/staff-tasks";
import { ApiError } from "@/server/http";
import { approvalAdminHref } from "@/lib/administration";
import { RerouteControl } from "./staff-work-controls";
import { Card } from "@/components/ui/foundation";

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
  const { write } = await holdingResources(actor.holdingId);
  const options = await staffSettings(write, actor);
  if (!options.canManage) notFound();
  if (actor.beta && kind && kind !== "expense") notFound();
  if (record && !["leave", "expense", "task"].includes(kind ?? "")) notFound();
  const queue = await write
    .prepare(
      "SELECT record_type,record_id FROM approval_instances WHERE company_id=? AND state IN ('pending','blocked') AND (?::text IS NULL OR (record_type=? AND record_id=?)) ORDER BY created_at LIMIT 50",
    )
    .bind(actor.companyId, record ?? null, kind ?? null, record ?? null)
    .all<{ record_type: ApprovalKind; record_id: string }>();
  const rows = [];
  for (const row of record
    ? [{ record_type: kind as ApprovalKind, record_id: record }]
    : queue.results.filter(
        (row) => !actor.beta || row.record_type === "expense",
      )) {
    try {
      // Resolve the business record first: administration does not grant record access.
      const current =
        row.record_type === "leave"
          ? await getLeave(actor.holdingId, row.record_id, actor)
          : row.record_type === "expense"
            ? await getExpense(actor, row.record_id)
            : await getTask(write, actor, row.record_id);
      const approval = await approvalView(
        write,
        row.record_type,
        row.record_id,
        actor,
      );
      const destination =
        row.record_type === "leave"
          ? `tasks/people_payroll/${row.record_id}`
          : row.record_type === "expense"
            ? `expenses/${row.record_id}`
            : `staff-tasks/${row.record_id}`;
      rows.push({
        ...row,
        approval,
        version: Number(current.version),
        destination,
      });
    } catch (error) {
      if (!(error instanceof ApiError) || ![403, 404].includes(error.status))
        throw error;
    }
  }
  if (record && !rows.length)
    return (
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Approval issue unavailable</h2>
        <p>This item may have moved on or may no longer be accessible.</p>
        <Link href={`${base}/settings?section=approvals`}>
          Back to approval issues
        </Link>
      </Card>
    );
  return (
    <>
      <div className="ui-form-actions">
        {!actor.beta && (
          <UiActionLink appearance="standard" variant="secondary" href={`${base}/routine-templates`}>
            Routine templates
          </UiActionLink>
        )}
        <UiActionLink appearance="standard" variant="secondary"

          href={`${base}/settings?section=organization`}
        >
          Organization settings
        </UiActionLink>
      </div>
      <SetupChecklist title="Approval readiness" items={rows.some(row=>row.approval.blockedReason) ? rows.filter(row=>row.approval.blockedReason).map(row=>({id:`${row.record_type}:${row.record_id}`,label:`${row.record_type} approval routing`,ready:false,owner:"Organization administrator",href:approvalAdminHref(base,row.record_type,row.record_id),detail:row.approval.blockedReason ?? "Assign an eligible independent approver"})) : [{id:"routing",label:"Approval routing",ready:true,owner:"Organization administrator",href:`${base}/people/organization`,detail:"No blocked approval routes in your visible queue"}]} />
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
            <Link href={`${base}/${row.destination}`}>
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
