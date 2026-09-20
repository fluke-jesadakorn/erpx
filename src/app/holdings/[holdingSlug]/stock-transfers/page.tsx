import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { ListPagination } from "@/components/ui/list-pagination";
import { PageHeader,StatusBadge } from "@/components/ui/primitives";
import { TransferEditor } from "@/components/workplace/work-editors";
import { minorToDecimal } from "@/lib/finance-money";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { canPrepareTransfers,transferOptions,transfersPage } from "@/server/inventory-transfers";
import Link from "next/link";
export const dynamic="force-dynamic";
export default async function Transfers({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{q?:string;status?:string;cursor?:string;new?:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));const search=await searchParams;
 const [page,options]=await Promise.all([transfersPage(actor,search),search.new==='1'&&canPrepareTransfers(actor)?transferOptions(actor):null]);
 const filtered=page.items;
 return <FeaturePage className="content-stack" data-source="live"><PageHeader eyebrow="Inventory · Operations" title="Warehouse transfers" description="Move received stock between company warehouses with independent review and preserved valuation." action={<Link className="button secondary" href={`${base}/commerce?view=stock`}>Stock and movements</Link>}/><FeatureTabs/><WidgetGrid pageKey="operations-transfers">{canPrepareTransfers(actor)&&(options?<FeatureWidget id="operations-transfers-warehouse-transfer" title="Warehouse transfer"><TransferEditor base={base} options={options} initialOpen/></FeatureWidget>:<Link className="button" href="?new=1">New transfer</Link>)}<form className="business-form"><label>Search transfers<input name="q" defaultValue={search.q} placeholder="Transfer, warehouse or purpose"/></label><label>Transfer status<select name="status" defaultValue={search.status??""}><option value="">All statuses</option>{["draft","submitted","rejected","posted","cancelled"].map(s=><option key={s} value={s}>{s}</option>)}</select></label><button className="button secondary">Filter transfers</button></form><FeatureWidget id="operations-transfers-transfer-register" title="Transfer register"><div className="section-head"><h2>Transfer register</h2><span className="badge">{filtered.length}</span></div>{filtered.length===0?<p className="muted">No transfers match this view.</p>:filtered.map(t=><Link className="row" key={String(t.id)} href={`${base}/stock-transfers/${t.id}`}><div className="row-main"><strong>{String(t.number)}</strong><div className="muted">{String(t.source_warehouse_name)} → {String(t.target_warehouse_name)}<br/>{String(t.transfer_date)} · {String(t.reason)}</div></div><strong>{String(t.currency)} {minorToDecimal(Number(t.total_value_minor))}</strong><StatusBadge status={t.delivery_mode==='transit'&&t.status==='posted'?'transport_approved':String(t.status)}/></Link>)}</FeatureWidget><ListPagination {...page} query={search} count={filtered.length}/></WidgetGrid></FeaturePage>;
}
