import Link from "next/link";
import { notFound } from "next/navigation";
import { requireWorkContext,publicScope } from "@/server/authorization";
import { countDetail,countOptions,canEditCount,canReviewCount } from "@/server/inventory-counts";
import { ApiError } from "@/server/http";
import { workspaceBase } from "@/lib/workspace";
import { minorToDecimal } from "@/lib/finance-money";
import { CountEditor,CountActions } from "@/components/modules/stock-count-controls";
import { StatusBadge } from "@/components/ui/primitives";
export const dynamic="force-dynamic";
export default async function Count({params}:{params:Promise<{holdingSlug:string;id:string}>}){
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));const [{count,lines,history,decisions},options]=await Promise.all([countDetail(actor,id).catch(e=>{if(e instanceof ApiError&&e.status===404)notFound();throw e;}),countOptions(actor)]);
 const editable=canEditCount(actor,count),stale=lines.some(l=>Number(l.current_version)!==Number(l.snapshot_version));
 return <main className="content-stack" data-source="live"><Link className="back-link" href={`${base}/stock-counts`}>← Physical stock counts</Link><div className="section-head"><div><p className="eyebrow">{String(count.number)}</p><h1>{String(count.warehouse_name)}</h1><p className="muted">{String(count.count_date)} · {String(count.reason)}</p></div><StatusBadge status={String(count.status)}/></div>
 <section className="card-grid"><article className="card"><span className="eyebrow">Products counted</span><strong>{lines.length}</strong></article><article className="card"><span className="eyebrow">Inventory value adjustment</span><strong>{String(count.currency)} {minorToDecimal(Number(count.net_value_minor))}</strong></article></section>
 <section className="card"><h2>Counted quantities and valuation</h2><div className="business-table-wrap"><table className="business-table"><thead><tr><th>Product</th><th>Recorded quantity</th><th>Counted quantity</th><th>Difference</th><th>Value difference ({String(count.currency)})</th></tr></thead><tbody>{lines.map(l=><tr key={String(l.id)}><td>{String(l.sku)} · {String(l.description)}</td><td>{Number(l.snapshot_quantity_milli)/1000} {String(l.unit)}</td><td>{Number(l.counted_quantity_milli)/1000} {String(l.unit)}</td><td>{(Number(l.counted_quantity_milli)-Number(l.snapshot_quantity_milli))/1000}</td><td>{minorToDecimal(Number(l.counted_value_minor)-Number(l.snapshot_value_minor))}</td></tr>)}</tbody></table></div>{count.review_reason&&<p><strong>Review note:</strong> {String(count.review_reason)}</p>}{count.journal_id&&["holding_owner","finance_preparer","finance_approver","treasury","auditor"].includes(actor.role)&&!actor.branchId&&<Link href={`${base}/finance/journals/${count.journal_id}`}>Open inventory adjustment journal</Link>}</section>
 {editable&&<CountEditor key={Number(count.version)} base={base} options={options} count={count} lines={lines}/>}
 <CountActions base={base} count={count} editable={editable} reviewable={canReviewCount(actor,count)} stale={stale}/>
 <section className="card"><h2>Review decisions</h2>{decisions.length===0?<p className="muted">No review decisions yet.</p>:decisions.map(d=><div className="row" key={String(d.id)}><div className="row-main"><strong>{String(d.action)} · {String(d.actor_name)}</strong><p>{String(d.reason)}</p><span className="muted">Reviewed version {Number(d.reviewed_version)} · {new Date(Number(d.created_at)).toISOString().replace("T"," ").slice(0,19)} UTC</span></div></div>)}</section>
 <section className="card"><h2>Activity</h2>{history.map(h=><div className="row" key={String(h.id)}><div className="row-main"><strong>{String(h.action).replaceAll("."," ")}</strong><div className="muted">{String(h.actor_name)} · {new Date(Number(h.created_at)).toISOString().replace("T"," ").slice(0,19)} UTC</div></div></div>)}</section></main>;
}
