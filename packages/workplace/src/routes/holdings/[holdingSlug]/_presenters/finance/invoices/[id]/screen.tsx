import {nativeServiceInvoiceWorkspace,nativeJournalPageFailure} from "@erpx/feature-finance/public/server";
import {serviceNoChargeControls} from "@erpx/server-finance/public/service-no-charge";
import {ServiceNoChargeConfirmation} from "@erpx/feature-finance/components/service-no-charge-confirmation";
import {financeProductPresentationOptions} from "@erpx/server-finance/public/product-presentation-request";
import {servicePaymentControls} from "@erpx/server-finance/public/service-payment-purchases";
import {supplierInvoiceCorrections} from "@erpx/server-finance/public/supplier-payment-corrections";
import {SupplierPaymentCorrectionPreparation} from "@erpx/feature-finance/components/supplier-payment-correction";
import {SupplierBatchPreparation} from "@erpx/feature-procurement/components/supplier-batch-console";
import {supplierInvoiceBatchWorkspace} from "@erpx/server-procurement/public/supplier-payments";
import {financeActionAllowed} from "@erpx/server-finance/public/rules/purchase-controls";
import {ApiError} from "@erpx/server-runtime/public/http";
import {financeDocumentPostAccess} from "@erpx/server-finance/public/document-purchases";
import { commercePurchaseControls } from "@erpx/server-commerce/public/purchase-controls";
import { CommerceEnvironment } from "@erpx/feature-commerce/components/purchase-context";
import {financeDocumentPresentationAccess,financeDocumentFlow} from "@erpx/server-finance/public/record-presentation";
import { financePurchaseControls } from "@erpx/server-finance/public/purchases";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import {invoiceSalesCompletion,invoiceCustomerReceipts} from '@erpx/server-finance/public/sales';
import { procurementPresentationQueries } from "@erpx/server-procurement/public/presentation-queries";
import { Table as StandardTable, TableFrame } from "@erpx/ui/components/foundation";

import { FinanceRecordWorkspace, FinanceLegacyHeading } from "@erpx/feature-records/components/loaders/finance--finance-record-workspace";
import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import { BusinessDocumentBranding } from "@erpx/feature-brand/components/business-document";
import { TaxDocumentReference } from "@erpx/feature-records/components/loaders/hospitality--tax-document-reference";
import { RecordDocumentPanel } from "@erpx/feature-records/components/loaders/attachments--record-document-panel";
import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@erpx/feature-shared/components/features/feature-page";
import { SettlementForm } from "@erpx/feature-records/components/loaders/modules--commerce-controls";
import { PrepareControlledPayment } from "@erpx/feature-records/components/loaders/modules--payment-lifecycle";
import { ThaiVatInvoiceIssue } from "@erpx/feature-records/components/loaders/finance--finance-thai-tax-console";
import {taxIdentityWorkspace} from "@erpx/server-finance/public/finance-tax-identities";
import { DocumentPostingPreview } from "@erpx/feature-finance/components/document-posting-preview";
import { MoneyAmount } from "@erpx/feature-finance/components/money/currency-provider";
import { InvoiceActions, InvoiceEditor } from "@erpx/feature-records/components/loaders/modules--invoice-controls";
import { StatusBadge } from "@erpx/ui/components/primitives";
import { FlowPanel } from "@erpx/workplace/components/flow-panel";
import { RecordNextAction } from "@erpx/workplace/components/record-next-action";
import { WorkReturn } from "@erpx/feature-records/components/loaders/workplace--work-return";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope,requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import {commercePermissions} from '@erpx/server-commerce/public/commerce';
import {getExpense} from '@erpx/server-people_payroll/public/expense-read';
import {invoiceRecordWorkspace} from "@erpx/server-finance/public/record-presentation";

import { pageError } from "@erpx/server-work_projection/public/page-error";
import { ArrowLeft,ArrowUpRight } from "lucide-react";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { Card } from "@erpx/ui/components/foundation";
import styles from "@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/finance/invoices/[id]/invoice-summary.module.css";

export const dynamic = "force-dynamic";

export default async function InvoiceDetail({ params, searchParams }: { params: Promise<{ holdingSlug: string; id: string }>; searchParams: Promise<{ print?: string; brand?: string }> }) {
  const { holdingSlug, id } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const query = await searchParams, printing = query.print === "1";
  let data;
  let native;
  try { native = await nativeServiceInvoiceWorkspace(actor, holdingSlug, id); data = native ?? await invoiceRecordWorkspace(actor, id); } catch (error) { const mapped=nativeJournalPageFailure(error); return pageError(mapped?new ApiError(mapped.status,mapped.code,mapped.message.replaceAll('Journal','Invoice').replaceAll('journal','invoice')):error, actor.holdingSlug); }
  const base = workspaceBase(publicScope(actor));
  const noCharge=await serviceNoChargeControls(actor,id);
  const servicePaymentPurchase=await servicePaymentControls(actor,{kind:"invoice",id});
  const supplierCorrections=await supplierInvoiceCorrections(actor,id);
  const invoice = data.invoice,purchases=await financePurchaseControls(actor),presentation=native?{sourceOnly:native.sourceOnly,historyOnly:!financeActionAllowed(purchases,"settle-existing")}:await financeDocumentPresentationAccess(actor,"invoice",id,await financeProductPresentationOptions());
  const supplierCandidate=!printing&&invoice.kind==="payable"&&["posted","partially_paid"].includes(String(invoice.status))&&Number(invoice.outstanding_minor)>0&&hasPermission(actor,"finance.write")&&!actor.branchId&&!actor.departmentId;
  const supplierAccess=supplierCandidate?await financePurchaseControls(actor,["finance.banking","finance.inventory-valuation"]):undefined;
  const supplierPreparation=supplierCandidate&&(financeActionAllowed(supplierAccess,"settle-existing","finance.banking")||financeActionAllowed(supplierAccess,"settle-existing","finance.inventory-valuation"))?await supplierInvoiceBatchWorkspace(actor,id).catch(error=>{if(error instanceof ApiError&&error.status===403&&error.code==="PACKAGE_REQUIRED")return null;return pageError(error,holdingSlug);}):null;
  const documentPost = invoice.status === "draft" ? await financeDocumentPostAccess(actor,"invoice",id) : undefined;
  const commercePurchase = invoice.source_type === "commerce_fulfillment" ? await commercePurchaseControls(actor,{type:"invoice",id}) : undefined;
  const matched=await (await procurementPresentationQueries(actor.holdingId)).procurementInvoiceMatchForFinance(id, actor.companyId);
  const options=data.options;
  const owningAccount=options.accounts.find(account=>account.id===invoice.debit_account_id),owningChart=options.charts.find(chart=>chart.id===owningAccount?.chart_id);
  const expense=invoice.source_type==="expense_claim"?await getExpense(actor,String(invoice.source_id)):null;
  const canSettle=commercePermissions(actor).finance&&(invoice.kind!=="payable"||(hasPermission(actor,"finance.pay")&&invoice.created_by!==actor.userId))&&(!expense||![expense.created_by,expense.prepared_by,expense.claimant_user_id].includes(actor.userId));
  const bankAccounts=options.accounts.filter(account=>account.chart_id===owningChart?.id&&account.category==="asset"&&account.control_kind==="bank"&&account.currency===invoice.currency);
  const flow = await financeDocumentFlow(actor,"invoice",id);
  const canIssueThaiVat=(invoice.source_type==="commerce_fulfillment"||invoice.source_type==="standalone"&&invoice.line_mode==="native_lines")&&invoice.kind==="receivable"&&invoice.currency==="THB"&&["posted","partially_paid","paid"].includes(String(invoice.status))&&hasPermission(actor,"finance.approve")&&invoice.created_by!==actor.userId&&!actor.branchId&&!actor.departmentId&&data.thaiDocuments.length===0;
  const taxIdentities=canIssueThaiVat?await taxIdentityWorkspace(actor).catch(error=>pageError(error,actor.holdingSlug)):null;
  const customerReceipts = invoice.kind === "receivable" ? await invoiceCustomerReceipts(actor, id) : [];
  const completionEvidence = invoice.source_type === "standalone" ? await invoiceSalesCompletion(actor, id) : null;
  return (
    <FeaturePage className="ui-page-content" data-source="live" data-module="finance" data-print-document={printing || undefined}><FinanceRecordWorkspace sourceHistory={presentation.sourceOnly} purchases={purchases} base={base} context={owningChart?{chartId:String(owningChart.id),chartName:String(owningChart.name),currency:String(owningChart.base_currency)}:undefined} active={invoice.kind==="payable"?"payables":"receivables"} title={String(invoice.invoice_number)} description={`${invoice.party_name} · ${invoice.kind} · ${invoice.currency}`} actions={<StatusBadge appearance="standard" status={String(invoice.status)}/>}><WorkReturn base={base}/>
      <BusinessDocumentBranding actor={actor} kind="invoice" recordId={id} requestedMode={query.brand} title="Invoice" reference={String(invoice.invoice_number)} printing={printing}/>
      <FinanceLegacyHeading>{!presentation.sourceOnly&&<Link className="back-link" href={canonicalUiHref(`${base}/finance/invoices`)}><ArrowLeft size={14} /> Back to finance</Link>}
      <div className="ui-section-header">
        <div><p className="eyebrow">Finance · invoice detail</p><h1>{String(invoice.invoice_number)}</h1><p className="muted">{String(invoice.party_name)} · {String(invoice.kind)} · {String(invoice.currency)}</p></div>
        <StatusBadge appearance="standard" status={String(invoice.status)} />
      </div></FinanceLegacyHeading><FeatureTabs/><WidgetGrid pageKey="finance-invoices-detail">
      <section data-critical-notice><RecordNextAction flow={flow}/></section><section className={`card-grid ${styles.summary}`} aria-label="Invoice summary">
        {[["Invoice date", invoice.invoice_date], ["Due date", invoice.due_date], ["Total", `${invoice.currency} ${minorToDecimal(Number(invoice.total_minor))}`], ["Outstanding", `${invoice.currency} ${minorToDecimal(Number(invoice.outstanding_minor))}`],...(!["draft","no_charge_confirmed"].includes(String(invoice.status))?[["Recognition FX rate",Number(invoice.fx_rate_micros)/1_000_000],["Functional outstanding",`${owningChart?.base_currency??"Unresolved currency"} ${minorToDecimal(Number(invoice.base_outstanding_minor))}`]]:[])].map(([label, value]) => <Card as="article" appearance="standard" className="ui-panel" key={String(label)}><span className="eyebrow">{String(label)}</span><strong>{String(value)}</strong></Card>)}
        <Card as="article" appearance="standard" className="ui-panel"><span className="eyebrow">Selected currency · reference only</span><strong><MoneyAmount units={String(invoice.total_minor)} currency={String(invoice.currency)} date={String(invoice.invoice_date)} legacyScale={2} snapshotUsdUnits={invoice.snapshot_usd_units as string|null} snapshotSourceUnits={invoice.snapshot_source_units as string|null} snapshotRateId={invoice.snapshot_rate_id as string|null}/></strong></Card>
      </section>
      {noCharge?.confirmed&&printing&&<p>Service No-Charge Confirmation: the covered visit has no customer amount to collect.</p>}
      {noCharge&&!printing&&<FeatureWidget appearance="standard" id="finance-invoices-detail-no-charge" title="Service No-Charge Confirmation"><ServiceNoChargeConfirmation base={base} controls={{...noCharge,canConfirm:noCharge.canConfirm&&flow.canAct}}/></FeatureWidget>}
      {!noCharge&&invoice.status==="draft"&&(hasPermission(actor,"finance.write")||hasPermission(actor,"finance.approve"))&&<>{invoice.source_type==="standalone"&&hasPermission(actor,"finance.write")&&<FeatureWidget appearance="standard" id="finance-invoices-detail-invoice-details" title="Invoice details"><details><summary>Advanced invoice editor</summary><InvoiceEditor base={base} options={options} invoice={invoice} invoiceLines={data.lines}/></details></FeatureWidget>}<InvoiceActions canVoid={hasPermission(actor,"finance.write")} sourceCanPost={documentPost?.canPost} base={base} invoice={invoice} periods={options.periods} fxAccounts={invoice.currency!==owningChart?.base_currency?options.accounts.filter(account=>account.chart_id===owningChart?.id&&account.currency===owningChart?.base_currency&&['other_income','other_expense'].includes(String(account.category))):[]} canPost={flow.canAct&&hasPermission(actor,"finance.approve")&&invoice.created_by!==actor.userId}/></>}
      {!printing&&invoice.kind==="payable"&&<RecordDocumentPanel appearance="standard" base={base} targetType="supplier_bill" targetId={id} expectedTargetVersion={Number(invoice.version)} editable={invoice.status==="draft"&&hasPermission(actor,"finance.write")} title="Supplier invoice documents" purposes={[{key:"invoice_document",label:"Supplier invoice",description:"Keep the original supplier invoice with this payable.",visibility:"Visible only to authorized Finance colleagues in this Company."},{key:"tax_document",label:"Supplier tax document",description:"Keep the source tax invoice or statutory supplier document.",visibility:"Visible only to authorized Finance colleagues in this Company."}]}/>}
      <FeatureWidget appearance="standard" id="finance-invoices-detail-lines" title="Document lines">
        <p className="muted">Revision {String(invoice.line_revision)} · {data.lineHistory.length} retained revisions</p>
        <TableFrame appearance="standard" label="Records"><StandardTable><thead><tr><th>Description</th><th>Quantity</th><th>Unit price</th><th>Net</th><th>VAT</th><th>Total</th></tr></thead><tbody>{data.lines.map(line=><tr key={line.id}><td>{line.description}</td><td>{line.quantity_millis===null?"Whole document":Number(line.quantity_millis)/1000}</td><td>{line.unit_price_minor===null?"—":minorToDecimal(Number(line.unit_price_minor))}</td><td>{minorToDecimal(Number(line.subtotal_minor))}</td><td>{minorToDecimal(Number(line.tax_minor))}</td><td>{minorToDecimal(Number(line.total_minor))}</td></tr>)}</tbody></StandardTable></TableFrame>
      </FeatureWidget>
      {completionEvidence && <details><summary>Service completion evidence</summary><p>{completionEvidence.note}</p><p className="muted">Original declaration for invoice revision {completionEvidence.invoiceRevision} · <TimeText appearance="standard" value={completionEvidence.confirmedAt}/></p></details>}
      {invoice.kind === "receivable" && invoice.status!=="no_charge_confirmed" && <p>Tax document: {data.thaiDocuments.length ? "Issued" : invoice.currency === "THB" && data.lines.length > 0 && data.lines.every(line => Number(line.tax_bps) === 700) ? "Ordinary VAT source ready; approved tax identities and independent review are required." : "Review tax eligibility in Finance tax setup."} Payment status: {String(invoice.status)}. <Link href={canonicalUiHref(`${base}/finance/tax`)}>Tax setup and documents</Link></p>}
      {taxIdentities&&<ThaiVatInvoiceIssue base={base} invoice={invoice} data={taxIdentities}/>}
      {data.thaiDocuments.map(document=><p key={String(document.id)}><Link href={canonicalUiHref(`${base}/finance/thai-documents/${document.id}`)}>{String(document.document_number)} · retained Thai document edition {Number(document.edition)}</Link></p>)}
      {matched&&<Link href={canonicalUiHref(`${base}/supplier-bills/${matched.id}`)}>Supplier invoice matching · {matched.reference}</Link>}
      {invoice.source_type==="expense_claim"&&<Link href={canonicalUiHref(`${base}/expenses/${invoice.source_id}`)}>Open employee expense claim</Link>}
      {invoice.journal_id&&<Link href={canonicalUiHref(`${base}/finance/journals/${invoice.journal_id}`)}>Open posted journal</Link>}
      {["pos","pms"].includes(String(invoice.source_type))&&<FeatureWidget appearance="standard" id="finance-invoices-detail-external-tax-document" title="External tax document"><TaxDocumentReference base={base} invoiceId={id}/></FeatureWidget>}
      {invoice.source_type!=="expense_claim"&&(supplierCorrections||["posted","partially_paid"].includes(String(invoice.status))&&Number(invoice.outstanding_minor)>0)&&hasPermission(actor,"finance.write")&&!actor.branchId&&!actor.departmentId&&<FeatureWidget appearance="standard" id="finance-invoices-detail-controlled-payment" title="Controlled bank settlement">{supplierCorrections?<SupplierPaymentCorrectionPreparation base={base} invoice={invoice} corrections={supplierCorrections}/>:<PrepareControlledPayment base={base} invoice={invoice} bankAccounts={bankAccounts} sourcePurchase={servicePaymentPurchase}/>}</FeatureWidget>}
      <SupplierBatchPreparation feature={{id:"finance-invoices-detail-supplier-batch",title:"Supplier payment instructions"}} owner={{companyId:actor.companyId,userId:actor.userId,assignmentId:actor.assignmentId,branchId:actor.branchId,departmentId:actor.departmentId}} base={base} data={supplierPreparation} invoiceId={id}/>
      {["commerce_fulfillment","standalone","expense_claim"].includes(String(invoice.source_type)) && ["posted","partially_paid"].includes(String(invoice.status)) && Number(invoice.outstanding_minor)>0 && canSettle && flow.canAct && <CommerceEnvironment purchase={commercePurchase}><SettlementForm base={base} invoice={invoice} periods={options.periods} bankAccounts={bankAccounts}/></CommerceEnvironment> }
      {!!customerReceipts.length && <FeatureWidget appearance="standard" id="finance-invoices-detail-receipts" title="Customer receipts">{customerReceipts.map(receipt => <p key={String(receipt.id)}><Link href={canonicalUiHref(`${base}/finance/sales/receipts/${receipt.id}`)}>{String(receipt.document_number)}</Link> · {String(receipt.currency)} {minorToDecimal(Number(receipt.amount_minor))} · {["cancelled","reversed","voided"].includes(String(receipt.payment_status)) ? "Cancelled" : "Issued"}</p>)}</FeatureWidget>}
      <FeatureWidget appearance="standard" id="finance-invoices-detail-allocations" title="Allocations">
        <p className="muted">{data.allocations.length} allocated payments</p>
        {data.allocations.length === 0 ? <p className="muted">No payments have been allocated to this invoice.</p> : data.allocations.map((allocation) => <div className="ui-record-item" key={String(allocation.id)}><div className="row-main"><Link className="row-title" href={canonicalUiHref(`${base}/finance/payments/${allocation.payment_id}`)}>{String(allocation.payment_number)}</Link>{!!allocation.payment_slip_id&&<a href={`/api/v1${base}/attachments/${allocation.payment_slip_id}/content`}>Bank payment slip</a>}<span className="muted">{String(allocation.status)} · <TimeText appearance="standard" value={Number(allocation.created_at)}/></span></div><strong>{String(invoice.currency)} {minorToDecimal(Number(allocation.amount_minor))}<ArrowUpRight size={14} /></strong></div>)}
      </FeatureWidget>
    <details><summary>Accounting & audit details</summary><DocumentPostingPreview actor={actor} kind="invoice" id={id}/></details><FlowPanel flow={flow} actor={actor} kind="invoice" id={id} base={base}/></WidgetGrid></FinanceRecordWorkspace></FeaturePage>
  );
}
