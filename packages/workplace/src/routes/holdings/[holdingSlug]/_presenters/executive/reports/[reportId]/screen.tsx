import { runtimePresentationQueries } from "@erpx/server-runtime/public/presentation-queries";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";



import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { ExecutiveCanvas } from "@erpx/feature-runtime/components/executive/canvas";
import { pageError } from "@erpx/server-work_projection/public/page-error";
export default async function ReportPage({params}:{params:Promise<{holdingSlug:string;reportId:string}>}) {
  const {holdingSlug,reportId}=await params;
  let actor, initial, catalog;
  try { actor=await requireWorkContext(holdingSlug);const presentation=await runtimePresentationQueries(actor.holdingId);initial=await presentation.reportView(actor, reportId);catalog=await presentation.metricCatalogFor(actor); } catch(e) { return pageError(e,holdingSlug); }
  return <ExecutiveCanvas key={`${actor.companyId}:${actor.assignmentId}:${reportId}`} base={workspaceBase(publicScope(actor))} api={`/api/v1/holdings/${holdingSlug}/executive/${reportId}`} initial={initial} catalog={catalog}/>;
}
