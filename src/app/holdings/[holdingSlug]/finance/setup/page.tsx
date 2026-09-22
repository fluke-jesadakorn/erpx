import Link from "next/link";
import { FeaturePage } from "@/components/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@/components/finance/finance-workspace";
import { FinanceChartControls, type FinanceChart, type FinanceAccount } from "@/components/modules/finance-chart-controls";
import { OpeningBalanceImport } from "@/components/modules/ledger-controls";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { financeSetupWorkspace } from "@/server/finance-workspace";
import { ledgerWorkspace } from "@/server/ledger";

export const dynamic="force-dynamic";
export default async function FinanceSetupPage({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams?:Promise<Record<string,string|string[]|undefined>>}) {
  const {holdingSlug}=await params,query=searchParams?await searchParams:{},requestedChart=typeof query.chart==="string"?query.chart:null;
  const actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));
  const setup=await financeSetupWorkspace(actor),ledger=setup.charts.length?await ledgerWorkspace(actor,requestedChart??undefined):null;
  const complete=setup.checks.filter(check=>check.complete).length;
  const selectedChart=ledger?.chart as FinanceChart | undefined;
  const chartAccounts=ledger?.accounts as FinanceAccount[] | undefined;
  return <FeaturePage className="content-stack" data-source="live"><FinanceWorkspace base={base} active="setup" title="Finance setup" description="Prepare the company before receiving documents and posting">
    <section className={styles.hero}><div><p className="eyebrow">Readiness</p><strong>{complete}/{setup.checks.length}</strong><p>Key setup checks found by the system</p></div><aside><b>{complete===setup.checks.length?"Ready to test the workflow":"Setup still required"}</b><span className="muted">Go-live requires a close simulation and accountable data review.</span></aside></section>
    <section className={styles.panel}><h2>Getting-started checklist</h2>{setup.checks.map(check=><div className={styles.check} data-complete={check.complete} key={check.id}><span className={styles.checkMark}>{check.complete?"✓":"·"}</span><b>{check.label}</b><span className="muted">{check.detail}</span></div>)}</section>
    {setup.charts.length>1&&<section className={styles.panel} aria-label="Finance chart selector"><h2>Opening balance chart</h2><p className="muted">Choose the chart that owns the opening balances. The selected chart controls the accounts and periods shown below.</p><div className="form-actions">{setup.charts.map(chart=><Link className="button secondary" data-active={ledger?.chart.id===chart.id||undefined} href={`${base}/finance/setup?chart=${encodeURIComponent(String(chart.id))}`} key={String(chart.id)}>{String(chart.name)} · {String(chart.base_currency)}</Link>)}</div></section>}
    {hasPermission(actor,"finance.write")&&selectedChart&&chartAccounts&&<FinanceChartControls base={base} chart={selectedChart} charts={setup.charts as FinanceChart[]} accounts={chartAccounts}/>} 
    {hasPermission(actor,"finance.write")&&ledger&&<OpeningBalanceImport base={base} chart={ledger.chart} accounts={ledger.accounts} periods={ledger.periods} parties={setup.parties}/>} 
    <section className={styles.panel}><h2>Go-live policy</h2><p className="muted">Define independent preparers and approvers, Thai tax settings, bank accounts, mappings from PMS, POS, service, and manufacturing systems, and close owners. A chart of accounts alone does not mean the company is ready for production.</p></section>
  </FinanceWorkspace></FeaturePage>;
}
