import { requireWorkContext, publicScope } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { reportView } from "@/server/executive/reports";
import { metricCatalogFor } from "@/server/executive/metrics";
import { workspaceBase } from "@/lib/workspace";
import { ExecutiveCanvas } from "@/components/executive/canvas";
import { pageError } from "@/server/page-error";
export default async function ReportPage({params}:{params:Promise<{holdingSlug:string;reportId:string}>}) {
  const {holdingSlug,reportId}=await params;
  let actor, initial, catalog;
  try { actor=await requireWorkContext(holdingSlug);const {write}=await holdingResources(actor.holdingId);initial=await reportView(write,actor,reportId);catalog=await metricCatalogFor(write,actor); } catch(e) { return pageError(e,holdingSlug); }
  return <ExecutiveCanvas key={`${actor.companyId}:${actor.assignmentId}:${reportId}`} base={workspaceBase(publicScope(actor))} api={`/api/v1/holdings/${holdingSlug}/executive/${reportId}`} initial={initial} catalog={catalog}/>;
}
