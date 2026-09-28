import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { ClearanceWorkspace } from "@/components/people/clearance-workspace";
import { PageHeader } from "@/components/ui/primitives";
import { WorkReturn } from "@/components/workplace/work-return";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { clearanceCase,clearanceOwners } from "@/server/retirement-clearance";
import { holdingResources } from "@/server/runtime";
import { pageError } from "@/server/page-error";
import Link from "next/link";
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}) {
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor)),{write}=await holdingResources(actor.holdingId);
 const data=await clearanceCase(write,actor,id).catch(error=>pageError(error,holdingSlug));
 const owners=await clearanceOwners(write,actor).catch(error=>pageError(error,holdingSlug));
 return <FeaturePage className="content-stack"><WorkReturn base={base}/><Link href={`${base}/people?case=${id}`}>← Employment case</Link><PageHeader title={`${data.case.name} · Retirement clearance`} description="Track all departments in one case. Source approvals, posting and payments remain in their original workflows."/><FeatureTabs/><WidgetGrid pageKey="people-clearance-detail"><FeatureWidget id="people-clearance-detail-clearance" title="Clearance"><ClearanceWorkspace base={base} data={data} owners={owners}/></FeatureWidget></WidgetGrid></FeaturePage>;
}
