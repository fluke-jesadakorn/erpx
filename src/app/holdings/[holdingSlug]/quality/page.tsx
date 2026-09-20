import {notFound} from 'next/navigation';
import {accessPage} from '@/server/page-error';
import {ApiError} from '@/server/http';
import Link from 'next/link';
import {requireWorkContext,publicScope} from '@/server/authorization';
import {workspaceBase} from '@/lib/workspace';
import {qualityWorkspace} from '@/server/quality';
import {QualityConsole} from '@/components/modules/quality-console';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){const actor=await requireWorkContext((await params).holdingSlug),base=workspaceBase(publicScope(actor));let data:Awaited<ReturnType<typeof qualityWorkspace>>;try{data=await qualityWorkspace(actor);}catch(error){if(error instanceof ApiError&&error.status===403)return accessPage('permission',actor.holdingSlug);if(error instanceof ApiError&&error.status===404)notFound();throw error;}return <div className="content-stack"><h1>Quality holds</h1><p>A hold quarantines the entire product at one warehouse, including existing reservations and incoming receipts. Each active hold must be independently released after inspection, or independently cancelled before inspection if it was created in error.</p><QualityConsole base={base} data={data}/><section className="card"><h2>Review queue</h2>{data.holds.length?<table className="business-table"><thead><tr><th>Product / warehouse</th><th>Due date</th><th>Status</th><th>Next team</th></tr></thead><tbody>{data.holds.map(h=><tr key={String(h.id)}><td><Link href={`${base}/quality/${h.id}`}>{String(h.product_name)} · {String(h.warehouse_name)}</Link></td><td>{String(h.due_date)}</td><td>{String(h.status)}</td><td>{h.status==='released'?'Completed':Number(h.eligible_count)?h.status==='held'?'Quality inspector':'Independent Quality manager':'Blocked: administrator must assign an eligible recipient'}</td></tr>)}</tbody></table>:<p>No quality holds in your scope.</p>}</section></div>;}
