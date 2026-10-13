import type { FlowKind, FlowView } from "@/lib/shared/flow";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { documentFlow } from "@/server/modules/workflow/public/document-flow";
export { documentFlow } from "@/server/modules/workflow/public/document-flow";
import { FlowTimeline } from "./flow-timeline";

export async function FlowPanel({actor,kind,id,base,flow:provided}:{actor:WorkContext;kind:FlowKind;id:string;base:string;flow?:FlowView}) {
  const flow = provided ?? await documentFlow(actor,kind,id);
  return <><p style={{fontSize:13,color:"var(--ui-muted)"}}>{flow.canAct ? `${flow.current?.action.toLowerCase().includes("approve") ? "Approve" : "Act"} as ${actor.assignments[0]?.label ?? actor.role.replaceAll("_"," ")}` : `Active role: ${actor.assignments[0]?.label ?? actor.role.replaceAll("_"," ")}`} {!flow.canAct && flow.current?.primary?.userId===actor.userId && <a href={`/work-role?from=${encodeURIComponent(`${base}/home`)}`}>Choose a role for this step</a>}</p><FlowTimeline key={`${flow.recordVersion}:${flow.version}`} flow={flow} endpoint={`${base.replace(/^\/holdings\//,"/api/v1/holdings/")}/flows/${kind}/${encodeURIComponent(id)}`} /></>;
}
