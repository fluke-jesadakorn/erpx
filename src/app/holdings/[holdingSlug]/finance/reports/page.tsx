import { Button, Field, Select, Table, TableFrame, TextInput } from "@/components/ui/foundation";
import { FeaturePage } from "@/components/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@/components/finance/finance-workspace";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { reportDateSchema } from "@/server/finance";
import { ledgerWorkspace } from "@/server/ledger";
import Link from "next/link";
import { FinanceReportingConsole } from "@/components/modules/finance-reporting-console";
import { reportingWorkspace } from "@/server/finance-reporting";
import { hasPermission } from "@/lib/permissions";
import { FormLayout } from "@/components/ui/foundation";

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
  const selectedPeriod = data.periods.find((period) => period.id === query.periodId);
  const accountingHref = `${base}/finance/accounting?chart=${encodeURIComponent(String(data.chart.id))}#journal-entries`;
  const cards = [
    ["Assets", report.balanceSheet.assets],
    ["Liabilities", report.balanceSheet.liabilities],
    ["Equity", report.balanceSheet.equity],
    ["Revenue", report.profitAndLoss.revenue],
    ["Expenses", report.profitAndLoss.expenses],
  ];
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <FinanceWorkspace
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
              <Link href={accountingHref}>Trace journal entries →</Link>
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
                      {String(row.code)} · {String(row.name)}
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
        {reporting && <FinanceReportingConsole base={base} chartId={String(data.chart.id)} currency={String(data.chart.base_currency)} data={reporting} periods={data.periods} userId={actor.userId} canPrepare={hasPermission(actor,"finance.write")} canApprove={hasPermission(actor,"finance.approve")}/>}
      </FinanceWorkspace>
    </FeaturePage>
  );
}
