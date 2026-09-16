import { hasPermission } from "@/lib/permissions";
import { pageError } from "@/server/page-error";
import { DocumentPostingPreview } from "@/components/modules/document-posting-preview";
import { FlowPanel, documentFlow } from "@/components/workplace/flow-panel";
import { getExpense } from "@/server/expenses";
import { SettlementForm } from "@/components/modules/commerce-controls";
import { commercePermissions } from "@/server/commerce";
import Link from "next/link";
import { ArrowLeft, ArrowUpRight } from "lucide-react";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { InvoiceEditor,InvoiceActions } from "@/components/modules/invoice-controls";
import { getInvoice,invoiceWorkspace } from "@/server/finance";
import { workspaceBase } from "@/lib/workspace";
import { minorToDecimal } from "@/lib/finance-money";
import { StatusBadge } from "@/components/ui/primitives";

export const dynamic = "force-dynamic";

export default async function InvoiceDetail({ params }: { params: Promise<{ holdingSlug: string; id: string }> }) {
  const { holdingSlug, id } = await params;
  const actor = await requireWorkContext(holdingSlug);
  let data;
  try { data = await getInvoice(actor, id); } catch (error) { return pageError(error, actor.holdingSlug); }
  const base = workspaceBase(publicScope(actor));
  const invoice = data.invoice;
  const options=await invoiceWorkspace(actor);
  const expense=invoice.source_type==="expense_claim"?await getExpense(actor,String(invoice.source_id)):null;
  const canSettle=commercePermissions(actor).finance&&(invoice.kind!=="payable"||(hasPermission(actor,"finance.pay")&&invoice.created_by!==actor.userId))&&(!expense||![expense.created_by,expense.prepared_by,expense.claimant_user_id].includes(actor.userId));
  const flow = await documentFlow(actor,"invoice",id);
  return (
    <main className="content-stack" data-source="live" data-module="finance">
      <Link className="back-link" href={`${base}/finance/invoices`}><ArrowLeft size={14} /> Back to finance</Link>
      <div className="section-head">
        <div><p className="eyebrow">Finance · invoice detail</p><h1>{String(invoice.invoice_number)}</h1><p className="muted">{String(invoice.party_name)} · {String(invoice.kind)} · {String(invoice.currency)}</p></div>
        <StatusBadge status={String(invoice.status)} />
      </div>
      <section className="card-grid" aria-label="Invoice summary">
        {[["Invoice date", invoice.invoice_date], ["Due date", invoice.due_date], ["Total", `${invoice.currency} ${minorToDecimal(Number(invoice.total_minor))}`], ["Outstanding", `${invoice.currency} ${minorToDecimal(Number(invoice.outstanding_minor))}`]].map(([label, value]) => <article className="card" key={String(label)}><span className="eyebrow">{String(label)}</span><strong>{String(value)}</strong></article>)}
      </section>
      {invoice.status==="draft"&&hasPermission(actor,"finance.write")&&<>{invoice.source_type==="standalone"&&<InvoiceEditor base={base} options={options} invoice={invoice}/>}<InvoiceActions base={base} invoice={invoice} periods={options.periods} canPost={flow.canAct&&hasPermission(actor,"finance.approve")&&invoice.created_by!==actor.userId}/></>}
      {invoice.source_type==="expense_claim"&&<Link href={`${base}/expenses/${invoice.source_id}`}>Open employee expense claim</Link>}
      {invoice.journal_id&&<Link href={`${base}/finance/journals/${invoice.journal_id}`}>Open posted journal</Link>}
      {["commerce_fulfillment","standalone","expense_claim"].includes(String(invoice.source_type)) && ["posted","partially_paid"].includes(String(invoice.status)) && Number(invoice.outstanding_minor)>0 && canSettle && flow.canAct && <SettlementForm base={base} invoice={invoice}/> }
      <section className="card">
        <div className="section-head"><h2>Allocations</h2><span className="badge">{data.allocations.length}</span></div>
        {data.allocations.length === 0 ? <p className="muted">No payments have been allocated to this invoice.</p> : data.allocations.map((allocation) => <div className="row" key={String(allocation.id)}><div className="row-main"><strong className="row-title">{String(allocation.payment_number)}</strong>{!!allocation.payment_slip_id&&<a href={`/api/v1${base}/attachments/${allocation.payment_slip_id}/content`}>Bank payment slip</a>}<span className="muted">{String(allocation.status)} · {String(allocation.created_at)}</span></div><strong>{String(invoice.currency)} {minorToDecimal(Number(allocation.amount_minor))}<ArrowUpRight size={14} /></strong></div>)}
      </section>
    <DocumentPostingPreview actor={actor} kind="invoice" id={id}/><FlowPanel flow={flow} actor={actor} kind="invoice" id={id} base={base}/></main>
  );
}
