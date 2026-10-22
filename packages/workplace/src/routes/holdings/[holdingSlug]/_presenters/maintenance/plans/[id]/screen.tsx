import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import {requireWorkContext,publicScope} from "@erpx/server-identity_access/public/authorization";
import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";
import {pageError} from "@erpx/server-work_projection/public/page-error";
import {preventivePlanDetail} from "@erpx/server-maintenance/public/maintenance-preventive";
import { MaintenancePreventiveConsole } from "@erpx/feature-records/components/loaders/modules--maintenance-preventive-console";
import {TimeText} from "@erpx/feature-people_payroll/components/time/time-preference-context";
import { PageContent, Card } from "@erpx/ui/components/foundation";
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){
 const {holdingSlug,id}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await preventivePlanDetail(a,id).catch(error=>pageError(error,holdingSlug)),p=data.plan;
 return <PageContent as="div" className="ui-page-content"><Link href={canonicalUiHref(`${base}/maintenance`)}>Maintenance</Link><h1>{p.asset_code} · {p.title}</h1><p>{p.status} · every {p.interval_days} days · {p.time_zone}</p>
  <p>Next due: <TimeText appearance="standard" value={p.next_due_at}/></p><MaintenancePreventiveConsole base={base} detail={data}/>
  <Card as="section" appearance="standard" className="ui-panel"><h2>Generated work</h2>{data.occurrences.map(o=><p key={String(o.id)}><Link href={canonicalUiHref(`${base}/maintenance/${o.work_order_id}`)}>{String(o.due_local).replace('T',' ')} · {String(o.status)}</Link></p>)}{!data.occurrences.length&&<p>No work has been generated.</p>}</Card>
  <Card as="section" appearance="standard" className="ui-panel"><h2>Plan history</h2>{data.history.map(e=><p key={String(e.id)}>{String(e.action)} · {String(e.reason)}</p>)}</Card>
 </PageContent>;
}
