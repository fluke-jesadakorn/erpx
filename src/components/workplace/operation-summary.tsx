import Link from "next/link";
import { StatusBadge } from "@/components/ui/primitives";
import type { OperationPresentation } from "@/lib/operation-presentation";

export function OperationSummary({ operation, children }: { operation: OperationPresentation; children?: React.ReactNode }) {
  return <section className="card record-next-action" aria-label="Status, owner and next action">
    <div className="record-next-action-primary"><span className="eyebrow">Next action</span><strong>{operation.nextAction}</strong>
      {operation.blockedReason ? <p role="status" className="notice">Blocked: {operation.blockedReason}</p> : children}
      {operation.dueAt && <p className="muted">Due {new Date(operation.dueAt).toLocaleString("en-GB", {timeZone:"Asia/Bangkok"})}</p>}
      {operation.recordHref && <Link className="text-link" href={operation.recordHref}>Open original record →</Link>}
    </div>
    <div><span className="eyebrow">Status</span><StatusBadge status={operation.status}/></div>
    <div><span className="eyebrow">Responsible person</span><strong>{operation.responsible}</strong></div>
    <div><span className="eyebrow">Current step</span><strong>{operation.currentStep ?? "No active step"}</strong></div>
  </section>;
}
