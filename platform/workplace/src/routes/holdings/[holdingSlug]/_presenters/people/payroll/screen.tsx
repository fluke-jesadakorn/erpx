import {peopleFinancialSourcePresentation} from "@erpx/people-payroll-server/public/financial-product-presentation-request";
import {PayrollEnvironment} from "@erpx/people-payroll-ui/components/payroll-environment";
import {payrollPurchaseControls} from "@erpx/people-payroll-server/public/payroll-purchases";
import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { RecordLink } from "@erpx/records-ui/components/record-link";

import { PageContent } from "@erpx/platform-ui/components/patterns";
import { PayrollDashboard } from "@erpx/people-payroll-ui/components/payroll-workspace";
import { hasPermission } from "@erpx/identity-access-ui/lib/permissions";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { pageError } from "@erpx/work-projection-server/public/page-error";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { ownedPeoplePayrollRuns, ownedPeoplePayrollSummary } from "@erpx/people-payroll-ui/public/native-payroll-summary-owned";
import type { NativePayrollSummaryRow } from "@erpx/people-payroll-ui/public/native-payroll-summary";
import { payrollReadiness } from "@erpx/people-payroll-server/public/payroll-readiness";
import {Table,TableFrame} from "@erpx/platform-ui/components/foundation";
import {minorToDecimal} from "@erpx/finance-ui/lib/finance-money";
import { Card } from "@erpx/platform-ui/components/foundation";

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
  let summary: NativePayrollSummaryRow[] = [];
  let runs;
  try {
    const summaryRead = !redacted && !actor.branchId && !actor.departmentId
      ? await ownedPeoplePayrollSummary(actor, holdingSlug)
      : [];
    const runsRead = await ownedPeoplePayrollRuns(actor, holdingSlug);
    if (summaryRead === undefined || runsRead === undefined)
      throw new ApiError(503, "NATIVE_PEOPLE_PAYROLL_UNAVAILABLE", "The native People Payroll reader is unavailable.");
    summary = summaryRead;
    runs = runsRead;
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
