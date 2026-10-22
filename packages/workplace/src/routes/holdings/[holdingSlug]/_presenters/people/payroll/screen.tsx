import {peopleFinancialSourcePresentation} from "@erpx/server-people_payroll/public/financial-product-presentation-request";
import {PayrollEnvironment} from "@erpx/feature-people_payroll/components/payroll-environment";
import {payrollPurchaseControls} from "@erpx/server-people_payroll/public/payroll-purchases";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordLink } from "@erpx/feature-records/components/record-link";

import { PageContent } from "@erpx/ui/components/patterns";
import { PayrollDashboard } from "@erpx/feature-people_payroll/components/payroll-workspace";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { listPayrollRuns, payrollRunsPage, payrollSummary } from "@erpx/server-people_payroll/public/people-payroll";
import { payrollReadiness } from "@erpx/server-people_payroll/public/payroll-readiness";
import {Table,TableFrame} from "@erpx/ui/components/foundation";
import {minorToDecimal} from "@erpx/feature-finance/lib/finance-money";
import { Card } from "@erpx/ui/components/foundation";

export const dynamic = "force-dynamic";

export default async function PayrollPage({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const purchase = await payrollPurchaseControls(actor);
  const financialSource = await peopleFinancialSourcePresentation(actor,"payroll");
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
    <PayrollEnvironment purchase={purchase}><PageContent  data-source="live">
      <PayrollDashboard
        base={workspaceBase(publicScope(actor))}
        userId={actor.userId}
        runs={runs}
        readiness={readiness}
        canPrepare={!financialSource && hasPermission(actor, "payroll.prepare") && purchase?.newWork !== false}
        redacted={redacted}
      />
      {!redacted&&!actor.branchId&&!actor.departmentId&&<Card as="section" appearance="standard" className="ui-panel"><h2>Posted payroll by pay month</h2><p className="muted">Company totals from posted payroll items. Corrections use their actual payment month and employee identities remain in the restricted run detail.</p><TableFrame appearance="standard" label="Monthly posted payroll"><Table><thead><tr><th>Pay month</th><th>Currency</th><th>Runs</th><th>People</th><th>Gross</th><th>Deductions</th><th>Employer cost</th><th>Net</th></tr></thead><tbody>{summary.map(row=><tr key={`${row.pay_month}-${row.currency}`}><td><RecordLink href={canonicalUiHref(`${workspaceBase(publicScope(actor))}/record-details/payroll_month/${row.pay_month}`)} record={{kind:"payroll_month",id:row.pay_month,args:{currency:row.currency}}}>{row.pay_month}</RecordLink></td><td>{row.currency}</td><td>{row.run_count}</td><td>{row.headcount}</td><td>{minorToDecimal(Number(row.gross_minor))}</td><td>{minorToDecimal(Number(row.deductions_minor))}</td><td>{minorToDecimal(Number(row.employer_cost_minor))}</td><td>{minorToDecimal(Number(row.net_minor))}</td></tr>)}</tbody></Table></TableFrame>{!summary.length&&<p>No payroll has been posted.</p>}</Card>}
    </PageContent></PayrollEnvironment>
  );
}
