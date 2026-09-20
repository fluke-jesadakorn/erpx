import Link from 'next/link';
import {requireWorkContext,publicScope} from '@/server/authorization';
import {workspaceBase} from '@/lib/workspace';
import {pageError} from '@/server/page-error';
import {supplierBillWorkspace} from '@/server/supplier-bill-matching';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){const {holdingSlug}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await supplierBillWorkspace(a).catch(e=>pageError(e,holdingSlug));return <div className="content-stack"><Link href={`${base}/work`}>Back to Work</Link><h1>Supplier invoice reviews</h1><p>Start matching from a purchase order’s posted goods receipt. Resolve quantity and price exceptions before preparing AP.</p><section className="card">{data.map(r=><Link className="row" key={String(r.id)} href={`${base}/supplier-bills/${r.id}`}><strong>{String(r.reference)} · {String(r.supplier_name)}</strong><span>{String(r.order_number)} · {String(r.status)} · Due {String(r.due_date)}</span></Link>)}{!data.length&&<p>No supplier invoices captured in your scope.</p>}</section></div>;}
