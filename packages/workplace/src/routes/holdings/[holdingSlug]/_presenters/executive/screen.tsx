import { runtimePresentationQueries } from "@erpx/server-runtime/public/presentation-queries";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";


import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { ExecutiveLibrary } from "@erpx/feature-runtime/components/executive/library";
import { pageError } from "@erpx/server-work_projection/public/page-error";
export default async function ExecutivePage({params}:{params:Promise<{holdingSlug:string}>}) {
  const {holdingSlug}=await params;
  let actor, initial;
  try { actor=await requireWorkContext(holdingSlug);const presentation=await runtimePresentationQueries(actor.holdingId);initial=await presentation.reportLibrary(actor); } catch(e) { return pageError(e,holdingSlug); }
  return <ExecutiveLibrary key={`${actor.companyId}:${actor.assignmentId}`} base={workspaceBase(publicScope(actor))} api={`/api/v1/holdings/${holdingSlug}/executive`} initial={initial}/>;
}
