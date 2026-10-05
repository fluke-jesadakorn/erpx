import { RecordLink } from "@/components/records/record-link";

import { PageContent } from "@/components/ui/patterns";
import { PayrollDashboard } from "@/components/modules/payroll-workspace";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { pageError } from "@/server/page-error";
import { listPayrollRuns, payrollRunsPage, payrollSummary } from "@/server/people-payroll";
import { payrollReadiness } from "@/server/payroll-readiness";
import {Table,TableFrame} from "@/components/ui/foundation";
import {minorToDecimal} from "@/lib/finance-money";
import { Card } from "@/components/ui/foundation";

export const dynamic = "force-dynamic";

export default async function PayrollPage({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const redacted = !hasPermission(actor, "payroll.read");
  const summary=!redacted&&!actor.branchId&&!actor.departmentId?await payrollSummary(actor):[];
  let runs;
  try {
    runs = redacted ? await listPayrollRuns(actor) : (await payrollRunsPage(actor)).items;
  } catch (error) {
    return pageError(error, actor.holdingSlug);
  }
  const byPeriod = (a: Record<string, unknown>, b: Record<string, unknown>) =>
    String(a.period_end).localeCompare(String(b.period_end));
  const pending = runs
    .filter(
      (run) =>
        ["in_review", "approved"].includes(String(run.status)) ||
        (run.status === "posted" && run.payment_status !== "settled"),
    )
    .sort(byPeriod);
  const preparing = runs
    .filter((run) => ["draft", "calculating", "preview", "failed"].includes(String(run.status)))
    .sort((a, b) => byPeriod(b, a));
  const current = pending[0] ?? preparing[0] ?? runs[0];
  const readiness =
    !redacted && current
      ? await payrollReadiness(actor, String(current.id)).catch(() => null)
      : null;
  return (
    <PageContent  data-source="live">
      <PayrollDashboard
        base={workspaceBase(publicScope(actor))}
        userId={actor.userId}
        runs={runs}
        readiness={readiness}
        canPrepare={hasPermission(actor, "payroll.prepare")}
        redacted={redacted}
      />
      {!redacted&&!actor.branchId&&!actor.departmentId&&<Card as="section" appearance="standard" className="ui-panel"><h2>Posted payroll by pay month</h2><p className="muted">Company totals from posted payroll items. Corrections use their actual payment month and employee identities remain in the restricted run detail.</p><TableFrame appearance="standard" label="Monthly posted payroll"><Table><thead><tr><th>Pay month</th><th>Currency</th><th>Runs</th><th>People</th><th>Gross</th><th>Deductions</th><th>Employer cost</th><th>Net</th></tr></thead><tbody>{summary.map(row=><tr key={`${row.pay_month}-${row.currency}`}><td><RecordLink href={`${workspaceBase(publicScope(actor))}/record-details/payroll_month/${row.pay_month}`} record={{kind:"payroll_month",id:row.pay_month,args:{currency:row.currency}}}>{row.pay_month}</RecordLink></td><td>{row.currency}</td><td>{row.run_count}</td><td>{row.headcount}</td><td>{minorToDecimal(Number(row.gross_minor))}</td><td>{minorToDecimal(Number(row.deductions_minor))}</td><td>{minorToDecimal(Number(row.employer_cost_minor))}</td><td>{minorToDecimal(Number(row.net_minor))}</td></tr>)}</tbody></Table></TableFrame>{!summary.length&&<p>No payroll has been posted.</p>}</Card>}
    </PageContent>
  );
}
