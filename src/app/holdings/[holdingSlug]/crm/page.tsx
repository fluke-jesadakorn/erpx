import Link from 'next/link';
import {requireWorkContext,publicScope} from '@/server/authorization';
import {workspaceBase} from '@/lib/workspace';
import {pageError} from '@/server/page-error';
import {crmWorkspace} from '@/server/crm';
import {crmIdentities} from '@/server/crm-identities';
import {crmAnalytics,analyticsOptions,defaultAnalyticsRange} from '@/server/crm-marketing-report';
import {CRMConsole} from '@/components/modules/crm-console';
import {CRMIdentityConsole} from '@/components/modules/crm-identity-console';
import {AnalyticsReport} from '@/components/modules/analytics-report';
import { PageContent, Card } from "@/components/ui/foundation";
export const dynamic='force-dynamic';
type Query={from?:string;to?:string;branchId?:string;propertyId?:string;campaignId?:string;ownerId?:string};
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<Query>}){
 const {holdingSlug}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),q=await searchParams;
 const defaults=defaultAnalyticsRange(),raw={from:q.from??defaults.from,to:q.to??defaults.to,branchId:q.branchId,propertyId:q.propertyId,campaignId:q.campaignId,ownerId:q.ownerId};
 const [data,identities,report,options]=await Promise.all([crmWorkspace(a),crmIdentities(a),crmAnalytics(a,raw),analyticsOptions(a)]).catch(e=>pageError(e,holdingSlug));
 return <PageContent as="div" className="ui-page-content"><Link href={`${base}/work`}>Back to Work</Link><h1>Leads &amp; opportunities</h1><p>Review customer identities, trace the source of each lead and connect confirmed Sales or hotel outcomes.</p><Link href={`${base}/customer-followups`}>Customer contact activities</Link><AnalyticsReport kind="crm" base={base} report={report} options={options}/><CRMIdentityConsole base={base} data={identities}/><CRMConsole base={base} data={data} identityData={identities}/><Card as="section" appearance="standard" className="ui-panel"><h2>Leads</h2>{data.items.map(r=><Link className="ui-record-item" key={String(r.id)} href={`${base}/crm/${r.id}`}><strong>{String(r.title)} · {String(r.stage??r.status)}</strong><span>{String(r.owner_name??'Owner unavailable')} · Due {String(r.close_date??r.due_date)}</span></Link>)}{!data.items.length&&<p>No leads in this scope.</p>}</Card></PageContent>;
}
