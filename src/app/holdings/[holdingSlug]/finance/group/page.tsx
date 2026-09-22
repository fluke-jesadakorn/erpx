import { FeaturePage } from "@/components/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@/components/finance/finance-workspace";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { groupFinanceWorkspace } from "@/server/finance-workspace";
import { IntercompanyConsole } from "@/components/finance/intercompany-console";

export const dynamic="force-dynamic";
export default async function GroupFinancePage({params}:{params:Promise<{holdingSlug:string}>}) {
  const actor=await requireWorkContext((await params).holdingSlug),base=workspaceBase(publicScope(actor)),data=await groupFinanceWorkspace(actor);
  return <FeaturePage className="content-stack" data-source="live"><FinanceWorkspace base={base} active="group" title="Group finance" description="Track intercompany items and consolidation runs without changing company ledgers">
    <section className={styles.process}><article><b>Lock company data</b><p className="muted">Use balances from closed periods and approved policies</p></article><article><b>Convert and match</b><p className="muted">Map accounts, translate currencies, and match intercompany items</p></article><article><b>Consolidation adjustments</b><p className="muted">Post eliminations, NCI, equity method, and acquisition adjustments</p></article><article><b>Review and publish</b><p className="muted">An approver reviews the snapshot and version differences</p></article></section>
    <div className={styles.twoColumn}><IntercompanyConsole base={base} data={data}/><section className={styles.panel}><div className={styles.sectionHead}><div><h2>Consolidation runs</h2><p className="muted">Each run stores a snapshot separate from company ledgers</p></div><span className="badge">{data.runs.length}</span></div>
      {data.runs.length?data.runs.map(run=><article className="row" key={String(run.id)}><div className="row-main"><strong>{String(run.as_of_date)}</strong><span className="muted">{String(run.presentation_currency)} · Version {String(run.version)}</span></div><span className={styles.status}>{String(run.status)}</span></article>):<p className={styles.empty}>No consolidation runs yet. The first run becomes available when every in-scope company has closed the period.</p>}
    </section></div>
    <section className={styles.panel}><h2>Prerequisites before calculation</h2><div className={styles.grid}>{["Ownership structure by effective date","Accounting method for each entity","Group chart of accounts and mapping","Exchange rates and presentation currency","Elimination and NCI policies","Acquisition and goodwill data"].map(item=><div className="card" key={item}>{item}</div>)}</div></section>
  </FinanceWorkspace></FeaturePage>;
}
