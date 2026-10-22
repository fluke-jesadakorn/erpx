import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { pmsPresentationQueries } from "@erpx/server-pms/public/presentation-queries";

import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";
import { requireWorkContext,publicScope } from "@erpx/server-identity_access/public/authorization";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";

import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";


import { StandardPmsImport as PmsImport } from "@erpx/feature-pms/components/standard-pms-import";
import { PageHeader } from "@erpx/ui/components/primitives";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
export const dynamic="force-dynamic";
export default async function ImportPage({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{property?:string}>}){
  const {holdingSlug}=await params,actor=requirePermission(await requireWorkContext(holdingSlug),"pms.reports"),base=workspaceBase(publicScope(actor));
  const {property}=await searchParams,presentation=await pmsPresentationQueries(actor.holdingId);
  if(!property)return <div className="content-stack"><PageHeader appearance="standard" title="Hotel cutover import / นำเข้าข้อมูลโรงแรม"/><p>Select a hotel from its Settings page to start.</p><Link href={canonicalUiHref(`${base}/pms/settings`)}>Hotel settings →</Link></div>;
  let snapshot,initial;
  try{snapshot=await presentation.loadPms(actor, property);initial=await presentation.pmsImportBatches(actor, property);}
  catch(error){return pageError(error,holdingSlug);}
  return <div className="content-stack"><PageHeader appearance="standard" eyebrow={snapshot.property.name} title="Hotel cutover import / นำเข้าข้อมูลโรงแรม" action={<UiActionLink appearance="standard" variant="secondary" href={canonicalUiHref(`${base}/pms/settings?property=${property}`)}>Hotel settings</UiActionLink>}/>
    <PmsImport api={`/api/v1${base}/pms/imports`} base={base} initial={initial} propertyId={property} propertyVersion={snapshot.property.version} businessDate={snapshot.property.businessDate} userId={actor.userId} companyId={actor.companyId} canPrepare={hasPermission(actor,"pms.configure")&&hasPermission(actor,"pms.reservations")} canApprove={hasPermission(actor,"pms.accounting.approve")&&hasPermission(actor,"finance.read")}/></div>;
}
