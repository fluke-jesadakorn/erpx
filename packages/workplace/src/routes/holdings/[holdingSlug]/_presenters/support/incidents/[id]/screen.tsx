import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { InlineNotice } from "@erpx/ui/components/foundation";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import {requireWorkContext,publicScope} from "@erpx/server-identity_access/public/authorization";
import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";
import {pageError} from "@erpx/server-work_projection/public/page-error";
import {incidentDetail} from '@erpx/server-support/public/support-incidents';
import { SupportIncidentConsole } from "@erpx/feature-records/components/loaders/modules--support-incident-console";
import { PageContent, Card } from "@erpx/ui/components/foundation";
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}) {
 const {holdingSlug,id}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await incidentDetail(a,id).catch(e=>pageError(e,holdingSlug)),r=data.row;
 return <PageContent as="div" className="ui-page-content"><Link href={canonicalUiHref(`${base}/support/incidents`)}>Shared incidents</Link><h1>{String(r.title)}</h1><Card as="section" appearance="standard" className="ui-panel"><p>{String(r.severity)} · {String(r.status)} · Next update {String(r.due_date)}</p><p>{String(r.impact)}</p>{r.root_cause&&<p>Root cause: {String(r.root_cause)}</p>}{r.mitigation&&<p>Mitigation: {String(r.mitigation)}</p>}{!['closed','cancelled'].includes(String(r.status))&&!Number(r.owner_eligible)&&<InlineNotice tone="danger" role="alert">Incident owner is no longer eligible. A Support manager must reassign the incident.</InlineNotice>}{r.status==='mitigated'&&!Number(r.reviewer_count)&&<InlineNotice tone="danger" role="alert">No independent eligible reviewer. Administrator coverage is required.</InlineNotice>}</Card><Card as="section" appearance="standard" className="ui-panel"><h2>Affected customer cases</h2><p>Confirming this incident does not close these cases or stop their clocks.</p>{data.links.map(t=><Link className="ui-record-item" key={String(t.id)} href={canonicalUiHref(`${base}/support/${t.id}`)}>{String(t.title)} · {String(t.status)}</Link>)}</Card><SupportIncidentConsole key={String(r.version)} base={base} detail={data}/><Card as="section" appearance="standard" className="ui-panel"><h2>Incident history</h2>{data.history.map(e=><p key={String(e.id)}>Version {String(e.version)} · {String(e.action)} · {String(e.reason)}</p>)}</Card></PageContent>;
}
