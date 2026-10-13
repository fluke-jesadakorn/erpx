import { RecordLink as Link } from "@/components/records/record-link";
import {TimeText} from "@/components/people_payroll/time/time-preference-context";
import {requireWorkContext,publicScope} from "@/server/modules/identity_access/public/authorization";
import {workspaceBase} from "@/lib/work_projection/workspace";
import {pageError} from "@/server/modules/work_projection/public/page-error";
import {operationsExceptionDetail} from "@/server/modules/operations/public/operations-exceptions";
import { OperationsConsole } from "@/components/records/loaders/modules--operations-console";
import { PageContent, Card } from "@/components/ui/foundation";
export const dynamic="force-dynamic";
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){
 const {holdingSlug,id}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await operationsExceptionDetail(a,id).catch(e=>pageError(e,holdingSlug)),r=data.row,policy=JSON.parse(r.policy_snapshot_json) as {area:string;revision:number;targetMinutes:number};
 return <PageContent as="div" className="ui-page-content"><Link href={`${base}/operations`}>Operations queue</Link><h1>{r.title}</h1><Card as="section" appearance="standard" className="ui-panel"><p>{r.issue}</p><p>{r.status} · {r.priority} · Target <TimeText appearance="standard" value={r.due_at}/></p><Link href={`${base}${r.source.href}`}>{r.source.label}</Link><p>Recorded policy: {policy.area} revision {policy.revision} · {policy.targetMinutes} minutes</p>{r.resolution&&<p>Proposed resolution: {r.resolution}</p>}</Card><OperationsConsole key={r.version} base={base} detail={data}/><Card as="section" appearance="standard" className="ui-panel"><h2>History</h2>{data.history.map(event=><p key={String(event.id)}>Version {String(event.version)} · {String(event.action)} · {String(event.reason)} · <TimeText appearance="standard" value={Number(event.created_at)}/></p>)}</Card></PageContent>;
}
