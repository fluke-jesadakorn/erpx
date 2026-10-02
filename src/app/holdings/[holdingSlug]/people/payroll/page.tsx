import { PayrollDashboard } from "@/components/modules/payroll-workspace";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { pageError } from "@/server/page-error";
import { listPayrollRuns, payrollRunsPage } from "@/server/people-payroll";
import { payrollReadiness } from "@/server/payroll-readiness";
import { runtimeEnv } from "@/server/runtime";

export const dynamic = "force-dynamic";

export default async function PayrollPage({ params }: { params: Promise<{ holdingSlug: string }> }) {
  const { holdingSlug } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const redacted = !hasPermission(actor, "payroll.read");
  let runs;
  try { runs = redacted ? await listPayrollRuns(actor) : (await payrollRunsPage(actor)).items; }
  catch (error) { return pageError(error, actor.holdingSlug); }
  const byPeriod = (a: Record<string, unknown>, b: Record<string, unknown>) => String(a.period_end).localeCompare(String(b.period_end));
  const pending = runs.filter(run => ["in_review", "approved"].includes(String(run.status)) || (run.status === "posted" && run.payment_status !== "settled")).sort(byPeriod);
  const preparing = runs.filter(run => ["draft", "calculating", "preview", "failed"].includes(String(run.status))).sort((a, b) => byPeriod(b, a));
  const current = pending[0] ?? preparing[0] ?? runs[0];
  const readiness=!redacted&&current?await payrollReadiness(actor,String(current.id)).catch(()=>null):null;
  return <main className="content" data-source="live">
    <PayrollDashboard base={workspaceBase(publicScope(actor))} userId={actor.userId}
      runs={runs} readiness={readiness} canPrepare={hasPermission(actor, "payroll.prepare")}
      redacted={redacted} production={(await runtimeEnv()).ERPX_ENVIRONMENT === "production"}/>
  </main>;
}
