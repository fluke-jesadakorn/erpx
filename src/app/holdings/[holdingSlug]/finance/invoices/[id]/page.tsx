import { Table as StandardTable, TableFrame } from "@/components/ui/foundation";
import { FinanceRecordWorkspace, FinanceLegacyHeading } from "@/components/finance/finance-record-workspace";
import { TimeText } from "@/components/time/time-preference-context";
import { BusinessDocumentBranding } from "@/components/brand/business-document";
import { TaxDocumentReference } from "@/components/hospitality/tax-document-reference";
import {RecordDocumentPanel} from "@/components/attachments/record-document-panel";
import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { SettlementForm } from "@/components/modules/commerce-controls";
import { PrepareControlledPayment } from "@/components/modules/payment-lifecycle";
import {ThaiVatInvoiceIssue} from "@/components/finance/finance-thai-tax-console";
import {taxIdentityWorkspace} from "@/server/finance-tax-identities";
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
import { RecordLink as Link } from "@/components/records/record-link";
import { Card } from "@/components/ui/foundation";

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
  const bankAccounts=options.accounts.filter(account=>account.chart_id===owningChart?.id&&account.category==="asset"&&account.control_kind==="bank"&&account.currency===invoice.currency);
  const flow = await documentFlow(actor,"invoice",id);
  const canIssueThaiVat=invoice.source_type==="standalone"&&invoice.line_mode==="native_lines"&&invoice.kind==="receivable"&&invoice.currency==="THB"&&["posted","partially_paid","paid"].includes(String(invoice.status))&&hasPermission(actor,"finance.approve")&&invoice.created_by!==actor.userId&&!actor.branchId&&!actor.departmentId&&data.thaiDocuments.length===0;
  const taxIdentities=canIssueThaiVat?await taxIdentityWorkspace(actor).catch(error=>pageError(error,actor.holdingSlug)):null;
  return (
    <FeaturePage className="ui-page-content" data-source="live" data-module="finance" data-print-document={printing || undefined}><FinanceRecordWorkspace base={base} context={owningChart?{chartId:String(owningChart.id),chartName:String(owningChart.name),currency:String(owningChart.base_currency)}:undefined} active={invoice.kind==="payable"?"payables":"receivables"} title={String(invoice.invoice_number)} description={`${invoice.party_name} · ${invoice.kind} · ${invoice.currency}`} actions={<StatusBadge appearance="standard" status={String(invoice.status)}/>}><WorkReturn base={base}/>
      <BusinessDocumentBranding actor={actor} kind="invoice" recordId={id} requestedMode={query.brand} title="Invoice" reference={String(invoice.invoice_number)} printing={printing}/>
      <FinanceLegacyHeading><Link className="back-link" href={`${base}/finance/invoices`}><ArrowLeft size={14} /> Back to finance</Link>
      <div className="ui-section-header">
        <div><p className="eyebrow">Finance · invoice detail</p><h1>{String(invoice.invoice_number)}</h1><p className="muted">{String(invoice.party_name)} · {String(invoice.kind)} · {String(invoice.currency)}</p></div>
        <StatusBadge appearance="standard" status={String(invoice.status)} />
      </div></FinanceLegacyHeading><FeatureTabs/><WidgetGrid pageKey="finance-invoices-detail">
      <section data-critical-notice><RecordNextAction flow={flow}/></section><section className="card-grid" aria-label="Invoice summary">
        {[["Invoice date", invoice.invoice_date], ["Due date", invoice.due_date], ["Total", `${invoice.currency} ${minorToDecimal(Number(invoice.total_minor))}`], ["Outstanding", `${invoice.currency} ${minorToDecimal(Number(invoice.outstanding_minor))}`],...(invoice.status!=="draft"?[["Recognition FX rate",Number(invoice.fx_rate_micros)/1_000_000],["Functional outstanding",`${owningChart?.base_currency??"Unresolved currency"} ${minorToDecimal(Number(invoice.base_outstanding_minor))}`]]:[])].map(([label, value]) => <Card as="article" appearance="standard" className="ui-panel" key={String(label)}><span className="eyebrow">{String(label)}</span><strong>{String(value)}</strong></Card>)}
        <Card as="article" appearance="standard" className="ui-panel"><span className="eyebrow">Selected currency · reference only</span><strong><MoneyAmount units={String(invoice.total_minor)} currency={String(invoice.currency)} date={String(invoice.invoice_date)} legacyScale={2} snapshotUsdUnits={invoice.snapshot_usd_units as string|null} snapshotSourceUnits={invoice.snapshot_source_units as string|null} snapshotRateId={invoice.snapshot_rate_id as string|null}/></strong></Card>
      </section>
      {invoice.status==="draft"&&hasPermission(actor,"finance.write")&&<>{invoice.source_type==="standalone"&&<FeatureWidget appearance="standard" id="finance-invoices-detail-invoice-details" title="Invoice details"><InvoiceEditor base={base} options={options} invoice={invoice} invoiceLines={data.lines}/></FeatureWidget>}<InvoiceActions base={base} invoice={invoice} periods={options.periods} fxAccounts={invoice.currency!==owningChart?.base_currency?options.accounts.filter(account=>account.chart_id===owningChart?.id&&account.currency===owningChart?.base_currency&&['other_income','other_expense'].includes(String(account.category))):[]} canPost={flow.canAct&&hasPermission(actor,"finance.approve")&&invoice.created_by!==actor.userId}/></>}
      {!printing&&invoice.kind==="payable"&&<RecordDocumentPanel appearance="standard" base={base} targetType="supplier_bill" targetId={id} expectedTargetVersion={Number(invoice.version)} editable={invoice.status==="draft"&&hasPermission(actor,"finance.write")} title="Supplier invoice documents" purposes={[{key:"invoice_document",label:"Supplier invoice",description:"Keep the original supplier invoice with this payable.",visibility:"Visible only to authorized Finance colleagues in this Company."},{key:"tax_document",label:"Supplier tax document",description:"Keep the source tax invoice or statutory supplier document.",visibility:"Visible only to authorized Finance colleagues in this Company."}]}/>}
      <FeatureWidget appearance="standard" id="finance-invoices-detail-lines" title="Document lines">
        <p className="muted">Revision {String(invoice.line_revision)} · {data.lineHistory.length} retained revisions</p>
        <TableFrame appearance="standard" label="Records"><StandardTable><thead><tr><th>Description</th><th>Quantity</th><th>Unit price</th><th>Net</th><th>VAT</th><th>Total</th></tr></thead><tbody>{data.lines.map(line=><tr key={line.id}><td>{line.description}</td><td>{line.quantity_millis===null?"Whole document":Number(line.quantity_millis)/1000}</td><td>{line.unit_price_minor===null?"—":minorToDecimal(Number(line.unit_price_minor))}</td><td>{minorToDecimal(Number(line.subtotal_minor))}</td><td>{minorToDecimal(Number(line.tax_minor))}</td><td>{minorToDecimal(Number(line.total_minor))}</td></tr>)}</tbody></StandardTable></TableFrame>
      </FeatureWidget>
      {taxIdentities&&<ThaiVatInvoiceIssue base={base} invoice={invoice} data={taxIdentities}/>}
      {data.thaiDocuments.map(document=><p key={String(document.id)}><Link href={`${base}/finance/thai-documents/${document.id}`}>{String(document.document_number)} · retained Thai document edition {Number(document.edition)}</Link></p>)}
      {matched&&<Link href={`${base}/supplier-bills/${matched.id}`}>Supplier invoice matching · {matched.reference}</Link>}
      {invoice.source_type==="expense_claim"&&<Link href={`${base}/expenses/${invoice.source_id}`}>Open employee expense claim</Link>}
      {invoice.journal_id&&<Link href={`${base}/finance/journals/${invoice.journal_id}`}>Open posted journal</Link>}
      {["pos","pms"].includes(String(invoice.source_type))&&<FeatureWidget appearance="standard" id="finance-invoices-detail-external-tax-document" title="External tax document"><TaxDocumentReference base={base} invoiceId={id}/></FeatureWidget>}
      {invoice.source_type!=="expense_claim"&&["posted","partially_paid"].includes(String(invoice.status))&&Number(invoice.outstanding_minor)>0&&hasPermission(actor,"finance.write")&&!actor.branchId&&!actor.departmentId&&<FeatureWidget appearance="standard" id="finance-invoices-detail-controlled-payment" title="Controlled bank settlement"><PrepareControlledPayment base={base} invoice={invoice} bankAccounts={bankAccounts}/></FeatureWidget>}
      {["commerce_fulfillment","standalone","expense_claim"].includes(String(invoice.source_type)) && ["posted","partially_paid"].includes(String(invoice.status)) && Number(invoice.outstanding_minor)>0 && canSettle && flow.canAct && <SettlementForm base={base} invoice={invoice} periods={options.periods} bankAccounts={bankAccounts}/> }
      <FeatureWidget appearance="standard" id="finance-invoices-detail-allocations" title="Allocations">
        <p className="muted">{data.allocations.length} allocated payments</p>
        {data.allocations.length === 0 ? <p className="muted">No payments have been allocated to this invoice.</p> : data.allocations.map((allocation) => <div className="ui-record-item" key={String(allocation.id)}><div className="row-main"><Link className="row-title" href={`${base}/finance/payments/${allocation.payment_id}`}>{String(allocation.payment_number)}</Link>{!!allocation.payment_slip_id&&<a href={`/api/v1${base}/attachments/${allocation.payment_slip_id}/content`}>Bank payment slip</a>}<span className="muted">{String(allocation.status)} · <TimeText appearance="standard" value={Number(allocation.created_at)}/></span></div><strong>{String(invoice.currency)} {minorToDecimal(Number(allocation.amount_minor))}<ArrowUpRight size={14} /></strong></div>)}
      </FeatureWidget>
    <DocumentPostingPreview actor={actor} kind="invoice" id={id}/><FlowPanel flow={flow} actor={actor} kind="invoice" id={id} base={base}/></WidgetGrid></FinanceRecordWorkspace></FeaturePage>
  );
}
