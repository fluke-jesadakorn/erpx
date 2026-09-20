import { BusinessDocumentBranding } from "@/components/brand/business-document";
import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { PayrollActions } from "@/components/modules/payroll-controls";
import { StatusBadge } from "@/components/ui/primitives";
import { FlowPanel,documentFlow } from "@/components/workplace/flow-panel";
import { RecordNextAction } from "@/components/workplace/record-next-action";
import { WorkReturn } from "@/components/workplace/work-return";
import { minorToDecimal } from "@/lib/finance-money";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { pageError } from "@/server/page-error";
import { getPayrollRun } from "@/server/people-payroll";
import { ArrowLeft } from "lucide-react";
import Link from "next/link";

export const dynamic = "force-dynamic";

export default async function PayrollRunDetail({ params, searchParams }: { params: Promise<{ holdingSlug: string; id: string }>; searchParams: Promise<{ print?: string; brand?: string }> }) {
  const { holdingSlug, id } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const query = await searchParams, printing = query.print === "1";
  let data;
  try { data = await getPayrollRun(actor, id); } catch (error) { return pageError(error, actor.holdingSlug); }
  const base = workspaceBase(publicScope(actor));
  const run = data.run as Record<string, unknown>;
  const totals = JSON.parse(String(run.totals_json ?? "{}")) as { gross?: number; deductions?: number; employer?: number; net?: number };
  const flow = await documentFlow(actor,"payroll",id);
  return (
    <FeaturePage className="content-stack" data-source="live" data-print-document={printing || undefined}><WorkReturn base={base}/>
      <BusinessDocumentBranding actor={actor} branchId={null} requestedMode={query.brand} title="Payroll run" reference={`${String(run.period_start)} – ${String(run.period_end)}`} printing={printing}/>
      <Link className="back-link" href={`${base}/people`}><ArrowLeft size={14} /> Back to people</Link>
      <div className="section-head"><div><p className="eyebrow">People & payroll · run detail</p><h1>{String(run.period_start)} → {String(run.period_end)}</h1><p className="muted">{String(run.currency)} · pay date {String(run.pay_date)} · {String(run.mode)}</p></div><StatusBadge status={String(run.status)} /></div><FeatureTabs/><WidgetGrid pageKey="people-payroll-detail">
      <section data-critical-notice><RecordNextAction flow={flow}/></section><section className="card-grid" aria-label="Payroll totals">
        {[["Gross", totals.gross], ["Deductions", totals.deductions], ["Employer cost", totals.employer], ["Net pay", totals.net]].map(([label, value]) => <article className="card" key={String(label)}><span className="eyebrow">{String(label)}</span><strong>{String(run.currency)} {minorToDecimal(Number(value ?? 0))}</strong></article>)}
      </section>
      <PayrollActions base={base} run={run} canCalculate={flow.steps.find(s=>s.key==="calculate")?.primary?.userId===actor.userId&&hasPermission(actor,"payroll.prepare")} canApprove={flow.canAct&&hasPermission(actor,"payroll.approve")&&run.created_by!==actor.userId} canExport={hasPermission(actor,"payroll.read")}/>
      <FeatureWidget id="people-payroll-detail-calculation-policy" title="Calculation policy"><section className="card"><p><strong>Overtime {Number(run.overtime_multiplier_bps)/10000}×</strong> · Proration {String(run.proration_method).replaceAll("_"," ")} · Input revision {String(run.revision)}</p><p className="muted">These are company payroll-core settings, not a statutory country pack.</p>{data.exports.map(item=><Link className="button secondary" key={String(item.id)} href={`/api/v1${base}/people/payroll/runs/${run.id}/review-file?revision=${item.run_revision}`}>Download review revision {String(item.run_revision)}</Link>)}</section></FeatureWidget>
      <FeatureWidget id="people-payroll-detail-calculated-items" title="Calculated items">
        <div className="section-head"><h2>Calculated items</h2><span className="badge">{data.items.length}</span></div>
        {data.items.map((item) => <div className="row" key={String(item.id)}><div className="row-main"><strong className="row-title">{String(item.legal_name)}</strong><span className="muted">{String(item.worker_type)} · {String(item.status)}</span></div><span>{String(item.currency)} {minorToDecimal(Number(item.net_minor))} net</span></div>)}
      </FeatureWidget>
    <FlowPanel flow={flow} actor={actor} kind="payroll" id={id} base={base}/></WidgetGrid></FeaturePage>
  );
}
