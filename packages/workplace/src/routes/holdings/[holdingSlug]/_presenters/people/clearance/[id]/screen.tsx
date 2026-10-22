import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { peoplePayrollPresentationQueries } from "@erpx/server-people_payroll/public/presentation-queries";
import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@erpx/feature-shared/components/features/feature-page";
import { ClearanceWorkspace } from "@erpx/feature-records/components/loaders/people--clearance-workspace";
import { PageHeader } from "@erpx/ui/components/primitives";
import { WorkReturn } from "@erpx/feature-records/components/loaders/workplace--work-return";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope,requireWorkContext } from "@erpx/server-identity_access/public/authorization";


import { pageError } from "@erpx/server-work_projection/public/page-error";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}) {
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor)),presentation=await peoplePayrollPresentationQueries(actor.holdingId);
 const data=await presentation.clearanceCase(actor, id).catch(error=>pageError(error,holdingSlug));
 const owners=await presentation.clearanceOwners(actor).catch(error=>pageError(error,holdingSlug));
 return <FeaturePage className="ui-page-content"><WorkReturn base={base}/><Link href={canonicalUiHref(`${base}/people?case=${id}`)}>← Employment case</Link><PageHeader appearance="standard" title={`${data.case.name} · Retirement clearance`} description="Track all departments in one case. Source approvals, posting and payments remain in their original workflows."/><FeatureTabs/><WidgetGrid pageKey="people-clearance-detail"><FeatureWidget appearance="standard" id="people-clearance-detail-clearance" title="Clearance"><ClearanceWorkspace base={base} data={data} owners={owners}/></FeatureWidget></WidgetGrid></FeaturePage>;
}
