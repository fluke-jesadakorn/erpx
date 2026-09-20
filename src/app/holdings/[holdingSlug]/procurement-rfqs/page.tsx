import Link from 'next/link';
import {requireWorkContext,publicScope} from '@/server/authorization';
import {workspaceBase} from '@/lib/workspace';
import {pageError} from '@/server/page-error';
import {rfqWorkspace} from '@/server/procurement-rfqs';
import {requestQuotationSource} from '@/server/employee-requests';
import {holdingResources} from '@/server/runtime';
import {RfqCreate} from '@/components/modules/procurement-rfq-console';
export const dynamic='force-dynamic';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{request?:string}>}){const {holdingSlug}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await rfqWorkspace(a).catch(e=>pageError(e,holdingSlug)),query=await searchParams,linked=query.request?await requestQuotationSource((await holdingResources(a.holdingId)).write,a,query.request).catch(e=>pageError(e,holdingSlug)):null,source=linked?{id:linked.request.id,version:linked.request.version,title:linked.request.title,description:linked.request.description,neededDate:linked.request.needed_date,branchId:linked.request.branch_id,items:linked.items}:undefined;return <div className="content-stack"><Link href={`${base}/commerce?view=purchase`}>Back to purchasing</Link><header><h1>Supplier quotation requests</h1><p>Collect comparable offers, record the selection reason and hand an independently approved award to purchasing.</p></header>{source&&<Link href={`${base}/requests/${source.id}`}>Approved requisition: {source.title}</Link>}<RfqCreate base={base} data={data} source={source}/><section className="card"><h2>Latest 100 quotation requests in your scope</h2>{data.rfqs.map(r=><Link className="row" key={String(r.id)} href={`${base}/procurement-rfqs/${r.id}`}><strong>{String(r.title)}</strong><span>{String(r.needed_date)} · {String(r.status)}</span></Link>)}{!data.rfqs.length&&<p>No quotation requests yet.</p>}</section></div>;}
