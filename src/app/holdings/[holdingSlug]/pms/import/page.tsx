
import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { requireWorkContext,publicScope } from "@/server/authorization";
import { requirePermission } from "@/server/iam-permissions";
import { holdingResources } from "@/server/runtime";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { loadPms } from "@/server/pms";
import { pmsImportBatches } from "@/server/pms/import";
import { PmsImport } from "@/components/pms/import";
import { pageError } from "@/server/page-error";
import { RecordLink as Link } from "@/components/records/record-link";
export const dynamic="force-dynamic";
export default async function ImportPage({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{property?:string}>}){
  const {holdingSlug}=await params,actor=requirePermission(await requireWorkContext(holdingSlug),"pms.reports"),base=workspaceBase(publicScope(actor));
  const {property}=await searchParams,{write}=await holdingResources(actor.holdingId);
  if(!property)return <div className="content-stack"><h1>Hotel cutover import / นำเข้าข้อมูลโรงแรม</h1><p>Select a hotel from its Settings page to start.</p><Link href={`${base}/pms/settings`}>Hotel settings →</Link></div>;
  let snapshot,initial;
  try{snapshot=await loadPms(write,actor,property);initial=await pmsImportBatches(write,actor,property);}
  catch(error){return pageError(error,holdingSlug);}
  return <div className="content-stack"><header className="section-head"><div><p className="eyebrow">{snapshot.property.name}</p><h1>Hotel cutover import / นำเข้าข้อมูลโรงแรม</h1></div><UiActionLink variant="secondary" href={`${base}/pms/settings?property=${property}`}>Hotel settings</UiActionLink></header>
    <PmsImport api={`/api/v1${base}/pms/imports`} base={base} initial={initial} propertyId={property} propertyVersion={snapshot.property.version} businessDate={snapshot.property.businessDate} userId={actor.userId} canPrepare={hasPermission(actor,"pms.configure")&&hasPermission(actor,"pms.reservations")} canApprove={hasPermission(actor,"pms.accounting.approve")&&hasPermission(actor,"finance.read")}/></div>;
}
