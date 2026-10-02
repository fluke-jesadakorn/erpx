import { FinanceRecordWorkspace, FinanceLegacyHeading } from "@/components/finance/finance-record-workspace";
import { TimeText } from "@/components/time/time-preference-context";
import { BusinessDocumentBranding } from "@/components/brand/business-document";
import { TaxDocumentReference } from "@/components/hospitality/tax-document-reference";
import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { SettlementForm } from "@/components/modules/commerce-controls";
import { PrepareControlledPayment } from "@/components/modules/payment-lifecycle";
import { DocumentPostingPreview } from "@/components/modules/document-posting-preview";
import { MoneyAmount } from "@/components/money/currency-provider";
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
  const owningAccount=options.accounts.find(account=>account.id===invoice.debit_account_id),owningChart=options.charts.find(chart=>chart.id===owningAccount?.chart_id);
  const expense=invoice.source_type==="expense_claim"?await getExpense(actor,String(invoice.source_id)):null;
  const canSettle=commercePermissions(actor).finance&&(invoice.kind!=="payable"||(hasPermission(actor,"finance.pay")&&invoice.created_by!==actor.userId))&&(!expense||![expense.created_by,expense.prepared_by,expense.claimant_user_id].includes(actor.userId));
  const bankAccounts=options.accounts.filter(account=>account.category==="asset"&&account.control_kind==="bank"&&account.currency===invoice.currency);
  const flow = await documentFlow(actor,"invoice",id);
  return (
    <FeaturePage className="content-stack" data-source="live" data-module="finance" data-print-document={printing || undefined}><FinanceRecordWorkspace base={base} context={owningChart?{chartId:String(owningChart.id),chartName:String(owningChart.name),currency:String(owningChart.base_currency)}:undefined} active={invoice.kind==="payable"?"payables":"receivables"} title={String(invoice.invoice_number)} description={`${invoice.party_name} · ${invoice.kind} · ${invoice.currency}`} actions={<StatusBadge status={String(invoice.status)}/>}><WorkReturn base={base}/>
      <BusinessDocumentBranding actor={actor} kind="invoice" recordId={id} requestedMode={query.brand} title="Invoice" reference={String(invoice.invoice_number)} printing={printing}/>
      <FinanceLegacyHeading><Link className="back-link" href={`${base}/finance/invoices`}><ArrowLeft size={14} /> Back to finance</Link>
      <div className="section-head">
        <div><p className="eyebrow">Finance · invoice detail</p><h1>{String(invoice.invoice_number)}</h1><p className="muted">{String(invoice.party_name)} · {String(invoice.kind)} · {String(invoice.currency)}</p></div>
        <StatusBadge status={String(invoice.status)} />
      </div></FinanceLegacyHeading><FeatureTabs/><WidgetGrid pageKey="finance-invoices-detail">
      <section data-critical-notice><RecordNextAction flow={flow}/></section><section className="card-grid" aria-label="Invoice summary">
        {[["Invoice date", invoice.invoice_date], ["Due date", invoice.due_date], ["Total", `${invoice.currency} ${minorToDecimal(Number(invoice.total_minor))}`], ["Outstanding", `${invoice.currency} ${minorToDecimal(Number(invoice.outstanding_minor))}`],...(invoice.status!=="draft"?[["Recognition FX rate",Number(invoice.fx_rate_micros)/1_000_000],["Functional outstanding",`${owningChart?.base_currency??"Unresolved currency"} ${minorToDecimal(Number(invoice.base_outstanding_minor))}`]]:[])].map(([label, value]) => <article className="card" key={String(label)}><span className="eyebrow">{String(label)}</span><strong>{String(value)}</strong></article>)}
        <article className="card"><span className="eyebrow">Selected currency · reference only</span><strong><MoneyAmount units={String(invoice.total_minor)} currency={String(invoice.currency)} date={String(invoice.invoice_date)} legacyScale={2} snapshotUsdUnits={invoice.snapshot_usd_units as string|null} snapshotSourceUnits={invoice.snapshot_source_units as string|null} snapshotRateId={invoice.snapshot_rate_id as string|null}/></strong></article>
      </section>
      {invoice.status==="draft"&&hasPermission(actor,"finance.write")&&<>{invoice.source_type==="standalone"&&<FeatureWidget id="finance-invoices-detail-invoice-details" title="Invoice details"><InvoiceEditor base={base} options={options} invoice={invoice}/></FeatureWidget>}<InvoiceActions base={base} invoice={invoice} periods={options.periods} canPost={flow.canAct&&hasPermission(actor,"finance.approve")&&invoice.created_by!==actor.userId}/></>}
      {matched&&<Link href={`${base}/supplier-bills/${matched.id}`}>Supplier invoice matching · {matched.reference}</Link>}
      {invoice.source_type==="expense_claim"&&<Link href={`${base}/expenses/${invoice.source_id}`}>Open employee expense claim</Link>}
      {invoice.journal_id&&<Link href={`${base}/finance/journals/${invoice.journal_id}`}>Open posted journal</Link>}
      {["pos","pms"].includes(String(invoice.source_type))&&<FeatureWidget id="finance-invoices-detail-external-tax-document" title="External tax document"><TaxDocumentReference base={base} invoiceId={id}/></FeatureWidget>}
      {invoice.source_type!=="expense_claim"&&["posted","partially_paid"].includes(String(invoice.status))&&Number(invoice.outstanding_minor)>0&&hasPermission(actor,"finance.write")&&!actor.branchId&&!actor.departmentId&&<FeatureWidget id="finance-invoices-detail-controlled-payment" title="Controlled bank settlement"><PrepareControlledPayment base={base} invoice={invoice} bankAccounts={bankAccounts}/></FeatureWidget>}
      {["commerce_fulfillment","standalone","expense_claim"].includes(String(invoice.source_type)) && ["posted","partially_paid"].includes(String(invoice.status)) && Number(invoice.outstanding_minor)>0 && canSettle && flow.canAct && <SettlementForm base={base} invoice={invoice}/> }
      <FeatureWidget id="finance-invoices-detail-allocations" title="Allocations">
        <p className="muted">{data.allocations.length} allocated payments</p>
        {data.allocations.length === 0 ? <p className="muted">No payments have been allocated to this invoice.</p> : data.allocations.map((allocation) => <div className="row" key={String(allocation.id)}><div className="row-main"><Link className="row-title" href={`${base}/finance/payments/${allocation.payment_id}`}>{String(allocation.payment_number)}</Link>{!!allocation.payment_slip_id&&<a href={`/api/v1${base}/attachments/${allocation.payment_slip_id}/content`}>Bank payment slip</a>}<span className="muted">{String(allocation.status)} · <TimeText value={Number(allocation.created_at)}/></span></div><strong>{String(invoice.currency)} {minorToDecimal(Number(allocation.amount_minor))}<ArrowUpRight size={14} /></strong></div>)}
      </FeatureWidget>
    <DocumentPostingPreview actor={actor} kind="invoice" id={id}/><FlowPanel flow={flow} actor={actor} kind="invoice" id={id} base={base}/></WidgetGrid></FinanceRecordWorkspace></FeaturePage>
  );
}
