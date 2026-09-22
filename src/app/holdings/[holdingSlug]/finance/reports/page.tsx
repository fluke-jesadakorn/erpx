import { FeaturePage } from "@/components/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@/components/finance/finance-workspace";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { ledgerWorkspace } from "@/server/ledger";
import Link from "next/link";

export const dynamic="force-dynamic";
export default async function ReportsPage({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{chart?:string}>}) {
  const actor=await requireWorkContext((await params).holdingSlug),base=workspaceBase(publicScope(actor)),query=await searchParams,data=await ledgerWorkspace(actor,query.chart,{print:"1"}),report=data.report;
  const accountingHref=`${base}/finance/accounting?chart=${encodeURIComponent(String(data.chart.id))}#journal-entries`;
  const cards=[["Assets",report.balanceSheet.assets],["Liabilities",report.balanceSheet.liabilities],["Equity",report.balanceSheet.equity],["Revenue",report.profitAndLoss.revenue],["Expenses",report.profitAndLoss.expenses]];
  return <FeaturePage className="content-stack" data-source="live"><FinanceWorkspace base={base} active="reports" title="Reports and statements" description={`Company ${actor.companyId} · ${String(data.chart.name)} · ${String(data.chart.base_currency)}`}>
    <form className="business-form"><label>Chart of accounts<select name="chart" defaultValue={String(data.chart.id)}>{data.charts.map(chart=><option value={String(chart.id)} key={String(chart.id)}>{String(chart.name)} · {String(chart.base_currency)}</option>)}</select></label><button className="button secondary">Show report</button></form>
    <section className={styles.grid}>{cards.map(([label,value])=><article className="card" key={label}><span className="eyebrow">{label}</span><strong>{String(data.chart.base_currency)} {String(value)}</strong><Link href={accountingHref}>Trace journal entries →</Link></article>)}</section>
    <section className={styles.panel} id="trial-balance"><div className={styles.sectionHead}><div><h2>Trial balance</h2><p className="muted">Posted journal totals. Each report total links to the same chart’s supporting journal entries.</p></div><Link href={accountingHref}>Open Accounting →</Link></div><div className="business-table-wrap"><table className={styles.table}><thead><tr><th>Account</th><th>Category</th><th>Debit</th><th>Credit</th><th>Balance</th></tr></thead><tbody>{report.trialBalance.map(row=><tr key={String(row.id)}><td>{String(row.code)} · {String(row.name)}</td><td>{String(row.category)}</td><td>{String(row.debit)}</td><td>{String(row.credit)}</td><td>{String(row.balance)}</td></tr>)}</tbody></table></div></section>
  </FinanceWorkspace></FeaturePage>;
}
