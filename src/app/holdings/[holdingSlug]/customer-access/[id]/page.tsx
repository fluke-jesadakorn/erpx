import Link from 'next/link';
import {requireWorkContext,publicScope} from '@/server/authorization';
import {workspaceBase} from '@/lib/workspace';
import {pageError} from '@/server/page-error';
import {customerAccessHistory} from '@/server/customer-access';
import {CustomerAccessConsole} from '@/components/modules/customer-access-console';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){const {holdingSlug,id}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await customerAccessHistory(a,id).catch(e=>pageError(e,holdingSlug)),r=data.binding;return <div className="content-stack"><Link href={`${base}/customer-access`}>Customer access</Link><h1>{String(r.number)}</h1><CustomerAccessConsole key={`${r.id}:${r.version}`} base={base} detail={data}/><section className="card"><h2>Immutable decision history</h2>{data.events.map(e=><p key={`${e.version}`}>Version {String(e.version)} · {String(e.action)} · {String(e.reason)}</p>)}</section><section className="card"><h2>Customer read audit</h2>{data.reads.map((e,i)=><p key={`${e.read_at}:${i}`}>{new Date(Number(e.read_at)).toISOString()} · binding version {String(e.binding_version)} · projection {String(e.projection_version)}</p>)}{!data.reads.length&&<p>No customer reads recorded.</p>}</section></div>;}
