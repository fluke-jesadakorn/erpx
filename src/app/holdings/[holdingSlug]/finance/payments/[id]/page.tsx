import { BusinessDocumentBranding } from "@/components/brand/business-document";
import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { StatusBadge } from "@/components/ui/primitives";
import { PaymentCorrection } from "@/components/modules/payment-correction";
import { PaymentLifecycle } from "@/components/modules/payment-lifecycle";
import { minorToDecimal } from "@/lib/finance-money";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { getPayment } from "@/server/finance";
import { pageError } from "@/server/page-error";
import { ArrowLeft,ArrowUpRight } from "lucide-react";
import Link from "next/link";

export const dynamic = "force-dynamic";

export default async function PaymentDetail({ params, searchParams }: { params: Promise<{ holdingSlug: string; id: string }>; searchParams: Promise<{ print?: string; brand?: string }> }) {
  const { holdingSlug, id } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const query = await searchParams, printing = query.print === "1";
  let data;
  try { data = await getPayment(actor, id); } catch (error) { return pageError(error, actor.holdingSlug); }
  const base = workspaceBase(publicScope(actor));
  const payment = data.payment;
  const companyScope=!actor.branchId&&!actor.departmentId;
  const lifecycleAction=payment.status==="draft"&&companyScope&&hasPermission(actor,"finance.approve")&&payment.created_by!==actor.userId?"approve":payment.status==="approved"&&companyScope&&hasPermission(actor,"finance.pay")&&payment.created_by!==actor.userId&&payment.approved_by!==actor.userId?"release":payment.status==="released"&&companyScope&&hasPermission(actor,"finance.pay")&&payment.created_by!==actor.userId&&payment.approved_by!==actor.userId&&payment.released_by===actor.userId?"confirm":null;
  return (
    <FeaturePage className="content-stack" data-source="live" data-module="finance" data-print-document={printing || undefined}>
      <BusinessDocumentBranding actor={actor} kind="payment" recordId={id} requestedMode={query.brand} title="Payment" reference={String(payment.payment_number)} printing={printing}/>
      <Link className="back-link" href={`${base}/finance`}><ArrowLeft size={14} /> Back to finance</Link>
      <div className="section-head">
        <div><p className="eyebrow">Finance · payment detail</p><h1>{String(payment.payment_number)}</h1><p className="muted">{String(payment.party_name ?? "Unassigned party")} · {String(payment.kind)} · {String(payment.currency)}</p></div>
        <StatusBadge status={String(payment.status)} />
      </div><FeatureTabs/><WidgetGrid pageKey="finance-payments-detail">
      <FeatureWidget id="finance-payments-detail-payment-details" title="Payment details">
        {[["Payment date", payment.payment_date], ["Bank account", payment.bank_account ?? "Not assigned"], ["Amount", `${payment.currency} ${minorToDecimal(Number(payment.amount_minor))}`], ["Outstanding", `${payment.currency} ${minorToDecimal(Number(payment.outstanding_minor))}`]].map(([label, value]) => <article className="card" key={String(label)}><span className="eyebrow">{String(label)}</span><strong>{String(value)}</strong></article>)}
      </FeatureWidget>
      <FeatureWidget id="finance-payments-detail-invoice-allocations" title="Invoice allocations">
        <div className="section-head"><h2>Invoice allocations</h2><span className="badge">{data.allocations.length}</span></div>
        {data.allocations.length === 0 ? <p className="muted">This payment has not been allocated yet.</p> : data.allocations.map((allocation) => <div className="row" key={String(allocation.id)}><div className="row-main"><strong className="row-title">{String(allocation.invoice_number)}</strong><span className="muted">{String(allocation.status)} · {String(allocation.created_at)}</span></div><strong>{String(payment.currency)} {minorToDecimal(Number(allocation.amount_minor))}<ArrowUpRight size={14} /></strong></div>)}
      </FeatureWidget>
      {payment.journal_id&&<FeatureWidget id="finance-payments-detail-accounting" title="Accounting evidence"><Link href={`${base}/finance/journals/${payment.journal_id}`}>Open posted payment journal <ArrowUpRight size={14}/></Link></FeatureWidget>}
      {payment.fx_journal_id&&<FeatureWidget id="finance-payments-detail-fx-accounting" title="Realized currency result"><p>Settlement rate: {Number(payment.fx_rate_micros)/1_000_000} · Functional amount: {minorToDecimal(Number(payment.base_amount_minor))}</p><Link href={`${base}/finance/journals/${payment.fx_journal_id}`}>Open realized FX journal <ArrowUpRight size={14}/></Link></FeatureWidget>}
      {lifecycleAction&&<FeatureWidget id="finance-payments-detail-lifecycle" title="Controlled payment workflow"><PaymentLifecycle base={base} payment={payment} periods={data.periods} invoices={data.invoices} bankTransactions={data.bankTransactions} fxAccounts={data.fxAccounts} action={lifecycleAction}/></FeatureWidget>}
      {payment.status==="settled"&&hasPermission(actor,"finance.approve")&&payment.created_by!==actor.userId&&<FeatureWidget id="finance-payments-detail-payment-correction" title="Payment correction"><PaymentCorrection base={base} payment={payment} periods={data.periods}/></FeatureWidget>}
      {data.reversal&&<FeatureWidget id="finance-payments-detail-correction-evidence" title="Correction evidence"><section className="card"><p><strong>{String(data.reversal.reason)}</strong></p><p className="muted">Reversed by {String(data.reversal.reversed_by)} · {new Date(Number(data.reversal.reversed_at)).toISOString()}</p><Link href={`${base}/finance/journals/${data.reversal.reversal_journal_id}`}>Open reversing journal</Link></section></FeatureWidget>}
    </WidgetGrid></FeaturePage>
  );
}
