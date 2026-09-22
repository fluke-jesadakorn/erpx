import { FeaturePage } from "@/components/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@/components/finance/finance-workspace";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { groupFinanceWorkspace } from "@/server/finance-workspace";
import { IntercompanyConsole } from "@/components/finance/intercompany-console";
import { ConsolidationRunConsole } from "@/components/finance/consolidation-run-console";

export const dynamic="force-dynamic";
export default async function GroupFinancePage({params}:{params:Promise<{holdingSlug:string}>}) {
  const actor=await requireWorkContext((await params).holdingSlug),base=workspaceBase(publicScope(actor)),data=await groupFinanceWorkspace(actor);
  return <FeaturePage className="content-stack" data-source="live"><FinanceWorkspace base={base} active="group" title="Group finance" description="Track intercompany items and consolidation runs without changing company ledgers">
    <section className={styles.process}><article><b>Lock company data</b><p className="muted">Use balances from closed periods and approved policies</p></article><article><b>Convert and match</b><p className="muted">Map accounts, translate currencies, and match intercompany items</p></article><article><b>Consolidation adjustments</b><p className="muted">Post eliminations, NCI, equity method, and acquisition adjustments</p></article><article><b>Review and publish</b><p className="muted">An approver reviews the snapshot and version differences</p></article></section>
    <div className={styles.twoColumn}><IntercompanyConsole base={base} data={data}/><ConsolidationRunConsole base={base} runs={data.runs.map(run => ({ id: String(run.id), presentation_currency: String(run.presentation_currency), as_of_date: String(run.as_of_date), status: String(run.status) as "draft" | "review" | "approved" | "published", source_hash: String(run.source_hash ?? ""), version: Number(run.version), created_by: String(run.created_by ?? ""), reviewed_by: run.reviewed_by ? String(run.reviewed_by) : null, approved_by: run.approved_by ? String(run.approved_by) : null, published_by: run.published_by ? String(run.published_by) : null }))} canWrite={data.canWrite} canApprove={data.canApprove}/></div>
    <section className={styles.panel}><h2>Prerequisites before calculation</h2><div className={styles.grid}>{["Ownership structure by effective date","Accounting method for each entity","Group chart of accounts and mapping","Exchange rates and presentation currency","Elimination and NCI policies","Acquisition and goodwill data"].map(item=><div className="card" key={item}>{item}</div>)}</div></section>
  </FinanceWorkspace></FeaturePage>;
}
