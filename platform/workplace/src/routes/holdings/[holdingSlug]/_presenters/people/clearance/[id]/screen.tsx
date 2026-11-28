import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { peoplePayrollPresentationQueries } from "@erpx/people-payroll-server/public/presentation-queries";
import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@erpx/platform-shared-ui/components/features/feature-page";
import { ClearanceWorkspace } from "@erpx/records-ui/components/loaders/people--clearance-workspace";
import { PageHeader } from "@erpx/platform-ui/components/primitives";
import { WorkReturn } from "@erpx/records-ui/components/loaders/workplace--work-return";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { publicScope,requireWorkContext } from "@erpx/identity-access-server/public/authorization";


import { pageError } from "@erpx/work-projection-server/public/page-error";
import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}) {
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor)),presentation=await peoplePayrollPresentationQueries(actor.holdingId);
 const data=await presentation.clearanceCase(actor, id).catch(error=>pageError(error,holdingSlug));
 const owners=await presentation.clearanceOwners(actor).catch(error=>pageError(error,holdingSlug));
 return <FeaturePage className="ui-page-content"><WorkReturn base={base}/><Link href={canonicalUiHref(`${base}/people?case=${id}`)}>← Employment case</Link><PageHeader appearance="standard" title={`${data.case.name} · Retirement clearance`} description="Track all departments in one case. Source approvals, posting and payments remain in their original workflows."/><FeatureTabs/><WidgetGrid pageKey="people-clearance-detail"><FeatureWidget appearance="standard" id="people-clearance-detail-clearance" title="Clearance"><ClearanceWorkspace base={base} data={data} owners={owners}/></FeatureWidget></WidgetGrid></FeaturePage>;
}
