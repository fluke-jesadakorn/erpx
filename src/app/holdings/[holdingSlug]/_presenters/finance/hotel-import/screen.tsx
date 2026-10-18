import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { pmsPresentationQueries } from "@/server/modules/pms/public/presentation-queries";

import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { requireWorkContext,publicScope } from "@/server/modules/identity_access/public/authorization";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";

import { hasPermission } from "@/lib/identity_access/permissions";
import { workspaceBase } from "@/lib/work_projection/workspace";


import { StandardPmsImport as PmsImport } from "@/components/pms/standard-pms-import";
import { PageHeader } from "@/components/ui/primitives";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { RecordLink as Link } from "@/components/records/record-link";
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
