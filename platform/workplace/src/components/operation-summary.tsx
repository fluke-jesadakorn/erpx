import { InlineNotice } from "@erpx/platform-ui/components/foundation";
import Link from "@erpx/platform-workplace/components/workspace-link";
import { StatusBadge } from "@erpx/platform-ui/components/primitives";
import type { OperationPresentation } from "@erpx/platform-shared-ui/lib/operation-presentation";
import { TimeText } from "@erpx/people-payroll-ui/components/time/time-preference-context";
import { Card } from "@erpx/platform-ui/components/foundation";

export function OperationSummary({ operation, children }: { operation: OperationPresentation; children?: React.ReactNode }) {
  return <Card as="section" appearance="standard" className="ui-panel record-next-action" aria-label="Status, owner and next action">
    <div className="record-next-action-primary"><span className="eyebrow">Next action</span><strong>{operation.nextAction}</strong>
      {operation.blockedReason ? <InlineNotice tone="info" role="status" className="notice">Blocked: {operation.blockedReason}</InlineNotice> : children}
      {operation.dueAt && <p className="muted">Due <TimeText appearance="standard" value={operation.dueAt} /></p>}
      {operation.recordHref && <Link className="text-link" href={operation.recordHref}>Open original record →</Link>}
    </div>
    <div><span className="eyebrow">Status</span><StatusBadge appearance="standard" status={operation.status}/></div>
    <div><span className="eyebrow">Responsible person</span><strong>{operation.responsible}</strong></div>
    <div><span className="eyebrow">Current step</span><strong>{operation.currentStep ?? "No active step"}</strong></div>
  </Card>;
}
