import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { Button, Field, Select, Table, TableFrame, TextInput } from "@erpx/ui/components/foundation";
import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@erpx/feature-finance/components/finance-workspace";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import {reportDateSchema} from '@erpx/server-finance/public/finance';
import { ledgerWorkspace } from "@erpx/server-finance/public/ledger";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { StandardFinanceReportingConsole } from "@erpx/feature-finance/components/standard-finance-reporting-console";
import { reportingWorkspace } from "@erpx/server-finance/public/finance-reporting";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { FormLayout } from "@erpx/ui/components/foundation";
import { financePlanningWorkspace } from "@erpx/server-finance/public/finance-planning";
import { FinancePlanningControls } from "@erpx/feature-finance/components/finance-planning-controls";
import { financeCompanyTools } from "@erpx/server-finance/public/finance-company-tools";

export const dynamic = "force-dynamic";
export default async function ReportsPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{
    chart?: string;
    startDate?: string;
    endDate?: string;
    periodId?: string;
  }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor)),
    query = await searchParams,
    data = await ledgerWorkspace(actor, query.chart, {
      print: "1",
      periodId: query.periodId || undefined,
      reportDates: reportDateSchema.parse({
        startDate: query.startDate || undefined,
        endDate: query.endDate || undefined,
      }),
    }),
    report = data.report;
  const reporting = !actor.branchId && !actor.departmentId ? await reportingWorkspace(actor,String(data.chart.id)) : null;
  const planning = !actor.branchId && !actor.departmentId ? await financePlanningWorkspace(actor,String(data.chart.id)) : null;
  const planningEnabled=(await financeCompanyTools(actor)).some(r=>r.tool==="planning"&&r.enabled===1);
  const selectedPeriod = data.periods.find((period) => period.id === query.periodId);
  const accountingHref = canonicalUiHref(`${base}/finance/accounting?chart=${encodeURIComponent(String(data.chart.id))}#journal-entries`);
  const cards = [
    ["Assets", report.balanceSheet.assets],
    ["Liabilities", report.balanceSheet.liabilities],
    ["Equity", report.balanceSheet.equity],
    ["Revenue", report.profitAndLoss.revenue],
    ["Expenses", report.profitAndLoss.expenses],
  ];
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <FinanceWorkspace actor={actor}
        base={base}
        context={{
          chartId: String(data.chart.id),
          chartName: String(data.chart.name),
          currency: String(data.chart.base_currency),
        }}
        active="reports"
        title="Reports and statements"
        description={`${String(data.chart.name)} · ${String(data.chart.base_currency)}`}
      >
        <FormLayout className="ui-form-grid">
          <Field appearance="standard">
            Chart of accounts
            <Select appearance="standard" name="chart" defaultValue={String(data.chart.id)}>
              {data.charts.map((chart) => (
                <option value={String(chart.id)} key={String(chart.id)}>
                  {String(chart.name)} · {String(chart.base_currency)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Accounting period
            <Select appearance="standard" name="periodId" defaultValue={query.periodId ?? ""}>
              <option value="">Custom date range</option>
              {data.periods.map((p) => (
                <option key={String(p.id)} value={String(p.id)}>
                  {String(p.name)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Start date
            <TextInput appearance="standard" name="startDate" type="date" defaultValue={query.startDate} />
          </Field>
          <Field appearance="standard">
            End date
            <TextInput appearance="standard" name="endDate" type="date" defaultValue={query.endDate} />
          </Field>
          <Button appearance="standard" type="submit" variant="secondary">
            Show report
          </Button>
        </FormLayout>
        <p className="muted">
          {selectedPeriod
            ? `Accounting period: ${selectedPeriod.name} · ${selectedPeriod.start_date}–${selectedPeriod.end_date}. Date fields apply only to Custom date range.`
            : `Posted evidence: ${query.startDate ?? "All dates"} through ${query.endDate ?? "latest posted date"}. Choose an accounting period or a custom date range.`}
        </p>
        <section className={styles.reportCards}>
          {cards.map(([label, value]) => (
            <article className={`card ${styles.reportCard}`} key={label}>
              <span className="eyebrow">{label}</span>
              <strong>
                {String(data.chart.base_currency)} {String(value)}
              </strong>
              <Link href={canonicalUiHref(`${base}/record-details/finance_report/${String(label).toLowerCase()}`)} record={{kind:"finance_report",id:String(label).toLowerCase(),args:{chartId:String(data.chart.id),...(report.dates.startDate?{startDate:report.dates.startDate}:{}),...(report.dates.endDate?{endDate:report.dates.endDate}:{})}}}>Trace journal entries →</Link>
            </article>
          ))}
        </section>
        <section className={styles.panel} id="trial-balance">
          <div className={styles.sectionHead}>
            <div>
              <h2>Trial balance</h2>
              <p className="muted">
                Posted entries for the selected dates. Balance sheet figures accumulate to the end
                date; profit and loss covers the selected range.
              </p>
            </div>
            <Link href={accountingHref}>Open Accounting →</Link>
          </div>
          <TableFrame appearance="standard"
            label="Records"
            className="business-table-wrap"
            tabIndex={0}
            role="region"
            aria-label="Financial table — scroll for more columns"
          >
            <Table className={styles.table}>
              <thead>
                <tr>
                  <th>Account</th>
                  <th>Category</th>
                  <th>Debit</th>
                  <th>Credit</th>
                  <th>Balance</th>
                </tr>
              </thead>
              <tbody>
                {report.trialBalance.map((row) => (
                  <tr key={String(row.id)}>
                    <td>
                      <Link href={canonicalUiHref(`${base}/record-details/trial_balance/${encodeURIComponent(String(row.id))}`)} record={{kind:"trial_balance",id:String(row.id),args:{chartId:String(data.chart.id),...(report.dates.startDate?{startDate:report.dates.startDate}:{}),...(report.dates.endDate?{endDate:report.dates.endDate}:{})}}}>{String(row.code)} · {String(row.name)}</Link>
                    </td>
                    <td>{String(row.category)}</td>
                    <td>{String(row.debit)}</td>
                    <td>{String(row.credit)}</td>
                    <td>{String(row.balance)}</td>
                  </tr>
                ))}
              </tbody>
            </Table>
          </TableFrame>
        </section>
        {reporting && <StandardFinanceReportingConsole base={base} companyId={actor.companyId} chartId={String(data.chart.id)} currency={String(data.chart.base_currency)} data={reporting} periods={data.periods} userId={actor.userId} canPrepare={hasPermission(actor,"finance.write")} canApprove={hasPermission(actor,"finance.approve")}/>}
        {planning&&(planningEnabled||planning.editions.length>0)&&<FinancePlanningControls base={base} chartId={String(data.chart.id)} data={planning} userId={actor.userId} canWrite={hasPermission(actor,"finance.write")&&planningEnabled} canApprove={hasPermission(actor,"finance.approve")}/>}
      </FinanceWorkspace>
    </FeaturePage>
  );
}
