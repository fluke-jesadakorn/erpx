import Link from 'next/link';
import {requireWorkContext,publicScope} from '@/server/authorization';
import {workspaceBase} from '@/lib/workspace';
import {pageError} from '@/server/page-error';
import {customerAccessWorkspace} from '@/server/customer-access';
import {CustomerAccessConsole} from '@/components/modules/customer-access-console';
export const dynamic='force-dynamic';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{orderId?:string}>}){const {holdingSlug}=await params,{orderId}=await searchParams,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await customerAccessWorkspace(a).catch(e=>pageError(e,holdingSlug));return <div className="content-stack"><Link href={`${base}/work`}>Work</Link><h1>Customer access</h1><p>Bind a verified customer email to one approved source document. Another eligible sales manager must review access before publication.</p><CustomerAccessConsole base={base} data={data} selectedOrderId={orderId}/><section className="card"><h2>Requests and bindings</h2>{data.rows.map(r=><Link className="row" key={String(r.id)} href={`${base}/customer-access/${r.id}`}><strong>{String(r.number)} · {String(r.email)}</strong><span>{String(r.status)} · expires {new Date(Number(r.expires_at)).toISOString()}</span></Link>)}{!data.rows.length&&<p>No customer access requests in this scope.</p>}</section></div>;}
