import { TimeText } from "@/components/time/time-preference-context";
import Link from 'next/link';
import {requireWorkContext,publicScope} from '@/server/authorization';
import {workspaceBase} from '@/lib/workspace';
import {pageError} from '@/server/page-error';
import {customerAccessWorkspace} from '@/server/customer-access';
import {CustomerAccessConsole} from '@/components/modules/customer-access-console';
import { PageContent, Card } from "@/components/ui/foundation";
export const dynamic='force-dynamic';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{orderId?:string}>}){const {holdingSlug}=await params,{orderId}=await searchParams,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await customerAccessWorkspace(a).catch(e=>pageError(e,holdingSlug));return <PageContent as="div" className="ui-page-content"><Link href={`${base}/work`}>Work</Link><h1>Customer access</h1><p>Bind a verified customer email to one approved source document. Another eligible sales manager must review access before publication.</p><CustomerAccessConsole base={base} data={data} selectedOrderId={orderId}/><Card as="section" appearance="standard" className="ui-panel"><h2>Requests and bindings</h2>{data.rows.map(r=><Link className="ui-record-item" key={String(r.id)} href={`${base}/customer-access/${r.id}`}><strong>{String(r.number)} · {String(r.email)}</strong><span>{String(r.status)} · expires {<TimeText appearance="standard" value={Number(r.expires_at)} />}</span></Link>)}{!data.rows.length&&<p>No customer access requests in this scope.</p>}</Card></PageContent>;}
