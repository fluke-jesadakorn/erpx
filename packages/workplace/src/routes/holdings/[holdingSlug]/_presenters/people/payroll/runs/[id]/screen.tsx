import {ApiError} from '@erpx/server-runtime/public/http';
import {getFinalPayCancellation} from '@erpx/server-people_payroll/public/final-pay-cancellation';
import {FinalPayCancellationControls} from '@erpx/feature-records/components/loaders/modules--final-pay-cancellation-controls';
import {payrollReadiness} from "@erpx/server-people_payroll/public/payroll-readiness";
import {getPayrollOtherLiabilities} from '@erpx/server-people_payroll/public/payroll-other-remittance';
import {PayrollOtherRemittancesPanel} from '@erpx/feature-records/components/loaders/modules--payroll-other-remittances-panel';
import {getPayrollLiabilities} from "@erpx/server-people_payroll/public/payroll-remittance";
import {PayrollEnvironment} from "@erpx/feature-people_payroll/components/payroll-environment";
import {payrollPurchaseControls} from "@erpx/server-people_payroll/public/payroll-purchases";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { InlineNotice } from "@erpx/ui/components/foundation";
import { BusinessDocumentBranding } from "@erpx/feature-brand/components/business-document";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { FeaturePage, WidgetGrid } from "@erpx/feature-shared/components/features/feature-page";
import { PayrollCorrectionControls } from "@erpx/feature-records/components/loaders/modules--payroll-controls";
import { PayrollCancellationControls } from "@erpx/feature-records/components/loaders/modules--payroll-cancellation-controls";
import { PayrollAdjustmentControls } from "@erpx/feature-records/components/loaders/modules--payroll-adjustment-controls";
import {payrollAdjustmentPanel} from "@erpx/server-people_payroll/public/payroll-adjustments";
import { PayrollAccountingPanel } from "@erpx/feature-records/components/loaders/modules--payroll-accounting-panel";
import { PayrollPaymentPanel } from "@erpx/feature-records/components/loaders/modules--payroll-payment-panel";
import { PayrollLiabilitiesPanel } from "@erpx/feature-records/components/loaders/modules--payroll-liabilities-panel";
import { PayrollDetailOverview, PayrollDisclosure, PayrollHeading, PayrollItemList, PayrollNote, PayrollPolicySummary, PayrollReadinessPanel } from "@erpx/feature-records/components/loaders/modules--payroll-workspace";
import { FlowPanel,documentFlow } from "@erpx/workplace/components/flow-panel";
import { WorkReturn } from "@erpx/feature-records/components/loaders/workplace--work-return";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope,requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { getPayrollRun, listPayrollRuns, payrollPostingOptions } from "@erpx/server-people_payroll/public/people-payroll";
import { isThaiHotelPayrollMockEnabled } from "@erpx/server-runtime/public/simulation-only-adapters";
import {listPayslips} from "@erpx/server-people_payroll/public/payroll-payslips";
import { Card } from "@erpx/ui/components/foundation";

export const dynamic = "force-dynamic";

export default async function PayrollRunDetail({ params, searchParams }: { params: Promise<{ holdingSlug: string; id: string }>; searchParams: Promise<{ print?: string; brand?: string }> }) {
  const { holdingSlug, id } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const simulationPayroll = isThaiHotelPayrollMockEnabled(actor.holdingSlug);
  const query = await searchParams, printing = query.print === "1";
  let data;
  try { data = await getPayrollRun(actor, id); } catch (error) { return pageError(error, actor.holdingSlug); }
  const purchase = await payrollPurchaseControls(actor);
  const base = workspaceBase(publicScope(actor));
  const run = data.run as Record<string, unknown>;
  const previousItems = hasPermission(actor,"payroll.read") && data.items.length ? await (async()=>{
    const earlier=(await listPayrollRuns(actor)).filter(candidate=>String(candidate.pay_group_id)===String(run.pay_group_id)&&String(candidate.period_end)<String(run.period_start)&&["approved","posted"].includes(String(candidate.status))).sort((a,b)=>String(b.period_end).localeCompare(String(a.period_end)))[0];
    return earlier ? (await getPayrollRun(actor,String(earlier.id))).items : [];
  })() : [];
  const previousNet=Object.fromEntries(previousItems.map(item=>[String(item.engagement_id),Number(item.net_minor)]));
  const flow = await documentFlow(actor,"payroll",id);
  const canCancel = hasPermission(actor,"payroll.prepare") && !actor.branchId && !actor.departmentId
    && ["draft","calculating","failed","preview"].includes(String(run.status)) && run.mode!=="final_pay"
    && !run.submitted_by && !run.approved_by && !run.posted_by
    && (samePerson(actor,String(run.created_by)) || flow.steps.find(step=>step.key===(run.status==="preview"?"preview":"calculate"))?.primary?.userId===actor.userId);
  const postingOptions = hasPermission(actor,"finance.approve") ? await payrollPostingOptions(actor).catch(() => null) : null;
  const paymentBatch = (data.paymentBatch as Record<string, unknown> | null | undefined) ?? null;
  const paymentItems = (data.paymentItems as Record<string, unknown>[] | undefined) ?? [];
  const payslips=run.status==="posted"&&hasPermission(actor,"payroll.read")?await listPayslips(actor,{runId:id,ownOnly:false}):[];
  const adjustments=hasPermission(actor,'payroll.read')?await payrollAdjustmentPanel(actor,id):null;
  const [initialReadiness, initialLiabilities, initialOtherLiabilities, initialFinalPayCancellation] = await Promise.all([
    hasPermission(actor,"payroll.read")&&["draft","calculating","preview","failed","in_review"].includes(String(run.status)) ? payrollReadiness(actor,id).catch(() => null) : null,
    run.status==="posted"&&hasPermission(actor,"finance.pay") ? getPayrollLiabilities(actor,id).catch(() => null) : null,
    run.status==="posted"&&hasPermission(actor,"finance.pay")&&actor.companyId ? getPayrollOtherLiabilities(actor,id).catch(() => null) : null,
    run.mode==="final_pay"&&hasPermission(actor,"payroll.prepare") ? getFinalPayCancellation(actor,id).catch(error=>{if(error instanceof ApiError&&[403,404].includes(error.status))return null;throw error;}) : null,
  ]);
  return (
    <PayrollEnvironment purchase={purchase}><FeaturePage className="ui-page-content" data-source="live" data-print-document={printing || undefined}><WorkReturn base={base}/>
      <BusinessDocumentBranding actor={actor} branchId={null} requestedMode={query.brand} title="Payroll run" reference={`${String(run.period_start)} – ${String(run.period_end)}`} printing={printing}/>
      <PayrollDetailOverview base={base} userId={actor.userId} run={run} itemCount={data.items.length} netPayableMinor={paymentBatch ? Number(paymentBatch.total_minor) : undefined} paymentStatus={run.status==="posted"&&paymentBatch ? String(paymentBatch.status) : undefined} redacted={!hasPermission(actor,"payroll.read")}/>
      {simulationPayroll&&<p className="notice" role="note" data-payroll-adapter="mock">Simulation only: deterministic fixture results; not a Thai statutory calculation. No real payment is made.</p>}
      <WidgetGrid pageKey="people-payroll-detail">
      <section id="payroll-run-work" aria-label="Payroll stage actions">
        {run.status==="cancelled"&&<InlineNotice tone="info" className="notice" role="status">Payroll run cancelled. Original source evidence and review copies remain in history.</InlineNotice>}
        {canCancel&&<PayrollCancellationControls base={base} runId={id} version={Number(run.version)}/>}
        {initialFinalPayCancellation&&<FinalPayCancellationControls base={base} initial={initialFinalPayCancellation}/>}
        {hasPermission(actor,"payroll.read")&&["draft","calculating","preview","failed","in_review"].includes(String(run.status))&&<PayrollReadinessPanel initial={initialReadiness} base={base} runId={id} version={Number(run.version)} userId={actor.userId} canPrepare={hasPermission(actor,"payroll.prepare")} canApprove={hasPermission(actor,"payroll.approve")&&Boolean(run.submitted_by)&&run.submitted_by!==actor.userId} canExport={hasPermission(actor,"payroll.read")} simulationMode={simulationPayroll}/>}
        {run.status==="approved"&&hasPermission(actor,"finance.approve")&&<PayrollAccountingPanel base={base} run={run} options={postingOptions} userId={actor.userId}/>}
        {run.status==="posted"&&(hasPermission(actor,"payroll.payment.read")||hasPermission(actor,"payroll.payment.prepare")||hasPermission(actor,"finance.pay"))&&<PayrollPaymentPanel base={base} run={run} batch={paymentBatch} items={paymentItems} userId={actor.userId} canPrepare={hasPermission(actor,"payroll.payment.prepare")} canApprove={hasPermission(actor,"finance.pay")&&(paymentBatch?.retry_requested_by??paymentBatch?.created_by)!==actor.userId} canExport={hasPermission(actor,"payroll.payment.execute")} canConfirm={hasPermission(actor,"finance.pay")}/>}
      </section>
      <PayrollDisclosure title="Controlled corrections"><PayrollCorrectionControls appearance="standard" base={base} run={run} corrections={data.corrections} userId={actor.userId} canRequest={hasPermission(actor,"payroll.prepare")} canReview={hasPermission(actor,"payroll.approve")}/></PayrollDisclosure>
      {adjustments&&(run.status==='posted'||Boolean(run.adjustment_of_run_id))&&<PayrollAdjustmentControls base={base} run={run} items={data.items} data={adjustments} canPrepare={hasPermission(actor,'payroll.prepare')}/>}
      {hasPermission(actor,"payroll.read")&&<PayrollDisclosure title="Calculation policy and reports"><PayrollPolicySummary base={base} userId={actor.userId} run={run} exports={data.exports}/></PayrollDisclosure>}
      {hasPermission(actor,"payroll.read")&&<PayrollItemList userId={actor.userId} run={run} items={data.items} previousNet={previousNet}/>}
      {run.status==="posted"&&hasPermission(actor,"finance.pay")&&<PayrollLiabilitiesPanel initial={initialLiabilities} base={base} runId={id} version={Number(run.version)} userId={actor.userId}/>}
      {run.status==="posted"&&hasPermission(actor,"finance.pay")&&actor.companyId&&<PayrollOtherRemittancesPanel initial={initialOtherLiabilities} base={base} companyId={actor.companyId} runId={id} version={Number(run.version)} currency={String(run.currency)} periodEnd={String(run.period_end)}/>}
      {hasPermission(actor,"payroll.read")&&<Card as="section" appearance="standard" className="ui-panel"><PayrollHeading title="Payslips"/><PayrollNote text="Issued employee copies retain the posted calculation and source digest. Employees see only their own copies through My payslips."/>{payslips.length?<ul>{payslips.map(slip=><li key={slip.id}><Link href={canonicalUiHref(`${base}/people/payslips/${slip.id}`)}>{slip.employeeName} · issued revision {slip.runRevision}</Link></li>)}</ul>:<PayrollNote text="Copies are issued when the run is independently posted."/>}</Card>}
    <PayrollDisclosure title="Approval details and history"><FlowPanel flow={flow} actor={actor} kind="payroll" id={id} base={base}/></PayrollDisclosure></WidgetGrid></FeaturePage></PayrollEnvironment>
  );
}
