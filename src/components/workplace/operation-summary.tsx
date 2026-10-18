import { InlineNotice } from "@/components/ui/foundation";
import Link from "@/components/workplace/workspace-link";
import { StatusBadge } from "@/components/ui/primitives";
import type { OperationPresentation } from "@/lib/shared/operation-presentation";
import { TimeText } from "@/components/people_payroll/time/time-preference-context";
import { Card } from "@/components/ui/foundation";

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
