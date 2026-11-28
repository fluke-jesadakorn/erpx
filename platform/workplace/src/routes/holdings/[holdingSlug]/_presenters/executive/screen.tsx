import { runtimePresentationQueries } from "@erpx/platform-runtime-server/public/presentation-queries";
import { requireWorkContext, publicScope } from "@erpx/identity-access-server/public/authorization";


import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { ExecutiveLibrary } from "@erpx/platform-runtime-ui/components/executive/library";
import { pageError } from "@erpx/work-projection-server/public/page-error";
export default async function ExecutivePage({params}:{params:Promise<{holdingSlug:string}>}) {
  const {holdingSlug}=await params;
  let actor, initial;
  try { actor=await requireWorkContext(holdingSlug);const presentation=await runtimePresentationQueries(actor.holdingId);initial=await presentation.reportLibrary(actor); } catch(e) { return pageError(e,holdingSlug); }
  return <ExecutiveLibrary key={`${actor.companyId}:${actor.assignmentId}`} base={workspaceBase(publicScope(actor))} api={`/api/v1/holdings/${holdingSlug}/executive`} initial={initial}/>;
}
