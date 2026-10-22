import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { procurementPresentationQueries } from "@erpx/server-procurement/public/presentation-queries";
import { StatusBadge } from "@erpx/ui/components/primitives";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import {requireWorkContext,publicScope} from "@erpx/server-identity_access/public/authorization";
import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";
import {pageError} from "@erpx/server-work_projection/public/page-error";
import {rfqWorkspace} from "@erpx/server-procurement/public/procurement-rfqs";


import {RfqCreate} from "@erpx/feature-procurement/components/procurement-rfq-console";
import { PageContent, Card, Table, TableFrame } from "@erpx/ui/components/foundation";
export const dynamic='force-dynamic';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{request?:string}>}){const {holdingSlug}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await rfqWorkspace(a).catch(e=>pageError(e,holdingSlug)),query=await searchParams,linked=query.request?await (await procurementPresentationQueries(a.holdingId)).requestQuotationSource(a, query.request).catch(e=>pageError(e,holdingSlug)):null,source=linked?{id:linked.request.id,version:linked.request.version,title:linked.request.title,description:linked.request.description,neededDate:linked.request.needed_date,branchId:linked.request.branch_id,items:linked.items}:undefined;return <PageContent as="div" className="ui-page-content"><Link href={canonicalUiHref(`${base}/commerce?view=purchase`)}>Back to purchasing</Link><header><h1>Supplier quotation requests</h1><p>Collect comparable offers, record the selection reason and hand an independently approved award to purchasing.</p></header>{source&&<Link href={canonicalUiHref(`${base}/requests/${source.id}`)}>Approved requisition: {source.title}</Link>}<RfqCreate base={base} data={data} source={source}/><Card as="section" appearance="standard" className="ui-panel"><h2>Latest 100 quotation requests in your scope</h2><TableFrame appearance="standard" label="Supplier quotation requests"><Table><thead><tr><th>Request</th><th>Required date</th><th>Status</th></tr></thead><tbody>{data.rfqs.map(r=><tr key={String(r.id)}><td><Link href={canonicalUiHref(`${base}/procurement-rfqs/${r.id}`)}>{String(r.title)}</Link></td><td>{String(r.needed_date)}</td><td><StatusBadge appearance="standard" status={String(r.status)} /></td></tr>)}</tbody></Table></TableFrame>{!data.rfqs.length&&<p>No quotation requests yet.</p>}</Card></PageContent>;}
