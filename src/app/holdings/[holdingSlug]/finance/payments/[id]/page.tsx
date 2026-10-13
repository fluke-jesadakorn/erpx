import { FinanceRecordWorkspace, FinanceLegacyHeading } from "@/components/records/loaders/finance--finance-record-workspace";
import { TimeText } from "@/components/people_payroll/time/time-preference-context";
import { BusinessDocumentBranding } from "@/components/brand/business-document";
import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/shared/features/feature-page";
import { StatusBadge } from "@/components/ui/primitives";
import { PaymentCorrection } from "@/components/records/loaders/modules--payment-correction";
import { PaymentLifecycle } from "@/components/records/loaders/modules--payment-lifecycle";
import { PaymentWithholdingPanel } from "@/components/records/loaders/finance--finance-thai-tax-console";
import {withholdingWorkspace} from "@/server/modules/finance/public/finance-withholding";
import { minorToDecimal } from "@/lib/finance/finance-money";
import { hasPermission } from "@/lib/identity_access/permissions";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { publicScope,requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { getPayment } from "@/server/modules/finance/public/finance";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { ArrowLeft,ArrowUpRight } from "lucide-react";
import { RecordLink as Link } from "@/components/records/record-link";
import { Card } from "@/components/ui/foundation";

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
  const withholding=companyScope&&payment.kind==="payment"?await withholdingWorkspace(actor,id).catch(error=>pageError(error,actor.holdingSlug)):null;
  const lifecycleAction=payment.status==="draft"&&companyScope&&hasPermission(actor,"finance.approve")&&payment.created_by!==actor.userId?"approve":payment.status==="approved"&&companyScope&&hasPermission(actor,"finance.pay")&&payment.created_by!==actor.userId&&payment.approved_by!==actor.userId?"release":payment.status==="released"&&companyScope&&hasPermission(actor,"finance.pay")&&payment.created_by!==actor.userId&&payment.approved_by!==actor.userId&&payment.released_by===actor.userId?"confirm":null;
  return (
    <FeaturePage className="ui-page-content" data-source="live" data-module="finance" data-print-document={printing || undefined}>
      <FinanceRecordWorkspace base={base} active="cash" title={String(payment.payment_number)} description={`${payment.party_name ?? "Unassigned party"} · ${payment.kind} · ${payment.currency}`} actions={<StatusBadge appearance="standard" status={String(payment.status)}/>}>
      <BusinessDocumentBranding actor={actor} kind="payment" recordId={id} requestedMode={query.brand} title="Payment" reference={String(payment.payment_number)} printing={printing}/>
<FinanceLegacyHeading>      <Link className="back-link" href={`${base}/finance`}><ArrowLeft size={14} /> Back to finance</Link>
      <div className="ui-section-header">
        <div><p className="eyebrow">Finance · payment detail</p><h1>{String(payment.payment_number)}</h1><p className="muted">{String(payment.party_name ?? "Unassigned party")} · {String(payment.kind)} · {String(payment.currency)}</p></div>
        <StatusBadge appearance="standard" status={String(payment.status)} />
      </div></FinanceLegacyHeading><FeatureTabs/><WidgetGrid pageKey="finance-payments-detail">
      <FeatureWidget appearance="standard" id="finance-payments-detail-payment-details" title="Payment details">
        {[["Payment date", payment.payment_date], ["Bank account", payment.bank_account ?? "Not assigned"], ["Amount", `${payment.currency} ${minorToDecimal(Number(payment.amount_minor))}`], ["Outstanding", `${payment.currency} ${minorToDecimal(Number(payment.outstanding_minor))}`]].map(([label, value]) => <Card as="article" appearance="standard" className="ui-panel" key={String(label)}><span className="eyebrow">{String(label)}</span><strong>{String(value)}</strong></Card>)}
        {Number(payment.withheld_minor)>0&&<Card as="article" appearance="standard" className="ui-panel"><span className="eyebrow">Withholding and net bank amount</span><strong>THB {minorToDecimal(Number(payment.withheld_minor))} withheld · THB {minorToDecimal(Number(payment.amount_minor)-Number(payment.withheld_minor))} to bank</strong></Card>}
      </FeatureWidget>
      {withholding&&<FeatureWidget appearance="standard" id="finance-payments-detail-withholding" title="Supplier withholding"><PaymentWithholdingPanel base={base} data={withholding} userId={actor.userId} canPrepare={hasPermission(actor,"finance.write")} canApprove={hasPermission(actor,"finance.approve")}/></FeatureWidget>}
      <FeatureWidget appearance="standard" id="finance-payments-detail-invoice-allocations" title="Invoice allocations">
        <p className="muted">{data.allocations.length} allocated invoices</p>
        {data.allocations.length === 0 ? <p className="muted">This payment has not been allocated yet.</p> : data.allocations.map((allocation) => <div className="ui-record-item" key={String(allocation.id)}><div className="row-main"><Link className="row-title" href={`${base}/finance/invoices/${allocation.invoice_id}`}>{String(allocation.invoice_number)}</Link><span className="muted">{String(allocation.status)} · <TimeText appearance="standard" value={Number(allocation.created_at)}/></span></div><strong>{String(payment.currency)} {minorToDecimal(Number(allocation.amount_minor))}<ArrowUpRight size={14} /></strong></div>)}
      </FeatureWidget>
      {payment.journal_id&&<FeatureWidget appearance="standard" id="finance-payments-detail-accounting" title="Accounting evidence"><Link href={`${base}/finance/journals/${payment.journal_id}`}>Open posted payment journal <ArrowUpRight size={14}/></Link></FeatureWidget>}
      {payment.fx_journal_id&&<FeatureWidget appearance="standard" id="finance-payments-detail-fx-accounting" title="Realized currency result"><p>Settlement rate: {Number(payment.fx_rate_micros)/1_000_000} · Functional amount: {minorToDecimal(Number(payment.base_amount_minor))}</p><Link href={`${base}/finance/journals/${payment.fx_journal_id}`}>Open realized FX journal <ArrowUpRight size={14}/></Link></FeatureWidget>}
      {lifecycleAction&&<FeatureWidget appearance="standard" id="finance-payments-detail-lifecycle" title="Controlled payment workflow"><PaymentLifecycle base={base} payment={payment} periods={data.periods} invoices={data.invoices} bankTransactions={data.bankTransactions} fxAccounts={data.fxAccounts} action={lifecycleAction}/></FeatureWidget>}
      {payment.status==="settled"&&hasPermission(actor,"finance.approve")&&payment.created_by!==actor.userId&&<FeatureWidget appearance="standard" id="finance-payments-detail-payment-correction" title="Payment correction"><details><summary>Review a payment correction</summary><PaymentCorrection base={base} payment={payment} periods={data.periods}/></details></FeatureWidget>}
      {data.reversal&&<FeatureWidget appearance="standard" id="finance-payments-detail-correction-evidence" title="Correction evidence"><Card as="section" appearance="standard" className="ui-panel"><p><strong>{String(data.reversal.reason)}</strong></p><p className="muted">Reversed by {String(data.reversal.reversed_by)} · {<TimeText appearance="standard" value={Number(data.reversal.reversed_at)} />}</p><Link href={`${base}/finance/journals/${data.reversal.reversal_journal_id}`}>Open reversing journal</Link></Card></FeatureWidget>}
    </WidgetGrid></FinanceRecordWorkspace></FeaturePage>
  );
}
