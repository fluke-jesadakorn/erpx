import Link from "next/link";
import { requireWorkContext,publicScope } from "@/server/authorization";
import { listCounts,countOptions,canPrepareCounts } from "@/server/inventory-counts";
import { workspaceBase } from "@/lib/workspace";
import { minorToDecimal } from "@/lib/finance-money";
import { CountEditor } from "@/components/modules/stock-count-controls";
import { PageHeader,StatusBadge } from "@/components/ui/primitives";
export const dynamic="force-dynamic";
export default async function Counts({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{q?:string;status?:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));const [counts,options,search]=await Promise.all([listCounts(actor),countOptions(actor),searchParams]);
 const filtered=counts.filter(c=>(!search.status||c.status===search.status)&&(!search.q||`${c.number} ${c.warehouse_name} ${c.reason}`.toLowerCase().includes(search.q.toLowerCase())));
 return <main className="content-stack" data-source="live"><PageHeader eyebrow="Inventory · Finance" title="Physical stock counts" description="Count actual stock, review differences, and post the quantity and value adjustment together." action={<Link className="button secondary" href={`${base}/commerce?view=stock`}>Stock and movements</Link>}/>{canPrepareCounts(actor)&&<CountEditor base={base} options={options}/>}
 <form className="business-form"><label>Search stock counts<input name="q" defaultValue={search.q} placeholder="Count, warehouse or reason"/></label><label>Count status<select name="status" defaultValue={search.status??""}><option value="">All statuses</option>{["draft","submitted","rejected","posted","cancelled"].map(s=><option key={s} value={s}>{s}</option>)}</select></label><button className="button secondary">Filter counts</button></form>
 <section className="card"><div className="section-head"><h2>Count register</h2><span className="badge">{filtered.length}</span></div>{!filtered.length?<p className="muted">No physical counts match this view.</p>:filtered.map(c=><Link className="row" key={String(c.id)} href={`${base}/stock-counts/${c.id}`}><div className="row-main"><strong>{String(c.number)}</strong><div className="muted">{String(c.warehouse_name)} · {String(c.count_date)}<br/>{String(c.reason)}</div></div><strong>{options.currency} {minorToDecimal(Number(c.net_value_minor))}</strong><StatusBadge status={String(c.status)}/></Link>)}</section></main>;
}
