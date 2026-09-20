import Link from 'next/link';
import {requireWorkContext,publicScope} from '@/server/authorization';
import {workspaceBase} from '@/lib/workspace';
import {pageError} from '@/server/page-error';
import {incidentWorkspace} from '@/server/support-incidents';
import {getSupportTicket,supportRecipients} from '@/server/support';
import {SupportIncidentConsole} from '@/components/modules/support-incident-console';
export const dynamic='force-dynamic';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{ticketId?:string}>}) {
 const {holdingSlug}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await incidentWorkspace(a).catch(e=>pageError(e,holdingSlug)),{ticketId}=await searchParams;
 const source=ticketId&&data.canManage?await getSupportTicket(a,ticketId).catch(e=>pageError(e,holdingSlug)):null,recipients=source?await supportRecipients(a,String(source.order_id)):[];
 return <div className="content-stack"><Link href={`${base}/support`}>Support cases</Link><h1>Shared incidents</h1><p>Coordinate a common cause. Each affected customer case keeps its own owner, clock and customer confirmation.</p>{data.canManage&&<form className="card business-form"><label>Initial customer case<select name="ticketId" required defaultValue={ticketId??''}><option value="">Choose case</option>{data.tickets.map(t=><option key={String(t.id)} value={String(t.id)}>{String(t.title)}</option>)}</select></label><button className="button">Choose initial case</button></form>}<SupportIncidentConsole key={ticketId} base={base} source={source?{id:String(source.id),version:Number(source.version)}:undefined} recipients={recipients}/><section className="card">{data.items.map(r=><Link className="row" key={String(r.id)} href={`${base}/support/incidents/${r.id}`}>{String(r.title)} · {String(r.severity)} · {String(r.status)} · Due {String(r.due_date)}</Link>)}{!data.items.length&&<p>No shared incidents in this scope.</p>}</section></div>;
}
