import Link from 'next/link';
import {requireWorkContext,publicScope} from '@/server/authorization';
import {workspaceBase} from '@/lib/workspace';
import {pageError} from '@/server/page-error';
import {supportSlaWorkspace} from '@/server/support-sla';
import {SupportSlaConsole} from '@/components/modules/support-sla-console';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){const {holdingSlug}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await supportSlaWorkspace(a).catch(e=>pageError(e,holdingSlug));return <div className="content-stack"><Link href={`${base}/support`}>Support cases</Link><h1>Resolution-time watch</h1><p>Track approved working-time targets. Resolving a case stops this clock; customer confirmation remains a separate step. Reopening uses the remaining budget.</p><section className="card"><h2>Cases with a target</h2>{data.items.map(r=>{const c=r.clock,breached=c.breachedAt!==null||(c.state==='running'&&c.dueAt!==null&&c.dueAt<data.now);return <Link className="row" key={String(r.id)} href={`${base}/support/${r.id}`}><strong>{String(r.title)} · {breached?'Target breached':c.state}</strong><span>{c.dueAt?`Target ${new Date(c.dueAt).toISOString()}`:c.state==='paused'?'Clock paused':'Clock stopped'} · {String(r.status)}</span></Link>;})}{!data.items.length&&<p>No active cases with an attached target.</p>}</section><SupportSlaConsole base={base} data={data}/></div>;}
