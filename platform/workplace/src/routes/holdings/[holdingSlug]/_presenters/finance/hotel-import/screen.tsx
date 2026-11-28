import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { pmsPresentationQueries } from "@erpx/pms-server/public/presentation-queries";

import { ActionLink as UiActionLink } from "@erpx/platform-ui/components/foundation";
import { requireWorkContext,publicScope } from "@erpx/identity-access-server/public/authorization";
import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";

import { hasPermission } from "@erpx/identity-access-ui/lib/permissions";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";


import { StandardPmsImport as PmsImport } from "@erpx/pms-ui/components/standard-pms-import";
import { PageHeader } from "@erpx/platform-ui/components/primitives";
import { pageError } from "@erpx/work-projection-server/public/page-error";
import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
export const dynamic="force-dynamic";
export default async function ImportPage({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{property?:string}>}){
  const {holdingSlug}=await params,actor=requirePermission(await requireWorkContext(holdingSlug),"pms.reports"),base=workspaceBase(publicScope(actor));
  const {property}=await searchParams,presentation=await pmsPresentationQueries(actor.holdingId);
  if(!property)return <div className="content-stack"><PageHeader appearance="standard" title="Hotel cutover import / นำเข้าข้อมูลโรงแรม"/><p>Select a hotel from its Settings page to start.</p><Link href={canonicalUiHref(`${base}/finance/hospitality-settings`)}>Hotel settings →</Link></div>;
  let snapshot,initial;
  try{snapshot=await presentation.loadPms(actor, property);initial=await presentation.pmsImportBatches(actor, property);}
  catch(error){return pageError(error,holdingSlug);}
  return <div className="content-stack"><PageHeader appearance="standard" eyebrow={snapshot.property.name} title="Hotel cutover import / นำเข้าข้อมูลโรงแรม" action={<UiActionLink appearance="standard" variant="secondary" href={canonicalUiHref(`${base}/finance/hospitality-settings?property=${property}`)}>Hotel settings</UiActionLink>}/>
    <PmsImport api={`/api/v1${base}/pms/imports`} base={base} initial={initial} propertyId={property} propertyVersion={snapshot.property.version} businessDate={snapshot.property.businessDate} userId={actor.userId} companyId={actor.companyId} canPrepare={hasPermission(actor,"pms.configure")&&hasPermission(actor,"pms.reservations")} canApprove={hasPermission(actor,"pms.accounting.approve")&&hasPermission(actor,"finance.read")}/></div>;
}
