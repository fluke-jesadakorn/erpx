import type { FlowView } from "@erpx/platform-shared-ui/lib/flow";
import { operationFromFlow } from "@erpx/platform-shared-ui/lib/operation-presentation";
import { OperationSummary } from "@erpx/platform-workplace/components/operation-summary";
/** A read model only; domain commands still enforce current authorization. */
export function RecordNextAction({flow}: {flow: FlowView}) {
  return <OperationSummary operation={operationFromFlow(flow)}><p className="muted">{flow.canAct ? "Ready for you to continue below." : flow.current ? `Waiting for ${flow.current.primary?.name ?? "an eligible owner"}.` : "The recorded history is available below."}</p>{flow.blocker && flow.canManage && <a className="text-link" href="#workflow-owners">Review ownership and resolve blockers →</a>}</OperationSummary>;
}
