import { runtimePresentationQueries } from "@erpx/platform-runtime-server/public/presentation-queries";
import { requireWorkContext, publicScope } from "@erpx/identity-access-server/public/authorization";



import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { ExecutiveCanvas } from "@erpx/platform-runtime-ui/components/executive/canvas";
import { pageError } from "@erpx/work-projection-server/public/page-error";
export default async function ReportPage({params}:{params:Promise<{holdingSlug:string;reportId:string}>}) {
  const {holdingSlug,reportId}=await params;
  let actor, initial, catalog;
  try { actor=await requireWorkContext(holdingSlug);const presentation=await runtimePresentationQueries(actor.holdingId);initial=await presentation.reportView(actor, reportId);catalog=await presentation.metricCatalogFor(actor); } catch(e) { return pageError(e,holdingSlug); }
  return <ExecutiveCanvas key={`${actor.companyId}:${actor.assignmentId}:${reportId}`} base={workspaceBase(publicScope(actor))} api={`/api/v1/holdings/${holdingSlug}/executive/${reportId}`} initial={initial} catalog={catalog}/>;
}
