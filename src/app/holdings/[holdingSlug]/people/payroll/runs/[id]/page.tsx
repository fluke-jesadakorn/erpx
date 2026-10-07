import { InlineNotice } from "@/components/ui/foundation";
import { BusinessDocumentBranding } from "@/components/brand/business-document";
import { RecordLink as Link } from "@/components/records/record-link";
import { FeaturePage, WidgetGrid } from "@/components/features/feature-page";
import { PayrollCorrectionControls } from "@/components/modules/payroll-controls";
import { PayrollCancellationControls } from "@/components/modules/payroll-cancellation-controls";
import { PayrollAdjustmentControls } from "@/components/modules/payroll-adjustment-controls";
import {payrollAdjustmentPanel} from "@/server/modules/people_payroll/public/payroll-adjustments";
import { PayrollAccountingPanel } from "@/components/modules/payroll-accounting-panel";
import { PayrollPaymentPanel } from "@/components/modules/payroll-payment-panel";
import { PayrollLiabilitiesPanel } from "@/components/modules/payroll-liabilities-panel";
import { PayrollDetailOverview, PayrollDisclosure, PayrollHeading, PayrollItemList, PayrollNote, PayrollPolicySummary, PayrollReadinessPanel } from "@/components/modules/payroll-workspace";
import { FlowPanel,documentFlow } from "@/components/workplace/flow-panel";
import { WorkReturn } from "@/components/workplace/work-return";
import { hasPermission } from "@/lib/permissions";
import { samePerson } from "@/lib/iam";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { getPayrollRun, listPayrollRuns, payrollPostingOptions } from "@/server/modules/people_payroll/public/people-payroll";
import { isThaiHotelPayrollMockEnabled } from "@/server/modules/runtime/public/simulation-only-adapters";
import {listPayslips} from "@/server/modules/people_payroll/public/payroll-payslips";
import { Card } from "@/components/ui/foundation";

export const dynamic = "force-dynamic";

export default async function PayrollRunDetail({ params, searchParams }: { params: Promise<{ holdingSlug: string; id: string }>; searchParams: Promise<{ print?: string; brand?: string }> }) {
  const { holdingSlug, id } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const simulationPayroll = isThaiHotelPayrollMockEnabled(actor.holdingSlug);
  const query = await searchParams, printing = query.print === "1";
  let data;
  try { data = await getPayrollRun(actor, id); } catch (error) { return pageError(error, actor.holdingSlug); }
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
  return (
    <FeaturePage className="ui-page-content" data-source="live" data-print-document={printing || undefined}><WorkReturn base={base}/>
      <BusinessDocumentBranding actor={actor} branchId={null} requestedMode={query.brand} title="Payroll run" reference={`${String(run.period_start)} – ${String(run.period_end)}`} printing={printing}/>
      <PayrollDetailOverview base={base} userId={actor.userId} run={run} itemCount={data.items.length} netPayableMinor={paymentBatch ? Number(paymentBatch.total_minor) : undefined} paymentStatus={run.status==="posted"&&paymentBatch ? String(paymentBatch.status) : undefined} redacted={!hasPermission(actor,"payroll.read")}/>
      {simulationPayroll&&<p className="notice" role="note" data-payroll-adapter="mock">Simulation only: deterministic fixture results; not a Thai statutory calculation. No real payment is made.</p>}
      <WidgetGrid pageKey="people-payroll-detail">
      <section id="payroll-run-work" aria-label="Payroll stage actions">
        {run.status==="cancelled"&&<InlineNotice tone="info" className="notice" role="status">Payroll run cancelled. Original source evidence and review copies remain in history.</InlineNotice>}
        {canCancel&&<PayrollCancellationControls base={base} runId={id} version={Number(run.version)}/>}
        {hasPermission(actor,"payroll.read")&&["draft","calculating","preview","failed","in_review"].includes(String(run.status))&&<PayrollReadinessPanel base={base} runId={id} version={Number(run.version)} userId={actor.userId} canPrepare={hasPermission(actor,"payroll.prepare")} canApprove={hasPermission(actor,"payroll.approve")&&Boolean(run.submitted_by)&&run.submitted_by!==actor.userId} canExport={hasPermission(actor,"payroll.read")} simulationMode={simulationPayroll}/>}
        {run.status==="approved"&&hasPermission(actor,"finance.approve")&&<PayrollAccountingPanel base={base} run={run} options={postingOptions} userId={actor.userId}/>}
        {run.status==="posted"&&(hasPermission(actor,"payroll.payment.read")||hasPermission(actor,"payroll.payment.prepare")||hasPermission(actor,"finance.pay"))&&<PayrollPaymentPanel base={base} run={run} batch={paymentBatch} items={paymentItems} userId={actor.userId} canPrepare={hasPermission(actor,"payroll.payment.prepare")} canApprove={hasPermission(actor,"finance.pay")&&(paymentBatch?.retry_requested_by??paymentBatch?.created_by)!==actor.userId} canExport={hasPermission(actor,"payroll.payment.execute")} canConfirm={hasPermission(actor,"finance.pay")}/>}
      </section>
      <PayrollDisclosure title="Controlled corrections"><PayrollCorrectionControls appearance="standard" base={base} run={run} corrections={data.corrections} userId={actor.userId} canRequest={hasPermission(actor,"payroll.prepare")} canReview={hasPermission(actor,"payroll.approve")}/></PayrollDisclosure>
      {adjustments&&(run.status==='posted'||Boolean(run.adjustment_of_run_id))&&<PayrollAdjustmentControls base={base} run={run} items={data.items} data={adjustments} canPrepare={hasPermission(actor,'payroll.prepare')}/>}
      {hasPermission(actor,"payroll.read")&&<PayrollDisclosure title="Calculation policy and reports"><PayrollPolicySummary base={base} userId={actor.userId} run={run} exports={data.exports}/></PayrollDisclosure>}
      {hasPermission(actor,"payroll.read")&&<PayrollItemList userId={actor.userId} run={run} items={data.items} previousNet={previousNet}/>}
      {run.status==="posted"&&hasPermission(actor,"finance.pay")&&<PayrollLiabilitiesPanel base={base} runId={id} version={Number(run.version)} userId={actor.userId}/>}
      {hasPermission(actor,"payroll.read")&&<Card as="section" appearance="standard" className="ui-panel"><PayrollHeading title="Payslips"/><PayrollNote text="Issued employee copies retain the posted calculation and source digest. Employees see only their own copies through My payslips."/>{payslips.length?<ul>{payslips.map(slip=><li key={slip.id}><Link href={`${base}/people/payslips/${slip.id}`}>{slip.employeeName} · issued revision {slip.runRevision}</Link></li>)}</ul>:<PayrollNote text="Copies are issued when the run is independently posted."/>}</Card>}
    <PayrollDisclosure title="Approval details and history"><FlowPanel flow={flow} actor={actor} kind="payroll" id={id} base={base}/></PayrollDisclosure></WidgetGrid></FeaturePage>
  );
}
