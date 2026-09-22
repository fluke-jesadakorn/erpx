import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { ListPagination } from "@/components/ui/list-pagination";
import { InvoiceEditor } from "@/components/workplace/work-editors";
import { minorToDecimal } from "@/lib/finance-money";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { aging, invoiceWorkspace,invoicesPage } from "@/server/finance";
import Link from "next/link";
export const dynamic="force-dynamic";
type AgingRow = { bucket: string; currency: string; due_date: string; outstanding_minor: number | string };
type AgingSummary = { bucket: string; currency: string; amountMinor: number; count: number; oldestDue: string };
function summarizeAging(rows: AgingRow[]): AgingSummary[] {
  const grouped = new Map<string, AgingSummary>();
  for (const row of rows) {
    const key = `${String(row.bucket)}:${String(row.currency)}`;
    const current = grouped.get(key) ?? { bucket: String(row.bucket), currency: String(row.currency), amountMinor: 0, count: 0, oldestDue: String(row.due_date) };
    current.amountMinor += Number(row.outstanding_minor);
    current.count += 1;
    if (String(row.due_date) < current.oldestDue) current.oldestDue = String(row.due_date);
    grouped.set(key, current);
  }
  return [...grouped.values()].sort((a, b) => a.bucket.localeCompare(b.bucket) || a.currency.localeCompare(b.currency));
}
export default async function InvoicesPage({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{q?:string;kind?:string;status?:string;queue?:string;cursor?:string;new?:string}>}){
  const actor=await requireWorkContext((await params).holdingSlug),base=workspaceBase(publicScope(actor)),search=await searchParams,asOf=new Date().toISOString().slice(0,10);
  const [page,data,receivableAging,payableAging]=await Promise.all([invoicesPage(actor,search),search.new==='1'&&hasPermission(actor,"finance.write")?invoiceWorkspace(actor):null,aging(actor,"receivable",asOf),aging(actor,"payable",asOf)]);
  const rows=page.items, receivableSummary=summarizeAging(receivableAging as unknown as AgingRow[]), payableSummary=summarizeAging(payableAging as unknown as AgingRow[]);
  const agingCard=(title:string,summary:AgingSummary[],kind:"receivable"|"payable")=><section className="card" aria-labelledby={`${kind}-aging-title`}><div className="section-heading"><div><h2 id={`${kind}-aging-title`}>{title}</h2><p className="muted">Open posted obligations grouped by days overdue as of {asOf}. Amounts remain separated by currency.</p></div><Link href={`${base}/finance/invoices?kind=${kind}&queue=open`}>Open {kind}s →</Link></div>{summary.length?<div className="business-table-wrap"><table className="business-table"><thead><tr><th>Bucket</th><th>Currency</th><th>Open documents</th><th>Outstanding</th><th>Oldest due</th></tr></thead><tbody>{summary.map(item=><tr key={`${item.bucket}:${item.currency}`}><td>{item.bucket}</td><td>{item.currency}</td><td>{item.count}</td><td>{minorToDecimal(item.amountMinor)}</td><td>{item.oldestDue}</td></tr>)}</tbody></table></div>:<p>No open {kind}s.</p>}</section>;
  return <FeaturePage className="content-stack" data-source="live"><Link href={`${base}/finance`}>← Finance</Link><h1>Receivables and payables</h1><FeatureTabs/><WidgetGrid pageKey="finance-invoices"><p>Search obligations, create service invoices or supplier bills, and record payments against posted invoices.</p>{agingCard("Receivables aging",receivableSummary,"receivable")}{agingCard("Payables aging",payableSummary,"payable")}{hasPermission(actor,"finance.write")&&(data?<FeatureWidget id="finance-invoices-invoice-details" title="Invoice details"><InvoiceEditor base={base} options={data} initialOpen/></FeatureWidget>:<Link className="button" href="?new=1">Create standalone invoice</Link>)}<form className="business-form">{search.queue&&<input type="hidden" name="queue" value={search.queue}/>}<label>Search invoices<input name="q" defaultValue={search.q}/></label><label>Invoice type<select name="kind" defaultValue={search.kind??""}><option value="">All types</option><option value="receivable">Receivables</option><option value="payable">Payables</option></select></label><label>Status<select name="status" defaultValue={search.status??""}><option value="">All statuses</option>{["draft","posted","partially_paid","paid","voided"].map(v=><option key={v}>{v}</option>)}</select></label><button className="button secondary">Filter</button></form><section className="card"><div className="business-table-wrap"><table className="business-table"><thead><tr><th>Invoice</th><th>Party</th><th>Due date</th><th>Total</th><th>Outstanding</th><th>Status</th></tr></thead><tbody>{rows.map(i=><tr key={String(i.id)}><td><Link href={`${base}/finance/invoices/${i.id}`}>{String(i.invoice_number)}</Link></td><td>{String(i.party_name)}</td><td>{String(i.due_date)}</td><td>{String(i.currency)} {minorToDecimal(Number(i.total_minor))}</td><td>{minorToDecimal(Number(i.outstanding_minor))}</td><td>{String(i.status)}</td></tr>)}</tbody></table></div>{!rows.length&&<p>No invoices match this filter.</p>}</section><ListPagination {...page} query={search} count={rows.length}/></WidgetGrid></FeaturePage>;
}
