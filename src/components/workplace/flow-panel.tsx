import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import type { FlowKind, FlowView } from "@/lib/shared/flow";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { documentFlow } from "@/server/modules/workflow/public/document-flow";
export { documentFlow } from "@/server/modules/workflow/public/document-flow";
import { FlowTimeline } from "./flow-timeline";
import { headers } from "next/headers";
import { configuredProductHandoff, productHostForHost } from "@/lib/runtime/product-hosts";
import Link from "@/components/workplace/workspace-link";

export async function FlowPanel({actor,kind,id,base,flow:provided}:{actor:WorkContext;kind:FlowKind;id:string;base:string;flow?:FlowView}) {
  const flow = provided ?? await documentFlow(actor,kind,id);
  const roleLabel=actor.assignments.find(assignment=>assignment.id===actor.assignmentId)?.label??actor.role.replaceAll("_"," ");
  const needsRole=!flow.canAct&&flow.current?.primary?.userId===actor.userId;
  let roleHref=`/work-role?from=${encodeURIComponent(canonicalUiHref(`${base}/home`))}`,productRole=false;
  if(needsRole){
    const requestHeaders=await headers(),audience=productHostForHost(requestHeaders.get("host"));
    if(audience){
      // Resolve the request's exact current document through the same registry
      // as Apps. A navigation hint never selects or grants an acting role.
      const {productWorkspaceTarget,productWorkRolePath}=await import("@/server/modules/identity_access/public/product-workspace");
      const target=productWorkspaceTarget(audience,actor.companyId,requestHeaders.get("x-erpx-page-path"));
      roleHref=target?productWorkRolePath(target):configuredProductHandoff("apps")!;
      productRole=true;
    }
  }
  return <><p style={{fontSize:13,color:"var(--ui-muted)"}}>{flow.canAct ? `${flow.current?.action.toLowerCase().includes("approve") ? "Approve" : "Act"} as ${roleLabel}` : `Active role: ${roleLabel}`} {needsRole && <Link href={roleHref} data-product-switch={productRole||undefined}>Choose a role for this step</Link>}</p><FlowTimeline key={`${flow.recordVersion}:${flow.version}`} flow={flow} endpoint={`${base.replace(/^\/holdings\//,"/api/v1/holdings/")}/flows/${kind}/${encodeURIComponent(id)}`} /></>;
}
