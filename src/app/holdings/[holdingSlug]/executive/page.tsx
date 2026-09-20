import { requireWorkContext, publicScope } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { reportLibrary } from "@/server/executive/reports";
import { workspaceBase } from "@/lib/workspace";
import { ExecutiveLibrary } from "@/components/executive/library";
import { pageError } from "@/server/page-error";
export default async function ExecutivePage({params}:{params:Promise<{holdingSlug:string}>}) {
  const {holdingSlug}=await params;
  let actor, initial;
  try { actor=await requireWorkContext(holdingSlug);const {write}=await holdingResources(actor.holdingId);initial=await reportLibrary(write,actor); } catch(e) { return pageError(e,holdingSlug); }
  return <ExecutiveLibrary key={`${actor.companyId}:${actor.assignmentId}`} base={workspaceBase(publicScope(actor))} api={`/api/v1/holdings/${holdingSlug}/executive`} initial={initial}/>;
}
