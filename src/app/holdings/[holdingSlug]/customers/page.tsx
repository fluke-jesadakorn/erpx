import Link from 'next/link';
import {requireWorkContext,publicScope} from '@/server/authorization';
import {workspaceBase} from '@/lib/workspace';
import {customerDirectory} from '@/server/customer-timeline';
import {pageError} from '@/server/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){const {holdingSlug}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),rows=await customerDirectory(a).catch(e=>pageError(e,holdingSlug));return <div className="content-stack"><Link href={`${base}/commerce?view=sales`}>Sales</Link><h1>Customer 360</h1><p>Customers with a sales document or qualified opportunity visible in your current workspace.</p><section className="card">{rows.slice(0,200).map(r=><Link className="row" href={`${base}/customers/${r.id}`} key={r.id}>{r.name} · {r.code}</Link>)}{!rows.length&&<p>No customers with visible source records.</p>}{rows.length>200&&<p>Showing the first 200 customers. Open additional customer timelines from their sales documents.</p>}</section></div>;}
