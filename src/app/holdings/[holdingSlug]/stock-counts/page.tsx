import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { ListPagination } from "@/components/ui/list-pagination";
import { PageHeader,StatusBadge } from "@/components/ui/primitives";
import { CountEditor } from "@/components/workplace/work-editors";
import { minorToDecimal } from "@/lib/finance-money";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { canPrepareCounts,countOptions,countsPage } from "@/server/inventory-counts";
import Link from "next/link";
export const dynamic="force-dynamic";
export default async function Counts({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{q?:string;status?:string;cursor?:string;new?:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));const search=await searchParams;
 const [page,options]=await Promise.all([countsPage(actor,search),search.new==='1'&&canPrepareCounts(actor)?countOptions(actor):null]);
 const filtered=page.items;
 return <FeaturePage className="content-stack" data-source="live"><PageHeader eyebrow="Inventory · Finance" title="Physical stock counts" description="Count actual stock, review differences, and post the quantity and value adjustment together." action={<Link className="button secondary" href={`${base}/commerce?view=stock`}>Stock and movements</Link>}/><FeatureTabs/><WidgetGrid pageKey="operations-counts">{canPrepareCounts(actor)&&(options?<FeatureWidget id="operations-counts-stock-count" title="Stock count"><CountEditor base={base} options={options} initialOpen/></FeatureWidget>:<Link className="button" href="?new=1">New stock count</Link>)}
 <form className="business-form"><label>Search stock counts<input name="q" defaultValue={search.q} placeholder="Count, warehouse or reason"/></label><label>Count status<select name="status" defaultValue={search.status??""}><option value="">All statuses</option>{["draft","submitted","rejected","posted","cancelled"].map(s=><option key={s} value={s}>{s}</option>)}</select></label><button className="button secondary">Filter counts</button></form>
 <FeatureWidget id="operations-counts-count-register" title="Count register"><div className="section-head"><h2>Count register</h2><span className="badge">{filtered.length}</span></div>{!filtered.length?<p className="muted">No physical counts match this view.</p>:filtered.map(c=><Link className="row" key={String(c.id)} href={`${base}/stock-counts/${c.id}`}><div className="row-main"><strong>{String(c.number)}</strong><div className="muted">{String(c.warehouse_name)} · {String(c.count_date)}<br/>{String(c.reason)}</div></div><strong>{String(c.currency)} {minorToDecimal(Number(c.net_value_minor))}</strong><StatusBadge status={String(c.status)}/></Link>)}</FeatureWidget><ListPagination {...page} query={search} count={filtered.length}/></WidgetGrid></FeaturePage>;
}
