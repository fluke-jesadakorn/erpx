import { FeaturePage } from "@/components/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@/components/finance/finance-workspace";
import { JournalEditor } from "@/components/modules/ledger-controls";
import { ListPagination } from "@/components/ui/list-pagination";
import { StatusBadge } from "@/components/ui/primitives";
import { minorToDecimal } from "@/lib/finance-money";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { ledgerWorkspace } from "@/server/ledger";
import Link from "next/link";

export const dynamic = "force-dynamic";
export default async function AccountingPage({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{chart?:string;status?:string;cursor?:string}>}) {
  const actor=await requireWorkContext((await params).holdingSlug),base=workspaceBase(publicScope(actor)),query=await searchParams;
  const data=await ledgerWorkspace(actor,query.chart,query);
  return <FeaturePage className="content-stack" data-source="live"><FinanceWorkspace base={base} active="accounting" title="Accounting and journals" description={`Chart ${String(data.chart.name)} · Base currency ${String(data.chart.base_currency)}`} actions={hasPermission(actor,"finance.write")?<a className="button" href="#new-journal">Create journal</a>:undefined}>
    <form className="business-form"><label>Chart of accounts<select name="chart" defaultValue={String(data.chart.id)}>{data.charts.map(c=><option value={String(c.id)} key={String(c.id)}>{String(c.name)} · {String(c.base_currency)}</option>)}</select></label><label>Status<select name="status" defaultValue={query.status??"all"}><option value="all">All</option><option value="draft">Draft and review</option><option value="posted">Posted</option><option value="reversed">Reversed</option></select></label><button className="button secondary">Show entries</button></form>
    {hasPermission(actor,"finance.write")&&<section id="new-journal"><JournalEditor base={base} chart={data.chart} accounts={data.accounts} periods={data.periods}/></section>}
    <section className={styles.panel}><div className={styles.sectionHead}><div><h2>Journal entries</h2><p className="muted">Open an entry to review its preparer, evidence, and debit/credit impact</p></div><span className="badge">{data.journals.length}</span></div>
      <div className="business-table-wrap"><table className={styles.table}><thead><tr><th>Number</th><th>Date</th><th>Source</th><th>Debit</th><th>Credit</th><th>Status</th></tr></thead><tbody>{data.journals.map(j=><tr key={String(j.id)}><td><Link href={`${base}/finance/journals/${j.id}`}>{String(j.journal_number)}</Link></td><td>{String(j.entry_date)}</td><td>{String(j.source_type)}</td><td>{minorToDecimal(Number(j.debit))}</td><td>{minorToDecimal(Number(j.credit))}</td><td><StatusBadge status={String(j.status)}/></td></tr>)}</tbody></table></div>
      {!data.journals.length&&<p className={styles.empty}>No journals match these filters.</p>}{data.journalPage&&<ListPagination {...data.journalPage} query={query} count={data.journals.length}/>}</section>
  </FinanceWorkspace></FeaturePage>;
}
