import Link from 'next/link';
import {requireWorkContext,publicScope} from '@/server/authorization';
import {workspaceBase} from '@/lib/workspace';
import {pageError} from '@/server/page-error';
import {incidentDetail} from '@/server/support-incidents';
import {SupportIncidentConsole} from '@/components/modules/support-incident-console';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}) {
 const {holdingSlug,id}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await incidentDetail(a,id).catch(e=>pageError(e,holdingSlug)),r=data.row;
 return <div className="content-stack"><Link href={`${base}/support/incidents`}>Shared incidents</Link><h1>{String(r.title)}</h1><section className="card"><p>{String(r.severity)} · {String(r.status)} · Next update {String(r.due_date)}</p><p>{String(r.impact)}</p>{r.root_cause&&<p>Root cause: {String(r.root_cause)}</p>}{r.mitigation&&<p>Mitigation: {String(r.mitigation)}</p>}{!['closed','cancelled'].includes(String(r.status))&&!Number(r.owner_eligible)&&<p role="alert">Incident owner is no longer eligible. A Support manager must reassign the incident.</p>}{r.status==='mitigated'&&!Number(r.reviewer_count)&&<p role="alert">No independent eligible reviewer. Administrator coverage is required.</p>}</section><section className="card"><h2>Affected customer cases</h2><p>Confirming this incident does not close these cases or stop their clocks.</p>{data.links.map(t=><Link className="row" key={String(t.id)} href={`${base}/support/${t.id}`}>{String(t.title)} · {String(t.status)}</Link>)}</section><SupportIncidentConsole key={String(r.version)} base={base} detail={data}/><section className="card"><h2>Incident history</h2>{data.history.map(e=><p key={String(e.id)}>Version {String(e.version)} · {String(e.action)} · {String(e.reason)}</p>)}</section></div>;
}
