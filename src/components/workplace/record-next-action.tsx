import type { FlowView } from "@/lib/flow";
import { StatusBadge } from "@/components/ui/primitives";
/** A read model only; domain commands still enforce current authorization. */
export function RecordNextAction({flow}: {flow: FlowView}) {
  return <section className="card record-next-action" aria-label="Status, owner and next action">
    <div><span className="eyebrow">Status</span><StatusBadge status={flow.state}/></div>
    <div><span className="eyebrow">Responsible person</span><strong>{flow.current?.primary?.name ?? flow.current?.teamName ?? (flow.current ? "Assignment required" : "No owner needed")}</strong></div>
    <div><span className="eyebrow">Next action</span><strong>{flow.current?.action ?? "No further action"}</strong><p className="muted">{flow.blocker ?? (flow.canAct ? "Ready for you to continue below." : flow.current ? `Waiting for ${flow.current.primary?.name ?? "an eligible owner"}.` : "The recorded history is available below.")}</p>{flow.blocker && flow.canManage && <a className="text-link" href="#workflow-owners">Review ownership and resolve blockers →</a>}</div>
  </section>;
}
