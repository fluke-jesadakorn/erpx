import Link from 'next/link';
import {requireWorkContext,publicScope} from '@/server/authorization';
import {workspaceBase} from '@/lib/workspace';
import {pageError} from '@/server/page-error';
import {customerFollowupWorkspace} from '@/server/customer-followups';
import {CustomerFollowupForm} from '@/components/modules/customer-followups';
export const dynamic='force-dynamic';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{orderId?:string}>}){const {holdingSlug}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),{orderId}=await searchParams,data=await customerFollowupWorkspace(a,orderId).catch(e=>pageError(e,holdingSlug));return <div className="content-stack"><Link href={orderId?`${base}/commerce/orders/${orderId}`:`${base}/work`}>Back to {orderId?'source order':'Work'}</Link><h1>Customer follow-ups</h1><p>Assign the next customer contact to an eligible colleague. Start from a sales order or quotation.</p>{data.canCreate&&data.order&&<CustomerFollowupForm base={base} order={data.order} recipients={data.recipients}/>}<section className="card">{data.items.map(r=><Link className="row" key={String(r.id)} href={`${base}/customer-followups/${r.id}`}><strong>{String(r.customer_name)} · {String(r.next_action)}</strong><span>{String(r.status)} · Due {String(r.due_date)}{!Number(r.owner_eligible)&&r.status==='open'?' · Owner needs reassignment':''}</span></Link>)}{!data.items.length&&<p>No follow-ups in this scope.</p>}</section></div>;}
