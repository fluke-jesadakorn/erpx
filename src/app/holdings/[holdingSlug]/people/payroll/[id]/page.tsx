import { hasPermission } from "@/lib/permissions";
import { pageError } from "@/server/page-error";
import { FlowPanel, documentFlow } from "@/components/workplace/flow-panel";
import Link from "next/link";
import { PayrollActions } from "@/components/modules/payroll-controls";
import { ArrowLeft } from "lucide-react";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { getPayrollRun } from "@/server/people-payroll";
import { workspaceBase } from "@/lib/workspace";
import { minorToDecimal } from "@/lib/finance-money";
import { StatusBadge } from "@/components/ui/primitives";

export const dynamic = "force-dynamic";

export default async function PayrollRunDetail({ params }: { params: Promise<{ holdingSlug: string; id: string }> }) {
  const { holdingSlug, id } = await params;
  const actor = await requireWorkContext(holdingSlug);
  let data;
  try { data = await getPayrollRun(actor, id); } catch (error) { return pageError(error, actor.holdingSlug); }
  const base = workspaceBase(publicScope(actor));
  const run = data.run as Record<string, unknown>;
  const totals = JSON.parse(String(run.totals_json ?? "{}")) as { gross?: number; deductions?: number; employer?: number; net?: number };
  const flow = await documentFlow(actor,"payroll",id);
  return (
    <main className="content-stack" data-source="live">
      <Link className="back-link" href={`${base}/people`}><ArrowLeft size={14} /> Back to people</Link>
      <div className="section-head"><div><p className="eyebrow">People & payroll · run detail</p><h1>{String(run.period_start)} → {String(run.period_end)}</h1><p className="muted">{String(run.currency)} · pay date {String(run.pay_date)} · {String(run.mode)}</p></div><StatusBadge status={String(run.status)} /></div>
      <section className="card-grid" aria-label="Payroll totals">
        {[["Gross", totals.gross], ["Deductions", totals.deductions], ["Employer cost", totals.employer], ["Net pay", totals.net]].map(([label, value]) => <article className="card" key={String(label)}><span className="eyebrow">{String(label)}</span><strong>{String(run.currency)} {minorToDecimal(Number(value ?? 0))}</strong></article>)}
      </section>
      <PayrollActions base={base} run={run} canCalculate={flow.steps.find(s=>s.key==="calculate")?.primary?.userId===actor.userId&&hasPermission(actor,"payroll.prepare")} canApprove={flow.canAct&&hasPermission(actor,"payroll.approve")&&run.created_by!==actor.userId}/>
      <section className="card">
        <div className="section-head"><h2>Calculated items</h2><span className="badge">{data.items.length}</span></div>
        {data.items.map((item) => <div className="row" key={String(item.id)}><div className="row-main"><strong className="row-title">{String(item.legal_name)}</strong><span className="muted">{String(item.worker_type)} · {String(item.status)}</span></div><span>{String(item.currency)} {minorToDecimal(Number(item.net_minor))} net</span></div>)}
      </section>
    <FlowPanel flow={flow} actor={actor} kind="payroll" id={id} base={base}/></main>
  );
}
