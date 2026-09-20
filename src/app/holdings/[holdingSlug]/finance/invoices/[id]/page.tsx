import { BusinessDocumentBranding } from "@/components/brand/business-document";
import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { SettlementForm } from "@/components/modules/commerce-controls";
import { DocumentPostingPreview } from "@/components/modules/document-posting-preview";
import { InvoiceActions,InvoiceEditor } from "@/components/modules/invoice-controls";
import { StatusBadge } from "@/components/ui/primitives";
import { FlowPanel,documentFlow } from "@/components/workplace/flow-panel";
import { RecordNextAction } from "@/components/workplace/record-next-action";
import { WorkReturn } from "@/components/workplace/work-return";
import { minorToDecimal } from "@/lib/finance-money";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { commercePermissions } from "@/server/commerce";
import { getExpense } from "@/server/expenses";
import { getInvoice,invoiceWorkspace } from "@/server/finance";
import {holdingResources} from "@/server/runtime";
import { pageError } from "@/server/page-error";
import { ArrowLeft,ArrowUpRight } from "lucide-react";
import Link from "next/link";

export const dynamic = "force-dynamic";

export default async function InvoiceDetail({ params, searchParams }: { params: Promise<{ holdingSlug: string; id: string }>; searchParams: Promise<{ print?: string; brand?: string }> }) {
  const { holdingSlug, id } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const query = await searchParams, printing = query.print === "1";
  let data;
  try { data = await getInvoice(actor, id); } catch (error) { return pageError(error, actor.holdingSlug); }
  const base = workspaceBase(publicScope(actor));
  const invoice = data.invoice;
  const matched=await (await holdingResources(actor.holdingId)).write.prepare("SELECT id,reference FROM procurement_bill_matches WHERE invoice_id=? AND company_id=?").bind(id,actor.companyId).first<{id:string;reference:string}>();
  const options=await invoiceWorkspace(actor);
  const expense=invoice.source_type==="expense_claim"?await getExpense(actor,String(invoice.source_id)):null;
  const canSettle=commercePermissions(actor).finance&&(invoice.kind!=="payable"||(hasPermission(actor,"finance.pay")&&invoice.created_by!==actor.userId))&&(!expense||![expense.created_by,expense.prepared_by,expense.claimant_user_id].includes(actor.userId));
  const flow = await documentFlow(actor,"invoice",id);
  return (
    <FeaturePage className="content-stack" data-source="live" data-module="finance" data-print-document={printing || undefined}><WorkReturn base={base}/>
      <BusinessDocumentBranding actor={actor} kind="invoice" recordId={id} requestedMode={query.brand} title="Invoice" reference={String(invoice.invoice_number)} printing={printing}/>
      <Link className="back-link" href={`${base}/finance/invoices`}><ArrowLeft size={14} /> Back to finance</Link>
      <div className="section-head">
        <div><p className="eyebrow">Finance · invoice detail</p><h1>{String(invoice.invoice_number)}</h1><p className="muted">{String(invoice.party_name)} · {String(invoice.kind)} · {String(invoice.currency)}</p></div>
        <StatusBadge status={String(invoice.status)} />
      </div><FeatureTabs/><WidgetGrid pageKey="finance-invoices-detail">
      <section data-critical-notice><RecordNextAction flow={flow}/></section><section className="card-grid" aria-label="Invoice summary">
        {[["Invoice date", invoice.invoice_date], ["Due date", invoice.due_date], ["Total", `${invoice.currency} ${minorToDecimal(Number(invoice.total_minor))}`], ["Outstanding", `${invoice.currency} ${minorToDecimal(Number(invoice.outstanding_minor))}`]].map(([label, value]) => <article className="card" key={String(label)}><span className="eyebrow">{String(label)}</span><strong>{String(value)}</strong></article>)}
      </section>
      {invoice.status==="draft"&&hasPermission(actor,"finance.write")&&<>{invoice.source_type==="standalone"&&<FeatureWidget id="finance-invoices-detail-invoice-details" title="Invoice details"><InvoiceEditor base={base} options={options} invoice={invoice}/></FeatureWidget>}<InvoiceActions base={base} invoice={invoice} periods={options.periods} canPost={flow.canAct&&hasPermission(actor,"finance.approve")&&invoice.created_by!==actor.userId}/></>}
      {matched&&<Link href={`${base}/supplier-bills/${matched.id}`}>Supplier invoice matching · {matched.reference}</Link>}
      {invoice.source_type==="expense_claim"&&<Link href={`${base}/expenses/${invoice.source_id}`}>Open employee expense claim</Link>}
      {invoice.journal_id&&<Link href={`${base}/finance/journals/${invoice.journal_id}`}>Open posted journal</Link>}
      {["commerce_fulfillment","standalone","expense_claim"].includes(String(invoice.source_type)) && ["posted","partially_paid"].includes(String(invoice.status)) && Number(invoice.outstanding_minor)>0 && canSettle && flow.canAct && <SettlementForm base={base} invoice={invoice}/> }
      <FeatureWidget id="finance-invoices-detail-allocations" title="Allocations">
        <div className="section-head"><h2>Allocations</h2><span className="badge">{data.allocations.length}</span></div>
        {data.allocations.length === 0 ? <p className="muted">No payments have been allocated to this invoice.</p> : data.allocations.map((allocation) => <div className="row" key={String(allocation.id)}><div className="row-main"><strong className="row-title">{String(allocation.payment_number)}</strong>{!!allocation.payment_slip_id&&<a href={`/api/v1${base}/attachments/${allocation.payment_slip_id}/content`}>Bank payment slip</a>}<span className="muted">{String(allocation.status)} · {String(allocation.created_at)}</span></div><strong>{String(invoice.currency)} {minorToDecimal(Number(allocation.amount_minor))}<ArrowUpRight size={14} /></strong></div>)}
      </FeatureWidget>
    <DocumentPostingPreview actor={actor} kind="invoice" id={id}/><FlowPanel flow={flow} actor={actor} kind="invoice" id={id} base={base}/></WidgetGrid></FeaturePage>
  );
}
